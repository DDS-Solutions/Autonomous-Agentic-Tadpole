//! @docs ARCHITECTURE:Runner
//!
//! ### AI Assist Note
//! **Zero-Trust Tool Execution Pipeline**: Orchestrates the strict pre-execution and
//! runtime security gates (`Budget -> CapabilityToken -> Safe/Observe Mode -> WAL ->
//! SecurityManager -> Aletheia Verification Gate -> HITL Oversight -> 3-Tier Dispatch
//! with Bounded Timeouts & Transient Retries`).
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Budget ceiling hit, expired `CapabilityToken`, Aletheia rejection, oversight denial, or tool timeout.
//! - **Telemetry Link**: Search `[pipeline]` or `ToolExecution` spans in tracing logs.

use super::capability::{CapabilityToken, ZeroTrustGuard};
use super::error::ToolExecutionError;
use super::security::{DefaultSecurityManager, SecurityManager, ValidationResult};
use super::{extract_target_path, normalize_tool_name, ToolContext};
use crate::agent::runner::{AgentRunner, RunContext};
use crate::agent::verification_gate::{MutationProposal, VerificationDecision};
use crate::error::AppError;
use tracing::Instrument;

impl AgentRunner {
    /// Dispatches a function call to the appropriate tool handler.
    /// Orchestrates the Zero-Trust pipeline: WAL -> CBS -> Audit -> Execute.
    pub(crate) async fn execute_tool(
        &self,
        ctx: &RunContext,
        fc: &crate::agent::types::ToolCall,
        output_text: &mut String,
        usage: &mut Option<crate::agent::types::TokenUsage>,
        user_message: &str,
    ) -> Result<Option<String>, AppError> {
        // 1. Mint Capability Token for this specific call
        let token = ZeroTrustGuard::mint_token(&ctx.agent_id, &ctx.mission_id, ctx.authority_level);

        match self
            .run_zero_trust_pipeline(ctx, fc, usage, user_message, token)
            .await
        {
            Ok(output) => {
                *output_text = output.clone();
                Ok(Some(output))
            }
            Err(e) => {
                let recovery = e.recovery_strategy();
                let hint = e.remediation_hint();
                let error_msg = format!(
                    "(TOOL FAILURE: {} | RECOVERY: {:?} | REMEDIATION: {})",
                    e, recovery, hint
                );
                *output_text = error_msg;

                // Surface recoverable tool failures to the agent loop
                Ok(Some(output_text.clone()))
            }
        }
    }

    /// Validates synchronous pre-execution security guards:
    /// 0. Fast-path budget ceiling
    /// 1. CapabilityToken ownership and expiration
    ///    1b. Safe-mode / Observe-mode mutation blocking (SEC-02)
    pub(crate) fn validate_pre_execution_guards(
        ctx: &RunContext,
        fc: &crate::agent::types::ToolCall,
        token: &CapabilityToken,
    ) -> Result<(), ToolExecutionError> {
        if ctx.current_cost_usd >= ctx.budget_limit_usd {
            return Err(ToolExecutionError::SecurityBlocked(format!(
                "Budget exhausted: Current ${:.4} >= Limit ${:.2}",
                ctx.current_cost_usd, ctx.budget_limit_usd
            )));
        }

        if token.agent_id != ctx.agent_id || chrono::Utc::now() > token.expires_at {
            return Err(ToolExecutionError::SecurityBlocked(format!(
                "Invalid or expired CapabilityToken '{}' for agent '{}'",
                token.id, ctx.agent_id
            )));
        }

        if ctx.is_read_only() {
            let is_mutation = fc.name.contains("write")
                || fc.name.contains("edit")
                || fc.name.contains("patch")
                || fc.name.contains("create")
                || fc.name.contains("delete")
                || fc.name.contains("execute")
                || fc.name.contains("run")
                || fc.name == "spawn_agent"
                || fc.name == "spawn_subagent"
                || fc.name == "recruit_specialist"
                || fc.name == "database_migration";
            if is_mutation {
                let mode_label = if ctx.accept_mode == crate::agent::types::AcceptMode::Observe {
                    "Observe (Shadow) mode"
                } else {
                    "Safe mode"
                };
                return Err(ToolExecutionError::SecurityBlocked(format!(
                    "{} active: mutation tool '{}' is blocked for agent '{}'",
                    mode_label, fc.name, ctx.agent_id
                )));
            }
        }

        Ok(())
    }

    /// Evaluates the Aletheia Protocol System 1 Verification Gate for mutation proposals.
    async fn evaluate_aletheia_gate(
        &self,
        ctx: &RunContext,
        fc: &crate::agent::types::ToolCall,
        validation: &mut ValidationResult,
        mission_id_opt: Option<String>,
    ) -> Result<(), ToolExecutionError> {
        let target_path = extract_target_path(&fc.args);
        let mutation_proposal = MutationProposal {
            agent_id: ctx.agent_id.clone(),
            skill_name: fc.name.clone(),
            parameters: fc.args.clone(),
            affected_path: target_path.map(|s| s.to_string()),
            reported_blast_radius: fc
                .args
                .get("blast_radius")
                .and_then(|v| v.as_u64())
                .unwrap_or(1) as usize,
            oversight_required: validation.oversight_required,
        };

        if !self
            .state
            .security
            .verification_gate
            .requires_verification(&mutation_proposal)
        {
            return Ok(());
        }

        tracing::info!(
            "🛡️ [Aletheia] Evaluating mutation proposal for skill '{}' by agent '{}'",
            fc.name,
            ctx.agent_id
        );

        let mut verified_blast_radius = None;
        if let Some(path_str) = target_path {
            let symbol_name = fc.args.get("symbol").and_then(|v| v.as_str()).unwrap_or("*");
            let intel_service =
                crate::intelligence::service::IntelligenceService::new(self.state.clone());
            if let Ok(affected) = intel_service.blast_radius(symbol_name, path_str, Some(50)).await {
                if !affected.is_empty() {
                    verified_blast_radius = Some(affected.len());
                }
            }
        }

        let args_serialized = fc.args.to_string();
        let assessment = crate::agent::verification_gate::SystemOneVerificationAssessment {
            mutation_risk: if fc.name == "delete_file" || fc.name == "database_migration" {
                1.8
            } else if fc.name == "execute_shell"
                && (args_serialized.contains("rm -rf") || args_serialized.contains("format"))
            {
                2.9
            } else {
                0.2
            },
            rule_violation_prob: if args_serialized.contains("DROP TABLE") {
                0.95
            } else {
                0.05
            },
            confidence: 0.95,
            notes: Some(format!("System 1 assessment for skill '{}'", fc.name)),
        };

        let decision = self.state.security.verification_gate.evaluate_with_assessment(
            &mutation_proposal,
            Some(&assessment),
            verified_blast_radius,
            Some("Aletheia live System 1 gate evaluation"),
        );

        match decision {
            VerificationDecision::Rejected {
                reason,
                remediation_hint,
            } => {
                tracing::warn!(
                    "❌ [Aletheia] Mutation proposal for '{}' REJECTED: {} (Hint: {})",
                    fc.name,
                    reason,
                    remediation_hint
                );
                self.broadcast_sys(
                    &format!(
                        "❌ Aletheia Gate REJECTED mutation '{}': {}",
                        fc.name, reason
                    ),
                    "error",
                    mission_id_opt,
                );
                Err(ToolExecutionError::SecurityBlocked(format!(
                    "Aletheia Gate REJECTED mutation '{}': {} (Remediation: {})",
                    fc.name, reason, remediation_hint
                )))
            }
            VerificationDecision::EscalateToOversight { reason, risk_score } => {
                tracing::warn!(
                    "⚠️ [Aletheia] Mutation proposal for '{}' escalated to oversight (Risk: {:.2}): {}",
                    fc.name,
                    risk_score,
                    reason
                );
                self.broadcast_sys(
                    &format!(
                        "⚠️ Aletheia Gate Escalation: '{}' requires oversight approval ({})",
                        fc.name, reason
                    ),
                    "warning",
                    mission_id_opt,
                );
                validation.oversight_required = true;
                Ok(())
            }
            VerificationDecision::Approved => {
                tracing::info!("✅ [Aletheia] Mutation proposal for '{}' APPROVED", fc.name);
                Ok(())
            }
            VerificationDecision::Bypassed => Ok(()),
        }
    }

    /// Constructs an isolated `ToolContext` snapshot from the active `RunContext`.
    pub(super) fn build_tool_context(&self, ctx: &RunContext) -> ToolContext {
        ToolContext {
            mission_id: ctx.mission_id.clone(),
            agent_id: ctx.agent_id.clone(),
            workspace_root: ctx.workspace_root.clone(),
            fs_adapter: ctx.fs_adapter.clone(),
            state: self.state.clone(),
            trace_id: ctx.trace_id.clone(),
            budget_usd: ctx.budget_usd,
            budget_limit_usd: ctx.budget_limit_usd,
            security_policy: ctx.security_policy.clone(),
            active_node_id: ctx.active_node_id.lock().clone(),
            depth: ctx.depth,
            lineage: ctx.lineage.clone(),
            safe_mode: ctx.safe_mode,
            primary_goal: ctx.primary_goal.clone(),
            last_accessed_files: ctx.last_accessed_files.clone(),
            metrics: ctx.metrics.clone(),
            accounting: ctx.accounting.clone(),
            execution_plane: ctx.execution_plane,
            accept_mode: ctx.accept_mode,
            mandate: ctx.mandate.clone(),
        }
    }

    /// Executes a single tool dispatch attempt across the 3-tier lookup hierarchy:
    /// 1. Native `tool_registry` (exact or normalized)
    /// 2. Registered dynamic `skills` (exact or normalized)
    /// 3. On-disk deterministic `execution/` script fallback
    async fn dispatch_single_tool_attempt(
        &self,
        ctx: &RunContext,
        tool_ctx: &ToolContext,
        fc: &crate::agent::types::ToolCall,
        normalized_name: &str,
        oversight_required: bool,
        usage: &mut Option<crate::agent::types::TokenUsage>,
    ) -> Result<String, ToolExecutionError> {
        if let Some(handler) = self
            .state
            .registry
            .tool_registry
            .get(&fc.name)
            .or_else(|| self.state.registry.tool_registry.get(normalized_name))
        {
            let tool_timeout_secs = if matches!(
                normalized_name,
                "issue_alpha_directive"
                    | "spawn_subagent"
                    | "send_mission_directive"
                    | "recruit_specialist"
                    | "read_codebase_file"
            ) || oversight_required
            {
                std::env::var("OVERSIGHT_TIMEOUT_SECS")
                    .ok()
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(300)
            } else {
                60
            };
            return match tokio::time::timeout(
                std::time::Duration::from_secs(tool_timeout_secs),
                handler.execute(tool_ctx, fc.args.clone(), usage),
            )
            .await
            {
                Ok(res) => res,
                Err(_) => {
                    tracing::error!(
                        "🚨 [Runner] Tool '{}' execution TIMED OUT after {}s",
                        fc.name,
                        tool_timeout_secs
                    );
                    Err(ToolExecutionError::ExecutionFailed(format!(
                        "Tool '{}' execution timed out after {} seconds",
                        fc.name, tool_timeout_secs
                    )))
                }
            };
        }

        let snapshot = self.state.registry.skills.snapshot();
        if let Some(skill) = snapshot
            .skills
            .get(&fc.name)
            .or_else(|| snapshot.skills.get(normalized_name))
            .map(|r| r.value().clone())
        {
            let mut out = String::new();
            return match self
                .handle_dynamic_skill(ctx, fc, &mut out, &skill, usage, oversight_required)
                .await
            {
                Ok(()) => Ok(out),
                Err(e) => Err(ToolExecutionError::AppError(e)),
            };
        }

        if let Some(synthesized_skill) =
            Self::resolve_script_fallback_skill(&ctx.workspace_root, normalized_name).await
        {
            let mut out = String::new();
            return match self
                .handle_dynamic_skill(
                    ctx,
                    fc,
                    &mut out,
                    &synthesized_skill,
                    usage,
                    oversight_required,
                )
                .await
            {
                Ok(()) => Ok(out),
                Err(e) => Err(ToolExecutionError::AppError(e)),
            };
        }

        Err(ToolExecutionError::ExecutionFailed(format!(
            "Unknown tool '{}' (normalized: '{}'). If this is a deterministic script, ensure it exists in execution/ or invoke it via 'execute_shell'.",
            fc.name, normalized_name
        )))
    }

    /// Manages the Zero-Trust sequence (Budget Check -> Token Validation -> WAL -> CBS -> Oversight -> Execute).
    async fn run_zero_trust_pipeline(
        &self,
        ctx: &RunContext,
        fc: &crate::agent::types::ToolCall,
        usage: &mut Option<crate::agent::types::TokenUsage>,
        _user_message: &str,
        token: CapabilityToken,
    ) -> Result<String, ToolExecutionError> {
        // 0 & 1: Fast-Path Budget, CapabilityToken, and Safe-Mode Guards
        Self::validate_pre_execution_guards(ctx, fc, &token)?;

        let args_str = serde_json::to_string(&fc.args).unwrap_or_default();
        let mission_id_opt = Some(ctx.mission_id.clone());

        // 2. Write-Ahead Log (WAL): Record intent before execution
        let _log_id = uuid::Uuid::new_v4().to_string();
        self.state
            .record_audit(
                &ctx.agent_id,
                mission_id_opt.as_deref(),
                ctx.user_id.as_deref(),
                &format!("[INTENT] {}", fc.name),
                &self.state.security.secret_redactor.redact(&args_str),
            )
            .await
            .map_err(ToolExecutionError::AppError)?;

        // 3. Security Manager (Hierarchy & Policy)
        let sec_mgr = DefaultSecurityManager;
        let mut validation = sec_mgr.pre_validate(self, ctx, fc).await?;

        // 3b. Aletheia Protocol Verification Gate
        self.evaluate_aletheia_gate(ctx, fc, &mut validation, mission_id_opt.clone())
            .await?;

        // 4. Oversight Check (§4.3: Prevent double human approval; execute_shell has its own dedicated Sapphire Gate)
        if validation.oversight_required && fc.name != "execute_shell" {
            self.broadcast_sys(
                &format!("🔒 Security Gate: '{}' requires explicit approval.", fc.name),
                "warning",
                mission_id_opt.clone(),
            );

            let approved = self
                .submit_oversight(
                    crate::agent::types::ToolCallAudit {
                        id: uuid::Uuid::new_v4().to_string(),
                        agent_id: ctx.agent_id.clone(),
                        mission_id: mission_id_opt.clone(),
                        skill: fc.name.clone(),
                        params: fc.args.clone(),
                        department: ctx.department.clone(),
                        description: validation.oversight_reason,
                        timestamp: chrono::Utc::now().to_rfc3339(),
                    },
                    mission_id_opt.clone(),
                )
                .await
                .map_err(ToolExecutionError::AppError)?;

            if !approved {
                return Err(ToolExecutionError::SecurityBlocked(format!(
                    "Execution of {} REJECTED by Oversight Security Gate",
                    fc.name
                )));
            }
        }

        // 5. Execute with Isolated Context and Self-Annealing Retry Loop
        let tool_ctx = self.build_tool_context(ctx);
        let mut retry_count = 0;
        let max_retries = 2;
        let normalized_name = normalize_tool_name(&fc.name);

        loop {
            let span = tracing::info_span!(
                "ToolExecution",
                tool = %fc.name,
                normalized_tool = %normalized_name,
                trace_id = %ctx.trace_id,
                agent_id = %ctx.agent_id,
                mission_id = %ctx.mission_id
            );

            let result = self
                .dispatch_single_tool_attempt(
                    ctx,
                    &tool_ctx,
                    fc,
                    &normalized_name,
                    validation.oversight_required,
                    usage,
                )
                .instrument(span)
                .await;

            match result {
                Ok(res) => {
                    let output_summary = self.safe_truncate(&res, 1000);
                    let _ = self
                        .state
                        .record_audit(
                            &ctx.agent_id,
                            mission_id_opt.as_deref(),
                            ctx.user_id.as_deref(),
                            &format!("[SUCCESS] {}", fc.name),
                            &output_summary,
                        )
                        .await;
                    return Ok(res);
                }
                Err(e) if e.is_transient() && retry_count < max_retries => {
                    retry_count += 1;
                    tokio::time::sleep(std::time::Duration::from_millis(500 * retry_count)).await;
                    continue;
                }
                Err(e) => {
                    let _ = self
                        .state
                        .record_audit(
                            &ctx.agent_id,
                            mission_id_opt.as_deref(),
                            ctx.user_id.as_deref(),
                            &format!("[FAILURE] {}", fc.name),
                            &format!("Error: {}", e),
                        )
                        .await;
                    return Err(e);
                }
            }
        }
    }
}

// Metadata: [pipeline]

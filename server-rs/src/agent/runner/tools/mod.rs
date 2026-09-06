//! @docs ARCHITECTURE:Registry
//! 
//! ### AI Assist Note
//! **! @docs ARCHITECTURE:Runner**
//! This module implements high-fidelity logic for the Sovereign Reality layer.
//! 
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Runtime logic error, state desynchronization, or resource exhaustion.
//! - **Telemetry Link**: Search `[mod]` in tracing logs.
//! 
//! @docs ARCHITECTURE:Runner
//!
//! ### AI Assist Note
//! **Tool Dispatcher**: Orchestrates the execution of both built-in and dynamic
//! script-based tools. Enforces **Zero-Trust CBS (Capability-Based Security)** and
//! **Human-in-the-Loop Oversight**. Implements **WAL (Write-Ahead Logging)** 
//! to ensure all tool attempts are persisted before execution (SEC-04).

pub mod manifest;
pub mod error;
pub mod security;
pub mod registry;
pub mod dispatcher;
pub mod trait_tool;
pub mod capability;
use crate::error::AppError;
use security::{SecurityManager, DefaultSecurityManager};
use tracing::Instrument;

pub use trait_tool::Tool;
pub use crate::agent::types::ToolContext;
pub use capability::{CapabilityToken, ZeroTrustGuard};

use super::{AgentRunner, RunContext};
use error::ToolExecutionError;
use crate::agent::verification_gate::{MutationProposal, VerificationDecision};

/// Normalizes raw tool names passed by LLMs by stripping path prefixes and script extensions.
pub fn normalize_tool_name(raw: &str) -> String {
    static PREFIXES: &[&str] = &[
        "./",
        "execution/",
        "execution\\",
        "agent_generated/skills/",
        "agent_generated\\skills\\",
        "skills/",
        "skills\\",
        "directives/",
        "directives\\",
    ];
    static SUFFIXES: &[&str] = &[".py", ".sh", ".ps1", ".bat", ".exe"];

    let mut s = raw.trim();
    for prefix in PREFIXES {
        if let Some(rest) = s.strip_prefix(prefix) {
            s = rest;
        }
    }
    for suffix in SUFFIXES {
        if let Some(rest) = s.strip_suffix(suffix) {
            s = rest;
        }
    }
    s.to_string()
}

/// Recursively merges a patch JSON object into a target JSON value.
pub fn recursive_merge_json(target: &mut serde_json::Value, patch: &serde_json::Value) {
    match (target, patch) {
        (serde_json::Value::Object(target_obj), serde_json::Value::Object(patch_obj)) => {
            for (key, val) in patch_obj {
                if let Some(existing) = target_obj.get_mut(key) {
                    recursive_merge_json(existing, val);
                } else {
                    target_obj.insert(key.clone(), val.clone());
                }
            }
        }
        (target_val, patch_val) => {
            *target_val = patch_val.clone();
        }
    }
}

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

        match self.run_zero_trust_pipeline(ctx, fc, usage, user_message, token).await {
            Ok(output) => {
                *output_text = output.clone();
                Ok(Some(output))
            }
            Err(e) => {
                let recovery = e.recovery_strategy();
                let error_msg = format!("(TOOL FAILURE: {} | RECOVERY: {:?})", e, recovery);
                *output_text = error_msg;
                
                // Even on error, we return Ok(Some) to surface the failure to the agent
                // unless it's a critical infrastructure failure.
                Ok(Some(output_text.clone()))
            }
        }
    }

    /// Manages the Zero-Trust sequence (Budget Check -> Token Validation -> WAL -> CBS -> Oversight -> Execute)
    async fn run_zero_trust_pipeline(
        &self,
        ctx: &RunContext,
        fc: &crate::agent::types::ToolCall,
        usage: &mut Option<crate::agent::types::TokenUsage>,
        _user_message: &str,
        token: CapabilityToken,
    ) -> Result<String, ToolExecutionError> {
        // 0. Fast-Path Budget Check (Fail fast before WAL and Oversight)
        if ctx.current_cost_usd >= ctx.budget_limit_usd {
            return Err(ToolExecutionError::SecurityBlocked(format!(
                "Budget exhausted: Current ${:.4} >= Limit ${:.2}",
                ctx.current_cost_usd, ctx.budget_limit_usd
            )));
        }

        // 1. Capability-Based Security (CBS) Token Validation
        if token.agent_id != ctx.agent_id || chrono::Utc::now() > token.expires_at {
            return Err(ToolExecutionError::SecurityBlocked(format!(
                "Invalid or expired CapabilityToken '{}' for agent '{}'",
                token.id, ctx.agent_id
            )));
        }

        let args_str = serde_json::to_string(&fc.args).unwrap_or_default();
        let mission_id_opt = Some(ctx.mission_id.clone());

        // 2. Write-Ahead Log (WAL)
        // We MUST record the intent before execution.
        let _log_id = uuid::Uuid::new_v4().to_string();
        self.state.record_audit(
            &ctx.agent_id,
            mission_id_opt.as_deref(),
            ctx.user_id.as_deref(),
            &format!("[INTENT] {}", fc.name),
            &self.state.security.secret_redactor.redact(&args_str),
        ).await.map_err(|e| ToolExecutionError::AppError(e))?;

        // 3. Security Manager (Hierarchy & Policy)
        let sec_mgr = DefaultSecurityManager;
        let validation = sec_mgr.pre_validate(self, ctx, fc).await?;

        // 3b. Aletheia Protocol Verification Gate
        let mutation_proposal = MutationProposal {
            agent_id: ctx.agent_id.clone(),
            skill_name: fc.name.clone(),
            parameters: fc.args.clone(),
            affected_path: fc.args.get("path").or_else(|| fc.args.get("file_path")).and_then(|v| v.as_str()).map(|s| s.to_string()),
            reported_blast_radius: fc.args.get("blast_radius").and_then(|v| v.as_u64()).unwrap_or(1) as usize,
            oversight_required: validation.oversight_required,
        };

        if self.state.security.verification_gate.requires_verification(&mutation_proposal) {
            tracing::info!(
                "🛡️ [Aletheia] Evaluating mutation proposal for skill '{}' by agent '{}'",
                fc.name, ctx.agent_id
            );

            // Compute verified blast radius from symbol graph if path or target is provided
            let mut verified_blast_radius = None;
            if let Some(target_path) = fc.args.get("path").or_else(|| fc.args.get("file_path")).and_then(|v| v.as_str()) {
                let symbol_name = fc.args.get("symbol").and_then(|v| v.as_str()).unwrap_or("*");
                let intel_service = crate::intelligence::service::IntelligenceService::new(self.state.clone());
                if let Ok(affected) = intel_service.blast_radius(symbol_name, target_path, Some(50)).await {
                    if !affected.is_empty() {
                        verified_blast_radius = Some(affected.len());
                    }
                }
            }

            let decision = self.state.security.verification_gate.evaluate(
                &mutation_proposal,
                true,
                verified_blast_radius,
                Some("Aletheia live gate evaluation"),
            );

            match decision {
                VerificationDecision::Rejected { reason, remediation_hint } => {
                    tracing::warn!(
                        "❌ [Aletheia] Mutation proposal for '{}' REJECTED: {} (Hint: {})",
                        fc.name, reason, remediation_hint
                    );
                    self.broadcast_sys(
                        &format!("❌ Aletheia Gate REJECTED mutation '{}': {}", fc.name, reason),
                        "error",
                        mission_id_opt.clone(),
                    );
                    return Err(ToolExecutionError::SecurityBlocked(format!(
                        "Aletheia Gate REJECTED mutation '{}': {} (Remediation: {})",
                        fc.name, reason, remediation_hint
                    )));
                }
                VerificationDecision::Approved => {
                    tracing::info!("✅ [Aletheia] Mutation proposal for '{}' APPROVED", fc.name);
                }
                VerificationDecision::Bypassed => {}
            }
        }

        // 4. Oversight Check
        if validation.oversight_required {
            self.broadcast_sys(
                &format!("🔒 Security Gate: '{}' requires explicit approval.", fc.name),
                "warning",
                mission_id_opt.clone(),
            );

            let approved = self.submit_oversight(
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
            ).await.map_err(ToolExecutionError::AppError)?;

            if !approved {
                return Err(ToolExecutionError::SecurityBlocked(format!("Execution of {} REJECTED by Oversight Security Gate", fc.name)));
            }
        }

        // 5. Execute with Isolated Context
        let tool_ctx = ToolContext {
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
        };

        // Execution Loop with Self-Annealing
        let mut retry_count = 0;
        let max_retries = 2;
        let normalized_name = normalize_tool_name(&fc.name);

        loop {
            let span = tracing::info_span!("ToolExecution", 
                tool = %fc.name,
                normalized_tool = %normalized_name,
                trace_id = %ctx.trace_id,
                agent_id = %ctx.agent_id,
                mission_id = %ctx.mission_id
            );
            // Wrap tool execution in a hard timeout to prevent "Silent Hangs" (INFRA-05)
            // Multi-tiered lookup: exact tool_registry -> normalized tool_registry -> exact skills -> normalized skills -> on-disk deterministic script trapping
            let result = async {
                if let Some(handler) = self.state.registry.tool_registry.get(&fc.name).or_else(|| self.state.registry.tool_registry.get(&normalized_name)) {
                    match tokio::time::timeout(std::time::Duration::from_secs(60), handler.execute(&tool_ctx, fc.args.clone(), usage)).await {
                        Ok(res) => res,
                        Err(_) => {
                            tracing::error!("🚨 [Runner] Tool '{}' execution TIMED OUT after 60s", fc.name);
                            Err(ToolExecutionError::ExecutionFailed(format!("Tool '{}' execution timed out after 60 seconds", fc.name)))
                        }
                    }
                } else {
                    let snapshot = self.state.registry.skills.snapshot();
                    if let Some(skill) = snapshot.skills.get(&fc.name).or_else(|| snapshot.skills.get(&normalized_name)).map(|r| r.value().clone()) {
                        let mut out = String::new();
                        match self.handle_dynamic_skill(ctx, fc, &mut out, &skill, usage).await {
                            Ok(()) => Ok(out),
                            Err(e) => Err(ToolExecutionError::AppError(e)),
                        }
                    } else {
                        // Fallback: Deterministic Execution Script Trapping on disk (async I/O)
                        let ws_root = &ctx.workspace_root;
                        let script_candidates = [
                            ws_root.join("execution").join(format!("{}.py", normalized_name)),
                            ws_root.join("execution").join(format!("{}.sh", normalized_name)),
                            ws_root.join("execution").join(format!("{}.ps1", normalized_name)),
                            ws_root.join("execution").join("agent_generated").join("skills").join(format!("{}.py", normalized_name)),
                        ];

                        let mut existing_candidate = None;
                        for cand in &script_candidates {
                            if tokio::fs::metadata(cand).await.is_ok() {
                                existing_candidate = Some(cand.clone());
                                break;
                            }
                        }

                        if let Some(candidate) = existing_candidate {
                            let ext = candidate.extension().and_then(|e| e.to_str()).unwrap_or("py");
                            let rel_path = if candidate.to_string_lossy().contains("agent_generated") {
                                format!("execution/agent_generated/skills/{}.{}", normalized_name, ext)
                            } else {
                                format!("execution/{}.{}", normalized_name, ext)
                            };
                            let exec_cmd = match ext {
                                "sh" => format!("bash {}", rel_path),
                                "ps1" => format!("powershell {}", rel_path),
                                _ => format!("python {}", rel_path),
                            };

                            let synthesized_skill = crate::agent::script_skills::SkillDefinition {
                                id: None,
                                name: normalized_name.clone(),
                                description: format!("Deterministic execution script '{}'", normalized_name),
                                execution_command: exec_cmd,
                                schema: serde_json::json!({"type": "object", "properties": {}}),
                                oversight_required: true,
                                doc_url: None,
                                tags: Some(vec!["execution".to_string(), "deterministic".to_string()]),
                                full_instructions: None,
                                negative_constraints: None,
                                verification_script: None,
                                category: "execution".to_string(),
                            };

                            let mut out = String::new();
                            match self.handle_dynamic_skill(ctx, fc, &mut out, &synthesized_skill, usage).await {
                                Ok(()) => Ok(out),
                                Err(e) => Err(ToolExecutionError::AppError(e)),
                            }
                        } else {
                            Err(ToolExecutionError::ExecutionFailed(format!(
                                "Unknown tool '{}' (normalized: '{}'). If this is a deterministic script, ensure it exists in execution/ or invoke it via 'execute_shell'.",
                                fc.name, normalized_name
                            )))
                        }
                    }
                }
            }
            .instrument(span)
            .await;

            match result {
                Ok(res) => {
                    // Record successful completion in audit trail
                    let _ = self.state.record_audit(
                        &ctx.agent_id,
                        mission_id_opt.as_deref(),
                        ctx.user_id.as_deref(),
                        &format!("[SUCCESS] {}", fc.name),
                        "Execution completed successfully",
                    ).await;
                    return Ok(res);
                }
                Err(e) if e.is_transient() && retry_count < max_retries => {
                    retry_count += 1;
                    tokio::time::sleep(std::time::Duration::from_millis(500 * retry_count)).await;
                    continue;
                }
                Err(e) => {
                    // Record failure in audit trail
                    let _ = self.state.record_audit(
                        &ctx.agent_id,
                        mission_id_opt.as_deref(),
                        ctx.user_id.as_deref(),
                        &format!("[FAILURE] {}", fc.name),
                        &format!("Error: {}", e),
                    ).await;
                    return Err(e);
                }
            }
        }
    }


    /// Handles execution of dynamic file-based skills via the MCP Host.
    /// 
    /// ### 🚀 Dynamic Lifecycle
    /// - **Verification**: If the skill defines a `verification_script`, it is run 
    ///   immediately after tool completion to validate the "Physical Reality" 
    ///   matches the tool's intended effect.
    /// - **Sanitization**: All output is passed through the `Sanitizer` to prevent 
    ///   secret leakage or terminal escape sequences.
    #[allow(dead_code)]
    async fn handle_dynamic_skill(
        &self,
        ctx: &RunContext,
        fc: &crate::agent::types::ToolCall,
        output_text: &mut String,
        skill: &crate::agent::script_skills::SkillDefinition,
        _usage: &mut Option<crate::agent::types::TokenUsage>,
    ) -> Result<(), AppError> {
        // 🛡️ [Governance] Check Permission Policy & Oversight requirement
        let perm_mode = self
            .state
            .security
            .permission_policy
            .get_mode(&skill.name, &ctx.agent_id)
            .await;

        if perm_mode == crate::security::permissions::PermissionMode::Deny {
            *output_text = format!(
                "Execution of skill '{}' is explicitly denied by governance policy.",
                skill.name
            );
            return Ok(());
        }

        if (skill.oversight_required || perm_mode == crate::security::permissions::PermissionMode::Prompt)
            && perm_mode != crate::security::permissions::PermissionMode::Allow
        {
            *output_text = format!(
                "Skill '{}' requires explicit human oversight approval or 'Allow' permission mode.",
                skill.name
            );
            return Ok(());
        }

        let snapshot = self.state.registry.skills.snapshot();
        let tool_ctx = ToolContext {
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
        };

        let result = self
            .state
            .registry
            .mcp_host
            .call_tool(
                &skill.name,
                fc.args.clone(),
                &tool_ctx,
                &snapshot.skills,
            )
            .await;

        match result {
            Ok(crate::agent::mcp::McpResult::Raw(output)) => {
                // 🛡️ [Security] Sanitization Hook
                if let crate::agent::sanitizer::SanitizationResult::Alert(msg) =
                    crate::agent::sanitizer::Sanitizer::scan(&output)
                {
                    *output_text = format!("(TOOL EXECUTION HALTED FOR SECURITY: {})", msg);
                    return Ok(());
                }

                let mut final_output = output;
                if let Some(verify_script) = &skill.verification_script {
                    match self
                        .run_verification_script(
                            verify_script,
                            &skill.name,
                            &fc.args,
                            &final_output,
                            &ctx.workspace_root,
                        )
                        .await
                    {
                        Ok(verify_res) => {
                            final_output = format!(
                                "{}\n\n[VERIFICATION STATUS]:\n{}",
                                final_output, verify_res
                            );
                        }
                        Err(e) => {
                            final_output =
                                format!("{}\n\n[VERIFICATION CRITICAL ERROR]: {}", final_output, e);
                        }
                    }
                }

                *output_text = format!(
                    "({} EXECUTED SUCCESSFULLY):\n\n{}",
                    skill.name, final_output
                );
            }
            Ok(crate::agent::mcp::McpResult::SystemDelegate(name, args)) if name == "recruit_specialist" => {
                let mut mapped_args = serde_json::Map::new();
                if let Some(aid) = args.get("agent_id") {
                    mapped_args.insert("agent_id".to_string(), aid.clone());
                }
                if let Some(msg) = args.get("task_description") {
                    mapped_args.insert("message".to_string(), msg.clone());
                }

                let mapped_fc = crate::agent::types::ToolCall {
                    name: "spawn_subagent".to_string(),
                    args: serde_json::Value::Object(mapped_args),
                };
                let res = self.handle_spawn_subagent(ctx, &mapped_fc, _usage)
                    .await
                    .map_err(|e| match e {
                        ToolExecutionError::AppError(ae) => ae,
                        _ => AppError::InternalServerError(e.to_string()),
                    })?;
                output_text.push_str(&res);
            }
            Ok(crate::agent::mcp::McpResult::SystemDelegate(_, _)) => {
                // Handle other delegates if any
            }
            Err(e) => {
                *output_text = format!("(SKILL EXEC FAILED: {})", e);
            }
        }
        Ok(())
    }

    /// Updates the agent's persistent working memory (scratchpad).
    /// 
    /// ### 🧠 Cognition Side Effects
    /// This memory persists across agent spawns and engine restarts. It is the
    /// primary mechanism for an agent to maintain "Context Continuity" when
    /// executing multi-stage missions.
    pub(crate) async fn handle_update_working_memory(
        &self,
        ctx: &RunContext,
        fc: &crate::agent::types::ToolCall,
        output_text: &mut String,
    ) -> Result<(), AppError> {
        let new_memory = fc
            .args
            .get("memory")
            .cloned()
            .unwrap_or(serde_json::json!({}));

        if let Some(mut entry) = self.state.registry.agents.get_mut(&ctx.agent_id) {
            let agent = entry.value_mut();

            // Deep recursive JSON merge to preserve nested properties
            recursive_merge_json(&mut agent.state.working_memory, &new_memory);

            let agent_data = agent.clone();
            drop(entry); // Release DashMap lock

            // Sync to DB
            crate::agent::persistence::save_agent_db(&self.state.resources.pool, &agent_data)
                .await?;

            self.state.emit_event(serde_json::json!({
                "type": "agent:update",
                "data": agent_data
            }));

            *output_text = "(WORKING MEMORY UPDATED SUCCESSFULLY)".to_string();
        } else {
            *output_text =
                "(ERROR: Agent not found in registry during working memory update)".to_string();
        }

        Ok(())
    }

    /// Recursively executes a batch of tool calls provided by the LLM.
    /// 
    /// ### ⏩ Efficiency Engine
    /// This "collapses" multiple model turns into a single execution chain.
    /// It is used by the model when it has high confidence in a sequence of 
    /// deterministic steps (e.g., "Read File -> Grep -> Write Result").
    pub(crate) async fn handle_script_builder(
        &self,
        ctx: &RunContext,
        fc: &crate::agent::types::ToolCall,
        output_text: &mut String,
        usage: &mut Option<crate::agent::types::TokenUsage>,
        user_message: &str,
    ) -> Result<(), AppError> {
        let steps = fc
            .args
            .get("steps")
            .and_then(|v| v.as_array())
            .ok_or_else(|| AppError::BadRequest("'steps' must be an array in script_builder".to_string()))?;

        output_text.push_str("\n--- BATCH EXECUTION STARTED ---\n");

        for (i, step) in steps.iter().enumerate() {
            let tool_name = step
                .get("tool")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::BadRequest(format!("Step {} missing 'tool' name", i)))?;
            let params = step
                .get("params")
                .cloned()
                .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

            let mut step_output = String::new();
            let step_fc = crate::agent::types::ToolCall {
                name: tool_name.to_string(),
                args: params,
            };

            tracing::info!("📦 [ScriptBuilder] Executing step {}: {}", i + 1, tool_name);
            output_text.push_str(&format!("\n[Step {}: {}]\n", i + 1, tool_name));

            // Execute the individual tool
            let _ = std::pin::Pin::from(Box::new(self.execute_tool(
                ctx,
                &step_fc,
                &mut step_output,
                usage,
                user_message,
            )))
            .await?;

            output_text.push_str(&step_output);
        }

        output_text.push_str("\n--- BATCH EXECUTION COMPLETED ---\n");
        Ok(())
    }

    /// Handles `execute_shell`: runs a terminal command in the workspace.
    /// 🛡️ PROTECTED: Requires Sapphire Gate (Critical Oversight) and ShellScanner.
    pub(crate) async fn handle_execute_shell(
        &self,
        ctx: &RunContext,
        fc: &crate::agent::types::ToolCall,
        output_text: &mut String,
    ) -> Result<(), AppError> {
        let command_str = fc.args.get("command").and_then(|v| v.as_str()).unwrap_or("");
        if command_str.is_empty() {
            *output_text = "(SHELL FAILED: 'command' argument is missing)".to_string();
            return Ok(());
        }

        tracing::info!("💻 [System] Agent {} requesting shell execution: {}", ctx.agent_id, command_str);

        // 1. Tokenize & Validate
        let parts: Vec<String> = command_str.split_whitespace().map(|s| s.to_string()).collect();
        if parts.is_empty() {
            *output_text = "(SHELL FAILED: Command is empty)".to_string();
            return Ok(());
        }

        let bin = &parts[0];
        let args = &parts[1..];

        if let Err(e) = crate::utils::security::validate_tokenized_command(bin, args) {
            tracing::warn!("🛡️ [Security] Shell execution BLOCKED by tokenized scanner: {}", e);
            *output_text = format!("(SECURITY BLOCKED: {})", e);
            return Ok(());
        }

        match self.state.security.shell_scanner.scan(command_str) {
            crate::security::scanner::ScannerResult::Risky(reason) => {
                tracing::warn!("🛡️ [Security] Shell execution BLOCKED by advanced scanner: {}", reason);
                *output_text = format!("(SECURITY BLOCKED: {})", reason);
                return Ok(());
            }
            crate::security::scanner::ScannerResult::Safe => {}
        }

        self.broadcast_agent(
            ctx,
            &format!("💎 Oversight: wants to run terminal command: {}. CRITICAL REVIEW REQUIRED.", command_str),
            "error", // Use error color for Sapphire Gate
        );

        // 2. Sapphire Gate Oversight
        let approved = self
            .submit_oversight(
                crate::agent::types::ToolCallAudit {
                    id: uuid::Uuid::new_v4().to_string(),
                    agent_id: ctx.agent_id.clone(),
                    mission_id: Some(ctx.mission_id.clone()),
                    skill: "execute_shell".to_string(),
                    params: fc.args.clone(),
                    department: ctx.department.clone(),
                    description: format!("Executing terminal command in workspace: {}", command_str),
                    timestamp: chrono::Utc::now().to_rfc3339(),
                },
                Some(ctx.mission_id.clone()),
            )
            .await?;

        if !approved {
            *output_text = format!("(Shell execution REJECTED by Oversight) {}", output_text);
            return Ok(());
        }

        self.broadcast_agent(ctx, &format!("💻 System: running '{}'...", command_str), "info");
        
        let _permit = self.state.resources.arbiter.acquire().await
            .map_err(|e| AppError::InternalServerError(format!("Resource arbiter failure: {}", e)))?;

        // 3. Execution via Security Sandbox
        let sandbox_config = crate::security::sandbox::SandboxConfig::default();
        match crate::security::sandbox::execute_sandboxed_shell(command_str, &ctx.workspace_root, &sandbox_config).await {
            Ok(output) => {
                let truncated = self.safe_truncate(&output, 5000);
                *output_text = format!("(SHELL OUTPUT of '{}'):\n\n{}", command_str, truncated);
            }
            Err(e) => {
                tracing::warn!("🛡️ [Security] Sandboxed shell execution blocked or failed: {}", e);
                *output_text = format!("(SHELL EXECUTION FAILED: {})", e);
            }
        }

        Ok(())
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use crate::agent::types::{EngineAgent, ToolCall};
    use crate::agent::constants::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_execute_tool_cbs_block() {
        let state = Arc::new(AppState::new_minimal_mock().await);
        let runner = AgentRunner::new(state.clone());
        let mut ctx = RunContext::default();
        ctx.agent_id = "worker-1".to_string();

        let mut agent = EngineAgent::default();
        agent.identity.id = ctx.agent_id.clone();
        agent.capabilities.skills = vec!["allowed_skill".to_string()];
        state.registry.agents.insert(ctx.agent_id.clone(), agent);

        let fc = ToolCall {
            name: "forbidden_skill".to_string(),
            args: serde_json::json!({}),
        };

        let mut output = String::new();
        let mut usage = None;
        let result = runner.execute_tool(&ctx, &fc, &mut output, &mut usage, "").await;
        println!("DEBUG OUTPUT: {}", output);

        assert!(result.is_ok());
        assert!(output.contains("Security Violation: Skill 'forbidden_skill' not in agent allowlist"));
        assert!(output.contains("| RECOVERY: Escalate"));
    }

    #[tokio::test]
    async fn test_execute_tool_hierarchy_block() {
        let state = Arc::new(AppState::new_minimal_mock().await);
        let runner = AgentRunner::new(state.clone());
        let mut ctx = RunContext::default();
        ctx.agent_id = AGENT_CEO.to_string();

        let mut agent = EngineAgent::default();
        agent.identity.id = ctx.agent_id.clone();
        state.registry.agents.insert(ctx.agent_id.clone(), agent);

        let fc = ToolCall {
            name: "spawn_subagent".to_string(),
            args: serde_json::json!({"agent_id": "worker"}),
        };

        let mut output = String::new();
        let mut usage = None;
        let result = runner.execute_tool(&ctx, &fc, &mut output, &mut usage, "").await;

        assert!(result.is_ok());
        assert!(output.contains("Hierarchy Violation: As CEO, you are prohibited from direct worker recruitment."));
        assert!(output.contains("| RECOVERY: Escalate"));
    }

    #[tokio::test]
    async fn test_execute_tool_policy_deny() {
        let state = Arc::new(AppState::new_minimal_mock().await);
        let runner = AgentRunner::new(state.clone());
        let ctx = RunContext::default();

        // Set policy to Deny for a specific tool
        state.security.permission_policy.set_mode("risky_tool", crate::security::permissions::PermissionMode::Deny).await;

        let fc = ToolCall {
            name: "risky_tool".to_string(),
            args: serde_json::json!({}),
        };

        let mut output = String::new();
        let mut usage = None;
        let result = runner.execute_tool(&ctx, &fc, &mut output, &mut usage, "").await;

        assert!(result.is_ok());
        assert!(output.contains("Security Violation: Policy for 'risky_tool' is set to DENY"));
        assert!(output.contains("| RECOVERY: Escalate"));
    }

    #[tokio::test]
    async fn test_update_working_memory() {
        let state = Arc::new(AppState::new_minimal_mock().await);
        let runner = AgentRunner::new(state.clone());
        let mut ctx = RunContext::default();
        ctx.agent_id = "memory-agent".to_string();

        let mut agent = EngineAgent::default();
        agent.identity.id = ctx.agent_id.clone();
        state.registry.agents.insert(ctx.agent_id.clone(), agent);
        
        // Ensure agent exists in DB for persistence call
        crate::agent::persistence::save_agent_db(&state.resources.pool, &state.registry.agents.get(&ctx.agent_id).unwrap()).await.unwrap();

        let fc = ToolCall {
            name: "update_working_memory".to_string(),
            args: serde_json::json!({"memory": {"last_step": "initialized"}}),
        };

        let mut output = String::new();
        let result = runner.handle_update_working_memory(&ctx, &fc, &mut output).await;

        assert!(result.is_ok());
        let agent = state.registry.agents.get(&ctx.agent_id).unwrap();
        assert_eq!(agent.state.working_memory["last_step"], "initialized");
    }

    #[test]
    fn test_normalize_tool_name() {
        assert_eq!(normalize_tool_name("parity_guard"), "parity_guard");
        assert_eq!(normalize_tool_name("execution/parity_guard.py"), "parity_guard");
        assert_eq!(normalize_tool_name("./execution/security_scan.py"), "security_scan");
        assert_eq!(normalize_tool_name("execution\\verify_ai_context.py"), "verify_ai_context");
        assert_eq!(normalize_tool_name("agent_generated/skills/custom_task.py"), "custom_task");
        assert_eq!(normalize_tool_name("skills/auto_clean.sh"), "auto_clean");
        assert_eq!(normalize_tool_name("db_health_check.ps1"), "db_health_check");
    }

    #[test]
    fn test_recursive_merge_json() {
        let mut base = serde_json::json!({
            "nested": {
                "key1": "val1",
                "key2": 42
            },
            "array": [1, 2],
            "unchanged": true
        });

        let patch = serde_json::json!({
            "nested": {
                "key2": 100,
                "key3": "new_val"
            },
            "new_top": "hello"
        });

        recursive_merge_json(&mut base, &patch);

        assert_eq!(base["nested"]["key1"], "val1");
        assert_eq!(base["nested"]["key2"], 100);
        assert_eq!(base["nested"]["key3"], "new_val");
        assert_eq!(base["unchanged"], true);
        assert_eq!(base["new_top"], "hello");
    }

    #[tokio::test]
    async fn test_execute_tool_aletheia_gate_rejection() {
        let state = Arc::new(AppState::new_minimal_mock().await);
        let runner = AgentRunner::new(state.clone());
        let mut ctx = RunContext::default();
        ctx.agent_id = "worker-mutator".to_string();

        let mut agent = EngineAgent::default();
        agent.identity.id = ctx.agent_id.clone();
        agent.capabilities.skills = vec!["write_file".to_string()];
        state.registry.agents.insert(ctx.agent_id.clone(), agent);

        // Propose a mutation with high blast radius (exceeds default threshold 15)
        let fc = ToolCall {
            name: "write_file".to_string(),
            args: serde_json::json!({
                "path": "src/core.rs",
                "blast_radius": 45,
                "content": "pub fn mutated() {}"
            }),
        };

        let mut output = String::new();
        let mut usage = None;
        let result = runner.execute_tool(&ctx, &fc, &mut output, &mut usage, "").await;

        assert!(result.is_ok());
        assert!(output.contains("Aletheia Gate REJECTED mutation 'write_file'"));
        assert!(output.contains("exceeds safety threshold (15)"));
    }

    #[tokio::test]
    async fn test_handle_execute_shell_sandboxed_default_deny() {
        let state = Arc::new(AppState::new_minimal_mock().await);
        let runner = AgentRunner::new(state.clone());
        let mut ctx = RunContext::default();
        ctx.agent_id = "worker-shell".to_string();

        let mut agent = EngineAgent::default();
        agent.identity.id = ctx.agent_id.clone();
        agent.capabilities.skills = vec!["execute_shell".to_string()];
        state.registry.agents.insert(ctx.agent_id.clone(), agent);

        let fc = ToolCall {
            name: "execute_shell".to_string(),
            args: serde_json::json!({
                "command": "echo test"
            }),
        };

        let mut output = String::new();
        let mut usage = None;
        let _ = runner.execute_tool(&ctx, &fc, &mut output, &mut usage, "").await;
        // In default configuration (no Docker, no Wasm, host fallback false), shell execution is denied
        // Note: Oversight might prompt first if required, or if rejected by Oversight/Sandbox
        assert!(output.contains("REJECTED by Oversight") || output.contains("Unsandboxed shell execution on host is disabled by default") || output.contains("SHELL OUTPUT"));
    }
}



// Metadata: [tools]





// Metadata: [mod]

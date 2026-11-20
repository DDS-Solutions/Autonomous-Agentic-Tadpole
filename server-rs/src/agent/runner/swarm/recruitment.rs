//! @docs ARCHITECTURE:Runner
//!
//! ### AI Assist Note
//! **Swarm Tiered Recruitment & Authority Guard**: Enforces role-authority ceilings and implements 3-tier specialist resolution (user-sector, swarm pool, fabrication).
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Privilege escalation attempt or database persistence failure during fabrication.
//! - **Telemetry Link**: Search `[recruitment]` in tracing logs.

use super::sanitize::{is_privileged_mutating_skill, validate_slug};
use super::{SubAgentOptions, DEFAULT_SUBAGENT_BUDGET_CAP_USD};
use crate::agent::runner::tools::error::ToolExecutionError;
use crate::agent::runner::{AgentRunner, RunContext};
use crate::agent::types::{EngineAgent, RoleAuthorityLevel};
use crate::error::AppError;
use crate::security::permissions::PermissionMode;

impl AgentRunner {
    /// ### 🧠 Orchestration: Tiered Recruitment (ensure_sub_agent_exists)
    /// Guarantees that a sub-agent exists in the swarm's memory registry before a 
    /// mission dispatch. Implements a prioritized search strategy to optimize for 
    /// specialist continuity.
    /// 
    /// ### 🧬 Search Strategy: Priority Tiers
    /// 1. **Tier 1 (Specialist)**: Searches for "user-sector" agents identified 
    ///    by `name` or `role`. These are typically highly-refined or fine-tuned 
    ///    specialists manually configured by the user.
    /// 2. **Tier 2 (Swarm Pool)**: Searches for existing "ai-sector" agents 
    ///    that have participated in previous missions. These agents may have 
    ///    accumulated "Experience" in their persistent working memory.
    /// 3. **Tier 3 (Fabrication)**: If no match is found, a new agent node is 
    ///    atomically created, registered in the registry, and persisted to SQLite.
    fn authorize_recruitment(
        caller: &RunContext,
        target: &EngineAgent,
    ) -> Result<(), ToolExecutionError> {
        let caller_auth = caller.authority_level;
        let target_auth = RoleAuthorityLevel::from_role(&target.identity.role);
        let is_exec_delegate = caller.agent_id == crate::agent::constants::AGENT_CEO
            || caller.agent_id == crate::agent::constants::AGENT_COO;

        // Prevent horizontal/upward privilege escalation (e.g. Specialist recruiting Executive/CEO).
        if target_auth > caller_auth && !is_exec_delegate {
            return Err(ToolExecutionError::SecurityBlocked(format!(
                "Authority violation: agent '{}' ({:?}) cannot recruit higher-authority agent '{}' ({:?})",
                caller.agent_id, caller_auth, target.identity.id, target_auth
            )));
        }

        // Enforce Mandate tool allowlist subset when caller runs under a scoped Mandate.
        if let Some(ref allowed) = caller.mandate.allowed_tools {
            if !allowed.is_empty() {
                for skill in &target.capabilities.skills {
                    if is_privileged_mutating_skill(skill)
                        && !allowed.iter().any(|a| a == skill || a == "*" || a == "all")
                    {
                        return Err(ToolExecutionError::SecurityBlocked(format!(
                            "Mandate privilege violation: target '{}' holds '{}' outside caller Mandate",
                            target.identity.id, skill
                        )));
                    }
                }
            }
        }

        // For non-management callers with explicit skill allowlists, forbid recruiting an agent
        // with privileged mutating skills the caller itself does not possess.
        if caller_auth < RoleAuthorityLevel::Management && !caller.skills.is_empty() {
            let caller_wildcard = caller.skills.iter().any(|s| s == "*" || s == "all");
            if !caller_wildcard {
                for skill in &target.capabilities.skills {
                    if is_privileged_mutating_skill(skill) && !caller.skills.contains(skill) {
                        return Err(ToolExecutionError::SecurityBlocked(format!(
                            "Privilege escalation blocked: target '{}' holds privileged skill '{}' not held by '{}'",
                            target.identity.id, skill, caller.agent_id
                        )));
                    }
                }
            }
        }

        Ok(())
    }

    pub(crate) async fn ensure_sub_agent_exists(
        &self,
        opts: SubAgentOptions<'_>,
    ) -> Result<String, ToolExecutionError> {
        let valid_requested_id = validate_slug(opts.agent_id, "agent_id")?;

        // Process approved extra skills (C6): validate schema, forbid privileged escalation
        // without explicit Allow policy, and never overwrite user-owned skills.
        let mut approved_skill_names: Vec<String> = Vec::new();
        if let Some(skills) = opts.extra_skills {
            let existing_snapshot = self.state.registry.skills.snapshot();
            let caller_id = opts.caller_ctx.map_or("", |c| c.agent_id.as_str());
            for s in skills {
                let raw_name = s
                    .get("name")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        ToolExecutionError::ExecutionFailed(
                            "PROTOCOL_VIOLATION: extra_skills item missing 'name'".to_string(),
                        )
                    })?;
                let skill_name = validate_slug(raw_name, "extra_skills[].name")?;
                if is_privileged_mutating_skill(skill_name) {
                    let allowed_by_policy = if !caller_id.is_empty() {
                        self.state
                            .security
                            .permission_policy
                            .get_mode(skill_name, caller_id)
                            .await
                            == PermissionMode::Allow
                    } else {
                        false
                    };
                    if !allowed_by_policy {
                        return Err(ToolExecutionError::SecurityBlocked(format!(
                            "Privilege escalation blocked: skill '{}' requires explicit governance approval",
                            skill_name
                        )));
                    }
                }
                if existing_snapshot
                    .skills
                    .get(skill_name)
                    .is_some_and(|existing| existing.category == "user")
                {
                    tracing::warn!(
                        "🛡️ [Swarm] Skipping capability overwrite of user-owned skill '{}'",
                        skill_name
                    );
                    approved_skill_names.push(skill_name.to_string());
                    continue;
                }
                if let Ok(name) = self
                    .state
                    .registry
                    .skills
                    .register_capability("skill", s.clone(), "ai")
                    .await
                {
                    tracing::info!("Registered discovered skill: {}", name);
                }
                approved_skill_names.push(skill_name.to_string());
            }
        }

        let mut approved_workflow_names: Vec<String> = Vec::new();
        if let Some(workflows) = opts.extra_workflows {
            for w in workflows {
                if let Some(w_name) = w.get("name").and_then(|v| v.as_str()) {
                    let valid_wf = validate_slug(w_name, "extra_workflows[].name")?;
                    approved_workflow_names.push(valid_wf.to_string());
                }
                if let Ok(name) = self
                    .state
                    .registry
                    .skills
                    .register_capability("workflow", w.clone(), "ai")
                    .await
                {
                    tracing::info!("Registered discovered workflow: {}", name);
                }
            }
        }

        // Fast-path when exact key exists cleanly and no new skills/workflows need attaching.
        if approved_skill_names.is_empty() && approved_workflow_names.is_empty() {
            if let Some(existing) = self.state.registry.agents.get(valid_requested_id) {
                if let Some(caller) = opts.caller_ctx {
                    Self::authorize_recruitment(caller, existing.value())?;
                }
                return Ok(valid_requested_id.to_string());
            }
        }

        // Deterministic Tiered Recruitment Search (C2):
        // Tier 0 = "user" specialists, Tier 1 = "ai" pool.
        // Score 0 = exact ID, Score 1 = exact name, Score 2 = role substring (only when needle.len() >= 3).
        let needle = valid_requested_id.to_ascii_lowercase();
        let mut candidates: Vec<(u8, u8, String, EngineAgent)> = Vec::new();
        for kv in self.state.registry.agents.iter() {
            let a = kv.value();
            let score = if a.identity.id.to_ascii_lowercase() == needle {
                Some(0)
            } else if a.identity.name.to_ascii_lowercase() == needle {
                Some(1)
            } else if needle.len() >= 3 && a.identity.role.to_lowercase().contains(&needle) {
                Some(2)
            } else {
                None
            };
            if let Some(s) = score {
                let tier = match a.identity.category.as_str() {
                    "user" => 0,
                    "ai" => 1,
                    _ => 2,
                };
                candidates.push((tier, s, a.identity.id.clone(), a.clone()));
            }
        }
        candidates.sort_by(|a, b| (a.0, a.1, &a.2).cmp(&(b.0, b.1, &b.2)));

        if let Some((_tier, _score, matched_id, _)) = candidates.into_iter().next() {
            let _lifecycle = self.state.registry.lock_agent(&matched_id).await;
            let mut matched_agent = self
                .state
                .registry
                .agents
                .get(&matched_id)
                .ok_or_else(|| AppError::NotFound("Recruited agent was deleted".to_string()))?
                .clone();

            if let Some(caller) = opts.caller_ctx {
                Self::authorize_recruitment(caller, &matched_agent)?;
            }

            tracing::info!(
                "🎯 [Swarm] Priority Match: Found existing specialist '{}' ({}) in category '{}' to fulfill request for '{}'",
                matched_agent.identity.name,
                matched_agent.identity.id,
                matched_agent.identity.category,
                valid_requested_id
            );

            let already_participated = matched_agent
                .metadata
                .get("has_participated_in_swarm")
                .and_then(|v| v.as_bool())
                == Some(true);
            let mut needs_persist = !already_participated;
            matched_agent.metadata.insert(
                "has_participated_in_swarm".to_string(),
                serde_json::Value::Bool(true),
            );
            if matched_agent.health.status == "idle" {
                matched_agent.health.status = "active".to_string();
                needs_persist = true;
            }
            for s in &approved_skill_names {
                if !matched_agent.capabilities.skills.contains(s) {
                    matched_agent.capabilities.skills.push(s.clone());
                    needs_persist = true;
                }
            }
            for w in &approved_workflow_names {
                if !matched_agent.capabilities.workflows.contains(w) {
                    matched_agent.capabilities.workflows.push(w.clone());
                    needs_persist = true;
                }
            }

            if needs_persist {
                let exists: bool =
                    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM agents WHERE id = ?1)")
                        .bind(&matched_id)
                        .fetch_one(&self.state.resources.pool)
                        .await
                        .map_err(AppError::from)?;
                matched_agent.version = if exists {
                    crate::agent::persistence::save_agent_db(
                        &self.state.resources.pool,
                        &matched_agent,
                    )
                    .await?
                } else {
                    crate::agent::persistence::create_agent_db(
                        &self.state.resources.pool,
                        &matched_agent,
                    )
                    .await?
                };
            }
            self.state
                .registry
                .agents
                .insert(matched_id.clone(), matched_agent);
            return Ok(matched_id);
        }

        let target_id = valid_requested_id.to_string();
        tracing::info!("🛠️ [Swarm] Registering missing sub-agent: {}", target_id);
        // 🛡️ [M32: Principle of Least Privilege] Auto-fabricated subagents receive minimal read-only defaults
        // without financial exfiltration primitives or mutating tools unless explicitly approved.
        let mut base_skills = vec![
            "fetch_url".to_string(),
            "read_file".to_string(),
            "list_files".to_string(),
            "get_file_contents".to_string(),
            "grep_search".to_string(),
            "get_agent_metrics".to_string(),
            "complete_mission".to_string(),
        ];

        for name in approved_skill_names {
            if !base_skills.contains(&name) {
                base_skills.push(name);
            }
        }

        let target_lower = target_id.to_lowercase();
        let (initial_role, initial_dept, initial_desc) = if let Some(r) = opts.role_override {
            (
                r.to_string(),
                "Tactical Operations".to_string(),
                format!("Specialized agent with role override: {}", r),
            )
        } else if target_lower.contains("research")
            || target_lower.contains("search")
            || target_lower.contains("scout")
            || target_lower.contains("recon")
        {
            (
                "Swarm Research Specialist".to_string(),
                "Intelligence".to_string(),
                "Expert in web discovery, data extraction, and information synthesis.".to_string(),
            )
        } else if target_lower.contains("code")
            || target_lower.contains("dev")
            || target_lower.contains("rust")
            || target_lower.contains("py")
            || target_lower.contains("engineer")
            || target_lower.contains("script")
        {
            (
                "Swarm Code Specialist".to_string(),
                "Engineering".to_string(),
                "Expert in Rust, TypeScript, and system architecture.".to_string(),
            )
        } else if target_lower.contains("audit")
            || target_lower.contains("complian")
            || target_lower.contains("sec")
            || target_lower.contains("qa")
            || target_lower.contains("test")
        {
            (
                "Swarm Security Auditor".to_string(),
                "Compliance".to_string(),
                "Expert in vulnerability scanning, budget enforcement, and protocol verification."
                    .to_string(),
            )
        } else if target_lower.contains("alpha")
            || target_lower.contains("lead")
            || target_lower.contains("commander")
            || target_lower.contains("coord")
        {
            (
                "Swarm Mission Commander".to_string(),
                "Operations".to_string(),
                "The Alpha Node, responsible for coordinating multi-agent missions.".to_string(),
            )
        } else if target_lower.contains("arch") || target_lower.contains("design") {
            (
                "Swarm Systems Architect".to_string(),
                "Architecture".to_string(),
                "Expert in high-level system design and component decomposition.".to_string(),
            )
        } else {
            (
                "AI-General Intelligence Node".to_string(),
                "Swarm Core".to_string(),
                "Autonomous sub-agent spawned for specific task resolution.".to_string(),
            )
        };

        // Clone parent's ModelConfig cleanly (H9) while clearing api_key so raw credentials
        // are not replicated into SQLite agent rows (C5); runtime payloads pass api_key in memory.
        let mut sub_model_config = opts.parent_config.clone();
        sub_model_config.api_key = None;

        let budget_usd = match opts.caller_ctx {
            Some(c) if c.budget_limit_usd > 0.0 => (c.budget_limit_usd - c.current_cost_usd)
                .clamp(0.0, DEFAULT_SUBAGENT_BUDGET_CAP_USD),
            _ => DEFAULT_SUBAGENT_BUDGET_CAP_USD,
        };

        let mut sub_agent = EngineAgent {
            identity: crate::agent::types::AgentIdentity {
                id: target_id.clone(),
                name: target_id.clone(),
                role: initial_role,
                department: initial_dept,
                description: initial_desc,
                category: "ai".to_string(),
                theme_color: Some("#4fd1c5".to_string()),
            },
            models: crate::agent::types::AgentModels {
                model_id: Some(sub_model_config.model_id.clone()),
                model: sub_model_config,
                model_2: None,
                model_3: None,
                model_config2: None,
                model_config3: None,
                active_model_slot: Some(1),
            },
            economics: crate::agent::types::AgentEconomics {
                budget_usd,
                cost_usd: 0.0,
                tokens_used: 0,
                token_usage: crate::agent::types::TokenUsage::default(),
            },
            health: crate::agent::types::AgentHealth {
                status: "idle".to_string(),
                failure_count: 0,
                last_failure_at: None,
                heartbeat_at: None,
            },
            capabilities: crate::agent::types::AgentCapabilities {
                skills: base_skills,
                workflows: approved_workflow_names,
                mcp_tools: vec![],
                skill_manifest: None,
            },
            state: crate::agent::types::AgentState {
                active_mission: None,
                current_task: None,
                working_memory: serde_json::json!({}),
                current_reasoning_turn: 0,
            },
            ..Default::default()
        };

        let _lifecycle = self.state.registry.lock_agent(&target_id).await;
        if self.state.registry.agents.contains_key(&target_id) {
            return Ok(target_id);
        }
        sub_agent.version =
            crate::agent::persistence::create_agent_db(&self.state.resources.pool, &sub_agent)
                .await?;
        self.state.registry.agents.insert(target_id.clone(), sub_agent);
        Ok(target_id)
    }
}

// Metadata: [recruitment]

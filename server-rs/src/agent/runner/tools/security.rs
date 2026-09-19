//! @docs ARCHITECTURE:Registry
//! 
//! ### AI Assist Note
//! **Core technical resource for the Tadpole OS Sovereign infrastructure.**
//! This module implements high-fidelity logic for the Sovereign Reality layer.
//! 
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Runtime logic error, state desynchronization, or resource exhaustion.
//! - **Telemetry Link**: Search `[security]` in tracing logs.

use crate::agent::runner::RunContext;
use crate::agent::types::ToolCall;
use crate::agent::runner::AgentRunner;
// use crate::agent::constants::{AGENT_CEO, AGENT_COO, AGENT_ALPHA};
use super::error::ToolExecutionError;

pub struct ValidationResult {
    pub oversight_required: bool,
    pub oversight_reason: String,
}

#[async_trait::async_trait]
pub trait SecurityManager: Send + Sync {
    async fn pre_validate(&self, runner: &AgentRunner, ctx: &RunContext, fc: &ToolCall) -> Result<ValidationResult, ToolExecutionError>;
}

pub struct DefaultSecurityManager;

#[async_trait::async_trait]
impl SecurityManager for DefaultSecurityManager {
    async fn pre_validate(&self, runner: &AgentRunner, ctx: &RunContext, fc: &ToolCall) -> Result<ValidationResult, ToolExecutionError> {
        let mut trigger_oversight = false;
        let mut oversight_reason = String::new();
        let normalized_name = super::normalize_tool_name(&fc.name);

        // 1. [CBS] Skill-Based Security Allowlist
        if let Some(agent) = runner.state.registry.agents.get(&ctx.agent_id) {
            let allowed_skills = &agent.value().capabilities.skills;
            let is_builtin = super::registry::BUILTIN_TOOLS.contains(&fc.name.as_str()) || super::registry::BUILTIN_TOOLS.contains(&normalized_name.as_str());
            let is_orchestrator = ctx.agent_id == crate::agent::constants::AGENT_CEO || ctx.agent_id == crate::agent::constants::AGENT_COO;

            let is_allowed = allowed_skills.is_empty() 
                || allowed_skills.contains(&fc.name) 
                || allowed_skills.contains(&normalized_name)
                || allowed_skills.iter().any(|s| s == "all" || s == "execution" || s == "*");

            if !is_orchestrator && !is_builtin && !is_allowed {
                tracing::warn!("🛡️ [CBS] Agent {} attempted unauthorized skill: {} (normalized: {})", ctx.agent_id, fc.name, normalized_name);
                runner.broadcast_sys(
                    &format!("🛡️ CBS: {} attempted unauthorized skill: {}", ctx.name, fc.name),
                    "error",
                    Some(ctx.mission_id.clone()),
                );
                return Err(ToolExecutionError::SecurityBlocked(format!("Skill '{}' not in agent allowlist", fc.name)));
            }

            // 2. [Hierarchy Guard] Enforce strategic delegation for CEO/COO
            if matches!(fc.name.as_str(), "spawn_subagent" | "recruit_specialist") || matches!(normalized_name.as_str(), "spawn_subagent" | "recruit_specialist") {
                if ctx.agent_id == crate::agent::constants::AGENT_CEO {
                    tracing::warn!("🛡️ [Hierarchy Guard] CEO (ID: {}) blocked from spawning specialists directly.", crate::agent::constants::AGENT_CEO);
                    runner.broadcast_sys("🛡️ Hierarchy Guard: CEO blocked from direct worker recruitment. Use 'issue_alpha_directive' instead.", "warning", Some(ctx.mission_id.clone()));
                    return Err(ToolExecutionError::HierarchyBlocked("As CEO, you are prohibited from direct worker recruitment. You MUST use 'issue_alpha_directive' to delegate complex missions to the COO.".to_string()));
                }
                if ctx.agent_id == crate::agent::constants::AGENT_COO {
                    let target = fc.args.get("agent_id").and_then(|v| v.as_str()).unwrap_or("");
                    if target != crate::agent::constants::AGENT_ALPHA {
                        tracing::warn!("🛡️ [Hierarchy Guard] COO (ID: {}) blocked from spawning specialist '{}' directly.", crate::agent::constants::AGENT_COO, target);
                        runner.broadcast_sys("🛡️ Hierarchy Guard: COO blocked from direct worker recruitment. Use Alpha Node commander instead.", "warning", Some(ctx.mission_id.clone()));
                        return Err(ToolExecutionError::HierarchyBlocked("As COO, you are prohibited from direct worker recruitment. You MUST recruit an Alpha Node (ID: alpha) to serve as Swarm Mission Commander.".to_string()));
                    }
                }
            }
        }

        // 3. [Dynamic Policy] Check SQLite-backed PermissionPolicy
        let mut policy_mode = {
            let mode = runner.state.security.permission_policy.get_mode(&fc.name, &ctx.agent_id).await;
            if mode == crate::security::permissions::PermissionMode::Prompt && normalized_name != fc.name {
                let norm_mode = runner.state.security.permission_policy.get_mode(&normalized_name, &ctx.agent_id).await;
                if norm_mode != crate::security::permissions::PermissionMode::Prompt {
                    norm_mode
                } else {
                    mode
                }
            } else {
                mode
            }
        };

        // If the mode defaulted to Prompt (no explicit rule in permission_policies)
        // and Auto-Approve Safe Skills is enabled, allow safe read-only skills to pass through.
        let has_explicit = runner.state.security.permission_policy.has_explicit_policy(&fc.name, &ctx.agent_id)
            || runner.state.security.permission_policy.has_explicit_policy(&normalized_name, &ctx.agent_id);

        if policy_mode == crate::security::permissions::PermissionMode::Prompt && !has_explicit {
            let is_safe = is_safe_skill(&fc.name, &normalized_name, runner);
            let auto_approve = runner.state.governance.auto_approve_safe_skills.load(std::sync::atomic::Ordering::Relaxed);
            if is_safe && auto_approve {
                tracing::info!("⚡ [Security] Auto-confirming safe skill pass-through for '{}'", fc.name);
                policy_mode = crate::security::permissions::PermissionMode::Allow;
            }
        }

        match policy_mode {
            crate::security::permissions::PermissionMode::Deny => {
                return Err(ToolExecutionError::SecurityBlocked(format!("Policy for '{}' is set to DENY", fc.name)));
            }
            crate::security::permissions::PermissionMode::Prompt => {
                trigger_oversight = true;
                oversight_reason = format!("Sovereign Policy requires 'Prompt' for tool: {}", fc.name);
            }
            crate::security::permissions::PermissionMode::Allow => {}
        }

        if !trigger_oversight {
            // 4. [Security Gate] Skill Manifest Validation
            let mut manifest_requires = false;

            if let Some(manifest) = runner.state.registry.skill_registry.get(&fc.name).or_else(|| runner.state.registry.skill_registry.get(&normalized_name)) {
                if manifest.requires_oversight {
                    manifest_requires = true;
                }
            }
            if !manifest_requires {
                let requires_oversight = {
                    let snapshot = runner.state.registry.skills.snapshot();
                    snapshot.skills.get(&fc.name).or_else(|| snapshot.skills.get(&normalized_name)).map(|s| s.oversight_required).unwrap_or(false)
                };
                if requires_oversight {
                    manifest_requires = true;
                }
            }

            if manifest_requires {
                trigger_oversight = true;
                oversight_reason = format!("Security Gate triggered by manifest for: {}", fc.name);
            }
        }

        // 5. [Agent-Level Oversight]
        if let Some(agent) = runner.state.registry.agents.get(&ctx.agent_id) {
            if agent.value().requires_oversight {
                trigger_oversight = true;
                oversight_reason = format!("Mandatory oversight enabled for agent: {}", ctx.name);
            }
        }

        Ok(ValidationResult {
            oversight_required: trigger_oversight,
            oversight_reason,
        })
    }
}

/// Determines whether a skill or tool call represents a safe, read-only inspection operation.
fn is_safe_skill(name: &str, normalized_name: &str, runner: &AgentRunner) -> bool {
    let is_builtin_safe = matches!(
        name,
        "read_file"
            | "read_codebase_file"
            | "list_files"
            | "list_dir"
            | "grep_search"
            | "get_current_time"
            | "calculate"
            | "share_finding"
            | "update_working_memory"
            | "get_file_contents"
            | "get_project_status"
            | "get_agent_metrics"
            | "get_current_mission_status"
            | "search_global_vault"
            | "list_skill_metadata"
    ) || matches!(
        normalized_name,
        "read_file"
            | "read_codebase_file"
            | "list_files"
            | "list_dir"
            | "grep_search"
            | "get_current_time"
            | "calculate"
            | "share_finding"
            | "update_working_memory"
            | "get_file_contents"
            | "get_project_status"
            | "get_agent_metrics"
            | "get_current_mission_status"
            | "search_global_vault"
            | "list_skill_metadata"
    );

    if is_builtin_safe {
        return true;
    }

    if let Some(manifest) = runner.state.registry.skill_registry.get(name).or_else(|| runner.state.registry.skill_registry.get(normalized_name)) {
        if !manifest.requires_oversight && manifest.danger_level == crate::agent::skill_manifest::DangerLevel::Low {
            return true;
        }
    }

    let snapshot = runner.state.registry.skills.snapshot();
    if let Some(skill) = snapshot.skills.get(name).or_else(|| snapshot.skills.get(normalized_name)) {
        if !skill.oversight_required {
            return true;
        }
    }

    false
}

// Metadata: [security]


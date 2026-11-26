//! @docs ARCHITECTURE:Runner
//!
//! ### AI Assist Note
//! **Dynamic Skill & Script Fallback Runner**: Probes `execution/` for deterministic
//! on-disk scripts when no registered skill matches, enforces governance permission
//! policies, invokes `McpHost::call_tool`, runs output sanitization, and executes
//! post-mutation verification scripts.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Skill denied by policy, sanitizer security halt, or post-verification script failure.
//! - **Telemetry Link**: Search `[dynamic_skills]` in tracing logs.

use super::error::ToolExecutionError;
use crate::agent::runner::{AgentRunner, RunContext};
use crate::error::AppError;

impl AgentRunner {
    /// Probes the workspace `execution/` directory for a deterministic script matching `normalized_name`
    /// and synthesizes an oversight-gated `SkillDefinition` if found.
    pub(super) async fn resolve_script_fallback_skill(
        ws_root: &std::path::Path,
        normalized_name: &str,
    ) -> Option<crate::agent::script_skills::SkillDefinition> {
        let script_candidates = [
            ws_root.join("execution").join(format!("{}.py", normalized_name)),
            ws_root.join("execution").join(format!("{}.sh", normalized_name)),
            ws_root.join("execution").join(format!("{}.ps1", normalized_name)),
            ws_root
                .join("execution")
                .join("agent_generated")
                .join("skills")
                .join(format!("{}.py", normalized_name)),
        ];

        let mut existing_candidate = None;
        for cand in &script_candidates {
            if tokio::fs::metadata(cand).await.is_ok() {
                existing_candidate = Some(cand.clone());
                break;
            }
        }

        let candidate = existing_candidate?;
        let ext = candidate
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("py");
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

        Some(crate::agent::script_skills::SkillDefinition {
            id: None,
            name: normalized_name.to_string(),
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
        })
    }

    /// Handles execution of dynamic file-based skills via the MCP Host with
    /// sanitization hooks and optional post-mutation verification scripts.
    pub(super) async fn handle_dynamic_skill(
        &self,
        ctx: &RunContext,
        fc: &crate::agent::types::ToolCall,
        output_text: &mut String,
        skill: &crate::agent::script_skills::SkillDefinition,
        usage: &mut Option<crate::agent::types::TokenUsage>,
        oversight_approved: bool,
    ) -> Result<(), AppError> {
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

        if (skill.oversight_required
            || perm_mode == crate::security::permissions::PermissionMode::Prompt)
            && perm_mode != crate::security::permissions::PermissionMode::Allow
            && !oversight_approved
        {
            *output_text = format!(
                "Skill '{}' requires explicit human oversight approval or 'Allow' permission mode.",
                skill.name
            );
            return Ok(());
        }

        let snapshot = self.state.registry.skills.snapshot();
        let tool_ctx = self.build_tool_context(ctx);

        let result = self
            .state
            .registry
            .mcp_host
            .call_tool(&skill.name, fc.args.clone(), &tool_ctx, &snapshot.skills)
            .await;

        match result {
            Ok(crate::agent::mcp::McpResult::Raw(output)) => {
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
                            *output_text = format!(
                                "({} MUTATION COMPLETED BUT VERIFICATION FAILED):\n{}\n\n[VERIFICATION ERROR]: {}",
                                skill.name, final_output, e
                            );
                            return Err(AppError::InternalServerError(format!(
                                "Skill '{}' executed mutation but failed post-verification: {}",
                                skill.name, e
                            )));
                        }
                    }
                }

                *output_text = format!(
                    "({} EXECUTED SUCCESSFULLY):\n\n{}",
                    skill.name, final_output
                );
            }
            Ok(crate::agent::mcp::McpResult::SystemDelegate(name, args))
                if name == "recruit_specialist" =>
            {
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
                let res = self
                    .handle_spawn_subagent(ctx, &mapped_fc, usage)
                    .await
                    .map_err(|e| match e {
                        ToolExecutionError::AppError(ae) => ae,
                        _ => AppError::InternalServerError(e.to_string()),
                    })?;
                output_text.push_str(&res);
            }
            Ok(crate::agent::mcp::McpResult::SystemDelegate(_, _)) => {}
            Err(e) => {
                *output_text = format!("(SKILL EXEC FAILED: {})", e);
            }
        }
        Ok(())
    }
}

// Metadata: [dynamic_skills]

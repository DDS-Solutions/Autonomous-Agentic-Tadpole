//! @docs ARCHITECTURE:Runner
//!
//! ### AI Assist Note
//! **Sandboxed Shell & Script-Builder Handlers**: Isolates the highest-risk built-in
//! execution primitives (`handle_execute_shell` with mandatory quote-aware validation,
//! `ShellScanner`, Sapphire Gate HITL oversight, resource arbiter permit, and sandbox
//! dispatch; plus `handle_script_builder` multi-step batch orchestration).
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Zero-trust shell validator block, Sapphire Gate rejection, or sandbox host denial.
//! - **Telemetry Link**: Search `[shell]` or `[ScriptBuilder]` in tracing logs.

use crate::agent::runner::{AgentRunner, RunContext};
use crate::error::AppError;

impl AgentRunner {
    /// Recursively executes a batch of tool calls provided by the LLM.
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
            .ok_or_else(|| {
                AppError::BadRequest("'steps' must be an array in script_builder".to_string())
            })?;

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
    /// Protected by zero-trust command validation, `ShellScanner`, Sapphire Gate oversight,
    /// resource arbiter permit, and sandboxed shell execution.
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

        tracing::info!(
            "💻 [System] Agent {} requesting shell execution: {}",
            ctx.agent_id,
            command_str
        );

        // 1. Quote-aware zero-trust command validation (blocks quoted/escaped interpreter flags & chaining)
        if let Err(e) = crate::utils::security::validate_shell_command(command_str) {
            tracing::warn!(
                "🛡️ [Security] Shell execution BLOCKED by zero-trust validator: {}",
                e
            );
            *output_text = format!("(SECURITY BLOCKED: {})", e);
            return Ok(());
        }

        match self.state.security.shell_scanner.scan(command_str) {
            crate::security::scanner::ScannerResult::Risky(reason) => {
                tracing::warn!(
                    "🛡️ [Security] Shell execution BLOCKED by advanced scanner: {}",
                    reason
                );
                *output_text = format!("(SECURITY BLOCKED: {})", reason);
                return Ok(());
            }
            crate::security::scanner::ScannerResult::Safe => {}
        }

        self.broadcast_agent(
            ctx,
            &format!(
                "💎 Oversight: wants to run terminal command: {}. CRITICAL REVIEW REQUIRED.",
                command_str
            ),
            "error",
        );

        // 2. Sapphire Gate Oversight — execute_shell is NEVER auto-approved via auto_approve_safe_skills
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

        self.broadcast_agent(
            ctx,
            &format!("💻 System: running '{}'...", command_str),
            "info",
        );

        let _permit = self
            .state
            .resources
            .arbiter
            .acquire()
            .await
            .map_err(|e| AppError::InternalServerError(format!("Resource arbiter failure: {}", e)))?;

        // 3. Execution via Security Sandbox
        let sandbox_config = crate::security::sandbox::SandboxConfig::default();
        match crate::security::sandbox::execute_sandboxed_shell(
            command_str,
            &ctx.workspace_root,
            &sandbox_config,
        )
        .await
        {
            Ok(output) => {
                let truncated = self.safe_truncate(&output, 5000);
                *output_text = format!("(SHELL OUTPUT of '{}'):\n\n{}", command_str, truncated);
            }
            Err(e) => {
                tracing::warn!(
                    "🛡️ [Security] Sandboxed shell execution blocked or failed: {}",
                    e
                );
                *output_text = format!("(SHELL EXECUTION FAILED: {})", e);
            }
        }

        Ok(())
    }
}

// Metadata: [shell]

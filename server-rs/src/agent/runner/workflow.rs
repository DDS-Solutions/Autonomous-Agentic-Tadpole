//! @docs ARCHITECTURE:Runner
//!
//! ### AI Assist Note
//! **Deterministic Workflow**: Orchestrates step-by-step execution of
//! Markdown-based SOPs. Replaces the stochastic "think-act" loop with high-fidelity
//! procedural logic, ensuring perfect adherence to business processes while
//! optimizing token usage for SME (Subject Matter Expert) workloads.
//!
//! ### 🔍 Debugging & Observability
//! - **Telemetry Link**: Search `[workflow]` in tracing logs.
//! - **Failure Path**: SOP parsing error, step execution timeout, or
//!   unexpected tool failure during a deterministic step.
//! - **Trace Scope**: `server-rs::agent::runner::workflow`
//!

use crate::agent::types::TaskPayload;
use crate::error::AppError;

use super::AgentRunner;

impl AgentRunner {
    // ─────────────────────────────────────────────────────────
    //  DETERMINISTIC WORKFLOW
    // ─────────────────────────────────────────────────────────

    /// Orchestrates the step-by-step execution of a deterministic markdown workflow.
    ///
    /// This method replaces the standard "reason-act" loop with a fixed sequence
    /// of instructions derived from the SOP file. This ensures perfect fidelity
    /// to business processes while reducing token overhead for SME workloads.
    pub(crate) async fn run_deterministic_workflow(
        &self,
        ctx: &super::RunContext,
        payload: TaskPayload,
        workflow: &mut crate::agent::workflows::WorkflowExecutionState,
        usage: &mut Option<crate::agent::types::TokenUsage>,
    ) -> Result<String, AppError> {
        let mut final_out = String::new();
        let mut total_usage = crate::agent::types::TokenUsage::default();

        self.broadcast_agent(
            &ctx,
            &format!(
                "📜 [SOP] Starting deterministic workflow: {}",
                workflow.workflow_name
            ),
            "info",
        );

        let mut accumulated_results = String::new();

        // 2. Step-by-Step Execution
        while let Some(step) = workflow.current_step() {
            self.broadcast_agent(
                &ctx,
                &format!(
                    "🔹 [Step {}/{}] {}",
                    workflow.current_step_index + 1,
                    workflow.steps.len(),
                    step.title
                ),
                "info",
            );

            self.update_status(
                &ctx.agent_id,
                &ctx.mission_id,
                "working",
                Some(&format!("Executing SOP Step: {}", step.title)),
            );

            // Synthesize a prompt for this specific step
            let mut step_prompt = format!(
                "You are currently executing Step {} of the '{}' workflow.\n\n### GLOBAL MISSION DIRECTIVE:\n{}\n\n### STEP TITLE: {}\n### INSTRUCTION:\n{}\n\n",
                workflow.current_step_index + 1,
                workflow.workflow_name,
                payload.primary_goal.as_deref().unwrap_or(&payload.message),
                step.title,
                step.instruction
            );

            if !accumulated_results.is_empty() {
                step_prompt.push_str(&format!(
                    "### PREVIOUS STEPS FINDINGS:\n{}\n\n",
                    accumulated_results
                ));
            }

            step_prompt.push_str(
                "### TASK:\nFollow the instruction above to achieve the global mission. ",
            );
            step_prompt.push_str(
                "DO NOT repeat work that has already been accomplished in previous steps. ",
            );
            step_prompt.push_str(
                "Provide a concise report of your actions and findings for this specific step.",
            );

            let mut step_payload = payload.clone();
            step_payload.message = step_prompt;

            // Execute via standard intelligence loop (supports tools)
            match self.execute_intelligence_loop(&ctx, &step_payload).await {
                Ok(out) => {
                    final_out.push_str(&format!("\n\n---\n## Step: {}\n{}", step.title, out.text));
                    accumulated_results.push_str(&format!(
                        "\n[Step {}: {}]\n{}\n",
                        workflow.current_step_index + 1,
                        step.title,
                        out.text
                    ));

                    if let Some(u) = out.usage {
                        total_usage.input_tokens = total_usage.input_tokens.saturating_add(u.input_tokens);
                        total_usage.output_tokens = total_usage.output_tokens.saturating_add(u.output_tokens);
                        total_usage.total_tokens = total_usage.total_tokens.saturating_add(u.total_tokens);
                        *usage = Some(total_usage.clone());
                    }
                    workflow.advance();
                }
                Err((e, turn_usage)) => {
                    if let Some(u) = turn_usage {
                        total_usage.input_tokens = total_usage.input_tokens.saturating_add(u.input_tokens);
                        total_usage.output_tokens = total_usage.output_tokens.saturating_add(u.output_tokens);
                        total_usage.total_tokens = total_usage.total_tokens.saturating_add(u.total_tokens);
                    }
                    *usage = Some(total_usage);
                    return Err(e);
                }
            }
        }

        // 3. Finalize
        self.broadcast_agent(
            &ctx,
            &format!("✅ [SOP] Workflow completed: {}", workflow.workflow_name),
            "success",
        );

        let _ = self
            .finalize_run(&ctx, &final_out, &Some(total_usage))
            .await?;
        Ok(final_out)
    }
}





// Metadata: [workflow]

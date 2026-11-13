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

use crate::agent::types::{TaskPayload, TokenUsage};
use crate::error::AppError;
use serde::{Deserialize, Serialize};

use super::AgentRunner;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DurableWorkflowStepOutput {
    text: String,
    usage: Option<TokenUsage>,
}

impl AgentRunner {
    // ─────────────────────────────────────────────────────────
    //  DETERMINISTIC WORKFLOW
    // ─────────────────────────────────────────────────────────

    /// Orchestrates the step-by-step execution of a deterministic markdown workflow
    /// backed by SQLite durable step memoization for crash-safe resumption.
    pub(crate) async fn run_deterministic_workflow(
        &self,
        ctx: &super::RunContext,
        payload: TaskPayload,
        workflow: &mut crate::agent::workflows::WorkflowExecutionState,
        usage: &mut Option<TokenUsage>,
    ) -> Result<String, AppError> {
        let mut final_out = String::new();
        let mut total_usage = TokenUsage::default();
        let workflow_id = payload
            .workflow_id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToString::to_string)
            .unwrap_or_else(|| format!("wf:{}:{}", ctx.mission_id, workflow.workflow_name));

        crate::agent::durable::start_workflow(
            &self.state.resources.pool,
            &workflow_id,
            Some(&ctx.mission_id),
            &ctx.agent_id,
        )
        .await?;

        self.broadcast_agent(
            ctx,
            &format!(
                "📜 [SOP] Starting deterministic workflow: {} (id: {})",
                workflow.workflow_name, workflow_id
            ),
            "info",
        );

        let mut accumulated_results = String::new();

        // 2. Step-by-Step Execution
        while let Some(step) = workflow.current_step().cloned() {
            let step_index = workflow.current_step_index as i64;
            self.broadcast_agent(
                ctx,
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
            step_payload.message = step_prompt.clone();

            let durable_input = serde_json::json!({
                "workflow_name": workflow.workflow_name,
                "step_index": step_index,
                "step_id": step.id,
                "title": step.title,
                "prompt": step_prompt,
            });
            let failed_turn_usage = std::sync::Arc::new(parking_lot::Mutex::new(None));
            let failed_turn_usage_ref = failed_turn_usage.clone();

            // Execute via durable step memoization + standard intelligence loop (supports tools)
            match crate::agent::durable::execute_durable_step(
                &self.state.resources.pool,
                &workflow_id,
                step_index,
                &step.title,
                &durable_input,
                || async {
                    match self.execute_intelligence_loop(ctx, &step_payload).await {
                        Ok(out) => Ok(DurableWorkflowStepOutput {
                            text: out.text,
                            usage: out.usage,
                        }),
                        Err((e, turn_usage)) => {
                            *failed_turn_usage_ref.lock() = turn_usage;
                            Err(e)
                        }
                    }
                },
            )
            .await
            {
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
                Err(e) => {
                    if let Some(u) = failed_turn_usage.lock().take() {
                        total_usage.input_tokens = total_usage.input_tokens.saturating_add(u.input_tokens);
                        total_usage.output_tokens = total_usage.output_tokens.saturating_add(u.output_tokens);
                        total_usage.total_tokens = total_usage.total_tokens.saturating_add(u.total_tokens);
                    }
                    *usage = Some(total_usage);
                    let _ = crate::agent::durable::fail_workflow(
                        &self.state.resources.pool,
                        &workflow_id,
                        &e.to_string(),
                    )
                    .await;
                    return Err(e);
                }
            }
        }

        // 3. Finalize
        crate::agent::durable::complete_workflow(&self.state.resources.pool, &workflow_id).await?;
        self.broadcast_agent(
            ctx,
            &format!("✅ [SOP] Workflow completed: {}", workflow.workflow_name),
            "success",
        );

        let _ = self
            .finalize_run(ctx, &final_out, &Some(total_usage))
            .await?;
        Ok(final_out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    #[tokio::test]
    async fn test_durable_workflow_skips_completed_steps_on_resume() {
        let provider_initial =
            crate::test_support::ProviderStub::start(axum::http::StatusCode::OK, "step done", false).await;
        let state = crate::test_support::state_with_agent("durable-owner", &provider_initial.base_url).await;
        let dir = state.base_dir.join("data/workflows");
        tokio::fs::create_dir_all(&dir).await.unwrap();
        tokio::fs::write(
            dir.join("resume_sop.md"),
            "## First step\nCollect baseline.\n## Second step\nVerify output.\n",
        )
        .await
        .unwrap();
        state
            .registry
            .agents
            .get_mut("durable-owner")
            .unwrap()
            .capabilities
            .workflows = vec!["resume_sop".to_string()];

        let runner = AgentRunner::new(state.clone());
        let first_out = runner
            .run(
                "durable-owner".to_string(),
                TaskPayload {
                    message: "/workflow resume_sop".to_string(),
                    workflow_id: Some("durable-wf-resume-test".to_string()),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert!(first_out.contains("First step") && first_out.contains("Second step"));
        assert_eq!(provider_initial.calls.load(Ordering::SeqCst), 4);

        // Resume with a stub that would fail if any step re-invoked the provider
        let provider_resume =
            crate::test_support::ProviderStub::start(axum::http::StatusCode::INTERNAL_SERVER_ERROR, "should not be called", false)
                .await;
        {
            let mut agent = state.registry.agents.get_mut("durable-owner").unwrap();
            agent.models.model.base_url = Some(provider_resume.base_url.clone());
        }

        let resumed_out = runner
            .run(
                "durable-owner".to_string(),
                TaskPayload {
                    message: "/workflow resume_sop".to_string(),
                    workflow_id: Some("durable-wf-resume-test".to_string()),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(resumed_out, first_out);
        assert_eq!(
            provider_resume.calls.load(Ordering::SeqCst),
            0,
            "Memoized durable steps must replay from SQLite without invoking the LLM provider"
        );

        let wf_status: String =
            sqlx::query_scalar("SELECT status FROM durable_workflows WHERE workflow_id = 'durable-wf-resume-test'")
                .fetch_one(&state.resources.pool)
                .await
                .unwrap();
        assert_eq!(wf_status, "COMPLETED");
    }
}





// Metadata: [workflow]

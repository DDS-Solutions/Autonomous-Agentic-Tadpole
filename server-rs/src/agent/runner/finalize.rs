//! @docs ARCHITECTURE:Runner
//!
//! ### AI Assist Note
//! **Run Finalization (Post-Mission Orchestrator)**: Orchestrates the
//! result persistence, memory archival, and cleanup for the Tadpole
//! OS agent runner. Features **Cumulative Token Accounting**:
//! updates the global agent state and budget guard with final usage
//! metrics (TPM/Cost). Implements **Durable Mission Finalization**:
//! commits the final output and status (Completed/Failed) to the
//! SQLite mission ledger. Includes **Automatic Memory Archival**:
//! spawns background tasks to index mission results into the
//! LanceDB vector store for long-term institutional knowledge. AI
//! agents should monitor the `finalize_run` telemetry to confirm
//! successful archival before closing the mission context (RUN-03).
//!
//! ### 🔍 Debugging & Observability
//! - **Telemetry Link**: Search `[finalize]` in tracing logs.
//! - **Failure Path**: Partial persistence due to database locks during
//!   finalization, memory archival failures in LanceDB, or
//!   telemetry broadcast drops causing UI desync.
//! - **Trace Scope**: `server-rs::agent::runner::finalize`

use super::analysis::spawn_post_mission_analysis;
use super::{AgentRunner, RunContext};
use crate::error::AppError;

impl AgentRunner {
    // ─────────────────────────────────────────────────────────
    //  FINALIZATION
    // ─────────────────────────────────────────────────────────

    /// Finalizes the run: updates token usage, persists mission state, broadcasts results.
    pub(crate) async fn finalize_run(
        &self,
        ctx: &RunContext,
        output_text: &str,
        usage: &Option<crate::agent::types::TokenUsage>,
    ) -> Result<String, AppError> {
        tracing::info!(
            "🔔 [DIAGNOSTIC] Finalizing run for agent {}. Content length: {}",
            ctx.agent_id,
            output_text.len()
        );
        tracing::info!(
            "✅ [Runner] Provider responded successfully ({} tokens). Output length: {}",
            usage.as_ref().map(|u| u.total_tokens).unwrap_or(0),
            output_text.len()
        );
        if output_text.is_empty() {
            tracing::warn!(
                "⚠️ [Runner] final_delivery is EMPTY for agent {}",
                ctx.agent_id
            );
        }
        tracing::debug!(
            "DEBUG [Runner] final_delivery content: {:?}",
            crate::utils::security::redact_secrets(output_text)
        );

        self.persist_terminal_state(ctx, usage, false, output_text).await?;

        // Delivery formatting
        let mut final_delivery = output_text.trim().to_string();
        if final_delivery.is_empty() {
            final_delivery =
                "(Agent completed its actions without a final conversational response.)"
                    .to_string();
        } else if let Some(placeholder) = detect_unresolved_placeholders(&final_delivery) {
            tracing::warn!(
                "⚠️ [Finalize] Detected unresolved placeholder/confabulation '{}' in output of agent {}",
                placeholder,
                ctx.agent_id
            );
            final_delivery = format!(
                "{}\n\n⚠️ [System Note: The preceding response contained ungrounded template placeholder '{}'. System has logged this discrepancy.]",
                final_delivery,
                placeholder
            );
        }

        self.broadcast_agent_message(&ctx.agent_id, &ctx.mission_id, &final_delivery);

        #[cfg(feature = "vector-memory")]
        self.archive_lance_db_memory(ctx, output_text);

        // 🧠 MISSION ANALYSIS TRIGGER
        if ctx.analysis && ctx.agent_id != "99" {
            spawn_post_mission_analysis(self.clone(), ctx.clone(), output_text.to_string());
        }

        Ok(final_delivery)
    }

    /// Saves the final output to the agent's permanent institutional knowledge in LanceDB.
    #[cfg(feature = "vector-memory")]
    fn archive_lance_db_memory(&self, ctx: &RunContext, output_text: &str) {
        let (_, agent_memory_dir, _) = ctx.resolve_paths();
        let mem_output = output_text.to_string();
        let mem_mission_id = ctx.mission_id.clone();
        let api_key = ctx.model_config.api_key.clone().unwrap_or_default();
        let http_client = self.state.resources.http_client.clone();

        let dedupe_threshold = std::env::var("LANCEDB_DEDUPE_THRESHOLD")
            .unwrap_or_else(|_| "0.2".to_string())
            .parse::<f32>()
            .unwrap_or(0.2);

        tokio::spawn(async move {
            match crate::agent::memory::VectorMemory::connect(&agent_memory_dir, "memories").await {
                Ok(mem) => {
                    if let Ok(vec) = crate::agent::memory::get_gemini_embedding(
                        &http_client,
                        &api_key,
                        &mem_output,
                    )
                    .await
                    {
                        match mem
                            .check_memory_duplicate(vec.clone(), dedupe_threshold)
                            .await
                        {
                            Ok(true) => {
                                tracing::info!("🧠 [Memory] Duplicate detected (dist < {}), skipping LanceDB insertion for mission {}", dedupe_threshold, mem_mission_id);
                            }
                            _ => {
                                let id = uuid::Uuid::new_v4().to_string();
                                let _ =
                                    mem.add_memory(&id, &mem_output, &mem_mission_id, vec).await;
                                tracing::info!(
                                    "🧠 [Memory] Archived final result to LanceDB for mission {}",
                                    mem_mission_id
                                );
                            }
                        }
                    }
                }
                Err(e) => tracing::error!("❌ [Memory] Failed to archive LanceDB memory: {}", e),
            }
        });
    }

    /// Centralized mission failure handler. Sets status, logs error, and updates agent health.
    pub(crate) async fn fail_mission(
        &self,
        ctx: &RunContext,
        e: &AppError,
        usage: &Option<crate::agent::types::TokenUsage>,
    ) -> Result<(), AppError> {
        let safe_error = self
            .state
            .security
            .secret_redactor
            .redact(&format!("{}", e));
        tracing::error!(
            "❌ [Runner] Mission failure for agent {}: {}",
            ctx.agent_id,
            safe_error
        );
        self.broadcast_agent_message(
            &ctx.agent_id,
            &ctx.mission_id,
            &format!("❌ Error: {}", safe_error),
        );

        self.persist_terminal_state(ctx, usage, true, &format!("Error: {}", safe_error)).await?;

        Ok(())
    }

    /// Handles mission cancellation (e.g. from the Gateway). 
    /// Ensures status is reverted and mission is marked as 'aborted'.
    pub(crate) async fn fail_mission_aborted(
        &self,
        ctx: &RunContext,
    ) -> Result<(), AppError> {
        tracing::warn!(
            "🛑 [Runner] Mission aborted for agent {}. Cleaning up...",
            ctx.agent_id
        );

        self.persist_terminal_state(ctx, &ctx.accounting.usage(), true, "Mission aborted by kernel/user").await?;

        Ok(())
    }
}

/// Scans output for unresolved prompt placeholders or hallucinated template tags.
pub fn detect_unresolved_placeholders(text: &str) -> Option<&'static str> {
    const SUSPICIOUS_PATTERNS: &[&str] = &[
        "[Current System Date]",
        "[Current System Time]",
        "[Current Date]",
        "[Current Time]",
        "[Current System",
        "**[Current System",
        "<execute_tool>",
        "<tool_call>",
    ];

    SUSPICIOUS_PATTERNS
        .iter()
        .copied()
        .find(|&pattern| text.contains(pattern))
}







#[cfg(test)]
mod tests {
    use crate::state::AppState;
    use crate::agent::runner::{AgentRunner, RunContext};
    use crate::agent::types::{TokenUsage, EngineAgent};
    use std::sync::Arc;

    #[tokio::test]
    async fn test_finalize_run_metrics() {
        let state = Arc::new(AppState::new_minimal_mock().await);
        let runner = AgentRunner::new(state.clone());
        let mut ctx = RunContext::default();
        ctx.agent_id = "test-agent".to_string();
        
        // Setup mock agent
        let mut agent = EngineAgent::default();
        agent.identity.id = ctx.agent_id.clone();
        state.registry.agents.insert(ctx.agent_id.clone(), agent.clone());

        // Sync agent to DB first so create_mission doesn't fail
        agent.version = crate::agent::persistence::create_agent_db(&state.resources.pool, &agent).await.unwrap();
        state.registry.agents.insert(ctx.agent_id.clone(), agent.clone());

        // Create a mock mission in the DB to satisfy persistence calls
        let mission = crate::agent::mission::create_mission(
            &state.resources.pool,
            &ctx.agent_id,
            "Test Mission",
            1.0
        ).await.unwrap();
        ctx.mission_id = mission.id;
        state.registry.agents.get_mut(&ctx.agent_id).unwrap().state.active_mission = Some(serde_json::json!({"id": ctx.mission_id}));

        let usage = Some(TokenUsage {
            input_tokens: 100,
            output_tokens: 50,
            total_tokens: 150,
        });

        let result = runner.finalize_run(&ctx, "Final output", &usage).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Final output");

        // Verify metrics
        let agent = state.registry.agents.get(&ctx.agent_id).unwrap();
        assert_eq!(agent.economics.tokens_used, 150);
        assert!(agent.economics.cost_usd > 0.0);
        assert_eq!(agent.health.failure_count, 0);
    }

    #[test]
    fn test_detect_unresolved_placeholders() {
        use super::detect_unresolved_placeholders;
        assert_eq!(
            detect_unresolved_placeholders("The real, current time is: **[Current System Date]** at **[Current System Time]**"),
            Some("[Current System Date]")
        );
        assert_eq!(
            detect_unresolved_placeholders("Please run <execute_tool> now."),
            Some("<execute_tool>")
        );
        assert_eq!(
            detect_unresolved_placeholders("The current time is 2026-09-19 14:50:00 EDT."),
            None
        );
    }
}

// Metadata: [finalize]

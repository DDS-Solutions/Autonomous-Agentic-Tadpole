//! @docs ARCHITECTURE:Runner
//! ### AI Assist Note
//! Commits mission completion, agent state, and task usage together before publication.
//! ### 🔍 Debugging & Observability
//! - **Telemetry Link**: Search `[terminal]` in tracing logs.
//! - **Failure Path**: Superseded mission, stale version, or failed database transaction.

use super::{AgentRunner, RunContext};
use crate::{agent::types::TokenUsage, error::AppError};

impl AgentRunner {
    pub(crate) async fn persist_terminal_state(
        &self,
        ctx: &RunContext,
        usage: &Option<TokenUsage>,
        failed: bool,
        text: &str,
    ) -> Result<(), AppError> {
        let runner = self.clone();
        let ctx = ctx.clone();
        let usage = usage.clone();
        let text = text.to_string();
        let job_run_id = self.owning_job_run(&ctx);
        // Keep SQL commit, registry publication and buffered charging together
        // even when the caller is cancelled while waiting for the commit.
        tokio::spawn(async move {
            let _lifecycle = runner.state.registry.lock_agent(&ctx.agent_id).await;
            runner.persist_terminal_state_for_run_locked(&ctx, &usage, failed, &text, job_run_id).await
        }).await.map_err(|error| AppError::InternalServerError(format!("Terminal persistence task failed: {error}")))?
    }

    /// Caller holds the owning agent's lifecycle lock.
    pub(crate) async fn persist_terminal_state_locked(
        &self, ctx: &RunContext, usage: &Option<TokenUsage>, failed: bool, text: &str,
    ) -> Result<(), AppError> {
        self.persist_terminal_state_for_run_locked(ctx, usage, failed, text, self.owning_job_run(ctx)).await
    }

    fn owning_job_run(&self, ctx: &RunContext) -> Option<String> {
        ctx.accounting.job_run_id.clone().or_else(|| tokio::task::try_id().and_then(|task_id|
            self.state.comms.active_runners.iter()
                .find_map(|handle| (handle.task_id == task_id).then(|| handle.job_run_id.clone()).flatten())
        ))
    }

    async fn persist_terminal_state_for_run_locked(
        &self, ctx: &RunContext, usage: &Option<TokenUsage>, failed: bool, text: &str, job_run_id: Option<String>,
    ) -> Result<(), AppError> {
        let mut agent = self.state.registry.agents.get(&ctx.agent_id)
            .ok_or_else(|| AppError::NotFound(format!("Agent '{}' was deleted", ctx.agent_id)))?.clone();
        if agent.state.active_mission.as_ref().and_then(|m| m.get("id")).and_then(|id| id.as_str())
            != Some(ctx.mission_id.as_str()) {
            return Err(AppError::Conflict("Mission was superseded; terminal state was not applied".to_string()));
        }
        let cost = usage.as_ref().map(|u| crate::agent::rates::calculate_cost_with_custom_rates(
            ctx.model_config.input_cost_per_m, ctx.model_config.output_cost_per_m,
            &ctx.model_config.model_id, u.input_tokens, u.output_tokens,
        )).unwrap_or(0.0);
        if !cost.is_finite() || cost < 0.0 {
            return Err(AppError::BadRequest("Mission cost must be finite and non-negative".to_string()));
        }
        let task_id = agent.state.current_task.clone();
        if let Some(u) = usage {
            agent.economics.token_usage = u.clone();
            agent.economics.tokens_used = agent.economics.tokens_used.saturating_add(u.total_tokens);
        }
        agent.economics.cost_usd += cost;
        if failed {
            agent.health.failure_count = agent.health.failure_count.saturating_add(1);
            agent.health.last_failure_at = Some(chrono::Utc::now());
        } else {
            agent.health.failure_count = 0;
        }
        agent.state.active_mission = None;
        agent.state.current_task = None;
        if agent.health.status != "suspended" {
            agent.health.status = "idle".to_string();
        }

        let mut tx = self.state.resources.pool.begin().await?;
        let result = sqlx::query("UPDATE mission_history SET status = ?, cost_usd = cost_usd + ?, updated_at = ? WHERE id = ? AND agent_id = ? AND status IN ('pending', 'active')")
            .bind(if failed { "failed" } else { "completed" }).bind(cost).bind(chrono::Utc::now())
            .bind(&ctx.mission_id).bind(&ctx.agent_id).execute(&mut *tx).await?;
        if result.rows_affected() != 1 {
            return Err(AppError::Conflict("Mission is missing or already terminal".to_string()));
        }
        agent.version = crate::agent::persistence::save_agent_db_in_tx(&mut tx, &agent).await?;
        if let Some(job_run_id) = job_run_id {
            let result = sqlx::query("UPDATE scheduled_job_runs SET mission_id = COALESCE(mission_id, ?), cost_usd = cost_usd + ? WHERE id = ? AND (status = 'running' OR (status = 'failed' AND (mission_id IS NULL OR mission_id = ?)))")
                .bind(&ctx.mission_id).bind(cost).bind(job_run_id).bind(&ctx.mission_id).execute(&mut *tx).await?;
            if result.rows_affected() != 1 {
                return Err(AppError::Conflict("Scheduled run is missing or already terminal".to_string()));
            }
        }
        let mut log_text = serde_json::Value::String(text.to_string());
        crate::utils::serialization::prune_large_strings(&mut log_text, 2048);
        sqlx::query("INSERT INTO mission_logs (id, mission_id, agent_id, source, text, severity, timestamp) VALUES (?, ?, ?, ?, ?, ?, ?)")
            .bind(uuid::Uuid::new_v4().to_string()).bind(&ctx.mission_id).bind(&ctx.agent_id)
            .bind(if failed { "System" } else { "Agent" }).bind(log_text.as_str().unwrap_or_default())
            .bind(if failed { "error" } else { "success" }).bind(chrono::Utc::now())
            .execute(&mut *tx).await?;
        if let (Some(task_id), Some(u)) = (task_id, usage.as_ref()) {
            sqlx::query("UPDATE agent_tasks SET tokens_in = tokens_in + ?, tokens_out = tokens_out + ?, cost_usd = cost_usd + ?, provider_id = COALESCE(provider_id, ?) WHERE id = ? AND agent_id = ?")
                .bind(u.input_tokens as i64).bind(u.output_tokens as i64).bind(cost)
                .bind(ctx.model_config.provider.to_string()).bind(task_id).bind(&ctx.agent_id)
                .execute(&mut *tx).await?;
        }
        tx.commit().await?;
        self.state.comms.mission_contexts.remove(&ctx.mission_id);
        tracing::info!(agent_id = %ctx.agent_id, mission_id = %ctx.mission_id, "[terminal] Durable terminal state committed");
        self.state.registry.agents.insert(ctx.agent_id.clone(), agent.clone());
        // This records into the existing quota buffer synchronously; no detached save or OCC retry.
        self.state.security.budget_guard.record_usage(&ctx.agent_id, cost).await
            .map_err(|error| AppError::InternalServerError(error.to_string()))?;
        self.state.emit_event(serde_json::json!({
            "type": "agent:update", "agent_id": ctx.agent_id, "data": agent
        }));
        self.broadcast_agent_status(&ctx.agent_id, &ctx.mission_id, &agent.health.status);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{agent::types::EngineAgent, state::AppState};
    use std::sync::Arc;

    async fn fixture() -> (Arc<AppState>, AgentRunner, RunContext) {
        let state = Arc::new(AppState::new_minimal_mock().await);
        let mut agent = EngineAgent::default();
        agent.identity.id = "terminal-agent".to_string();
        agent.version = crate::agent::persistence::create_agent_db(&state.resources.pool, &agent).await.unwrap();
        let mission = crate::agent::mission::create_mission(&state.resources.pool, &agent.identity.id, "Terminal fixture", 1.0).await.unwrap();
        agent.state.active_mission = Some(serde_json::json!({"id": mission.id}));
        agent.health.status = "busy".to_string();
        agent.version = crate::agent::persistence::save_agent_db(&state.resources.pool, &agent).await.unwrap();
        let ctx = RunContext { agent_id: agent.identity.id.clone(), mission_id: mission.id, ..Default::default() };
        state.registry.agents.insert(agent.identity.id.clone(), agent);
        (state.clone(), AgentRunner::new(state), ctx)
    }

    #[tokio::test]
    async fn superseded_completion_preserves_new_mission_and_usage() {
        let (state, runner, ctx) = fixture().await;
        state.registry.agents.get_mut(&ctx.agent_id).unwrap().state.active_mission = Some(serde_json::json!({"id": "replacement"}));
        let usage = Some(TokenUsage { input_tokens: 10, output_tokens: 20, total_tokens: 30 });
        assert!(matches!(runner.persist_terminal_state(&ctx, &usage, false, "old result").await, Err(AppError::Conflict(_))));
        let agent = state.registry.agents.get(&ctx.agent_id).unwrap();
        assert_eq!(agent.state.active_mission.as_ref().unwrap()["id"], "replacement");
        assert_eq!(agent.health.status, "busy");
        assert_eq!(agent.economics.tokens_used, 0);
    }

    #[tokio::test]
    async fn failed_agent_write_rolls_back_mission_and_leaves_memory_unchanged() {
        let (state, runner, ctx) = fixture().await;
        sqlx::query("CREATE TRIGGER reject_terminal BEFORE UPDATE ON agents BEGIN SELECT RAISE(ABORT, 'injected agent write failure'); END")
            .execute(&state.resources.pool).await.unwrap();
        assert!(runner.persist_terminal_state(&ctx, &None, false, "result").await.is_err());
        assert_eq!(state.registry.agents.get(&ctx.agent_id).unwrap().health.status, "busy");
        assert_eq!(sqlx::query_scalar::<_, String>("SELECT status FROM mission_history WHERE id = ?")
            .bind(&ctx.mission_id).fetch_one(&state.resources.pool).await.unwrap(), "pending");
        assert_eq!(sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM mission_logs WHERE mission_id = ?")
            .bind(&ctx.mission_id).fetch_one(&state.resources.pool).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn duplicate_completion_cannot_double_charge() {
        let (state, runner, ctx) = fixture().await;
        let usage = Some(TokenUsage { input_tokens: 10, output_tokens: 20, total_tokens: 30 });
        runner.persist_terminal_state(&ctx, &usage, false, "result").await.unwrap();
        assert!(matches!(runner.persist_terminal_state(&ctx, &usage, false, "result").await, Err(AppError::Conflict(_))));
        assert_eq!(state.registry.agents.get(&ctx.agent_id).unwrap().economics.tokens_used, 30);
        assert_eq!(sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM mission_logs WHERE mission_id = ?")
            .bind(&ctx.mission_id).fetch_one(&state.resources.pool).await.unwrap(), 1);
    }

    #[tokio::test]
    async fn scheduled_usage_is_linked_to_the_exact_run_before_completion() {
        let (state, runner, ctx) = fixture().await;
        sqlx::query("INSERT INTO scheduled_jobs (id, agent_id, name, prompt, cron_expr, next_run_at, created_at) VALUES ('usage-job', ?, 'Fixture', 'fixture', '* * * * *', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)")
            .bind(&ctx.agent_id).execute(&state.resources.pool).await.unwrap();
        let run = crate::agent::continuity::scheduler::create_job_run(&state.resources.pool, "usage-job").await.unwrap();
        let lifecycle = state.registry.lock_agent(&ctx.agent_id).await;
        let mission_id = ctx.mission_id.clone();
        let execution = crate::routes::agent::spawn_agent_execution(state.clone(), ctx.agent_id.clone(), async move {
            runner.persist_terminal_state(&ctx, &Some(TokenUsage { input_tokens: 100, output_tokens: 50, total_tokens: 150 }), false, "result").await?;
            Ok("done".to_string())
        });
        state.comms.active_runners.get_mut("terminal-agent").unwrap().job_run_id = Some(run.id.clone());
        drop(lifecycle);
        execution.await.unwrap().unwrap();
        let (recorded_mission, cost): (Option<String>, f64) = sqlx::query_as("SELECT mission_id, cost_usd FROM scheduled_job_runs WHERE id = ?")
            .bind(&run.id).fetch_one(&state.resources.pool).await.unwrap();
        assert_eq!(recorded_mission.as_deref(), Some(mission_id.as_str()));
        assert!(cost > 0.0);
    }

    #[tokio::test]
    async fn finding_regression_terminal_commit_survives_cancelled_caller() {
        let (state, runner, ctx) = fixture().await;
        let lifecycle = state.registry.lock_agent(&ctx.agent_id).await;
        let owned_ctx = ctx.clone();
        let caller = tokio::spawn(async move {
            runner.persist_terminal_state(&owned_ctx, &Some(TokenUsage { input_tokens: 10, output_tokens: 20, total_tokens: 30 }), false, "result").await
        });
        tokio::task::yield_now().await;
        assert!(!caller.is_finished());
        caller.abort();
        assert!(caller.await.unwrap_err().is_cancelled());
        drop(lifecycle);
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            while state.registry.agents.get(&ctx.agent_id).unwrap().state.active_mission.is_some() {
                tokio::task::yield_now().await;
            }
        }).await.unwrap();
        assert_eq!(sqlx::query_scalar::<_, String>("SELECT status FROM mission_history WHERE id = ?")
            .bind(&ctx.mission_id).fetch_one(&state.resources.pool).await.unwrap(), "completed");
        assert_eq!(state.registry.agents.get(&ctx.agent_id).unwrap().economics.tokens_used, 30);
        assert_eq!(sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM mission_logs WHERE mission_id = ?")
            .bind(&ctx.mission_id).fetch_one(&state.resources.pool).await.unwrap(), 1);
    }
}

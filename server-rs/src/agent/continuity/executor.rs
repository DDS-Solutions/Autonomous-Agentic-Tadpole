//! Background execution engine for scheduled agent missions and workflows.
//!
//! @docs ARCHITECTURE:Continuity
//!
//! ### AI Assist Note
//! **The Continuity Engine**: Drives the autonomous lifecycle of the swarm.
//! Polling every 60s, it awakens dormant agents to perform scheduled missions.
//!
//! ### 🔍 Debugging & Observability
//! - **Telemetry Link**: Search `[executor]` in tracing logs.
//! - **Trace Scope**: `server-rs::agent::continuity::executor`

use chrono::Utc;
use std::sync::Arc;
use std::time::Duration;

use super::scheduler::{complete_job_run, create_job_run, get_due_jobs, record_job_tick};
use super::types::JobRunStatus;
use crate::agent::types::TaskPayload;
use crate::state::AppState;
use crate::error::AppError;

/// Starts the continuity scheduler background task.
///
/// Runs every 60 seconds, looking for `scheduled_jobs` rows where `next_run_at <= now`.
/// For each due job, it spawns a mission via `AgentRunner::run()` and records the run.
///
/// Safety guarantees baked in:
/// - Each agent is limited to 1 concurrent scheduled job (skip if agent is "busy").
/// - Each job has a `budget_usd` cap enforced by the existing `AgentRunner` budget logic.
/// - Consecutive failures auto-disable a job after `max_failures` runs.
/// - Token burn: controlled by the agent's model rate limits.
pub async fn start_scheduler(state: Arc<AppState>) {
    tracing::info!("🕐 [Continuity] Scheduler started — checking for due jobs every 60 seconds.");

    let mut interval = tokio::time::interval(Duration::from_secs(60));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        interval.tick().await;

        if let Err(e) = tick(&state).await {
            tracing::error!("❌ [Continuity] Scheduler tick error: {}", e);
        }

        // Reaper: Clean up orphans every tick (60s)
        if let Err(error) = crate::agent::persistence::reap_stale_agents(&state, 300).await {
            tracing::error!(%error, "[Continuity] Reaper failed");
        }
    }
}

/// A single scheduler tick: fetch due jobs and execute them.
///
/// This function performs the following steps:
/// 1. Queries the database for all enabled jobs that are due for execution.
/// 2. Skips execution if no jobs are due.
/// 3. Spawns an asynchronous tokio task for each due job to prevent blocking the ticker.
async fn tick(state: &Arc<AppState>) -> Result<(), AppError> {
    let due_jobs = get_due_jobs(&state.resources.pool).await?;

    if due_jobs.is_empty() {
        return Ok(());
    }

    tracing::info!(
        "🕐 [Continuity] {} job(s) due for execution.",
        due_jobs.len()
    );

    for job in due_jobs {
        let state_clone = Arc::clone(state);
        let job_clone = job.clone();

        tokio::spawn(async move {
            if let Err(error) = execute_job(state_clone, job_clone).await {
                tracing::error!(%error, "[Continuity] Scheduled job failed");
            }
        });
    }

    Ok(())
}

/// Executes a single scheduled job:
/// 1. Atomically claims the agent → skip if busy (prevents concurrency conflicts).
/// 2. Creates a run record.
/// 3. Calls `AgentRunner::run()` with the job's prompt and budget.
/// 4. Finalises the run record and advances cron to next window.
async fn execute_job(state: Arc<AppState>, job: super::types::ScheduledJob) -> Result<(), AppError> {
    let now = Utc::now();
    let lifecycle = state.registry.lock_agent(&job.agent_id).await;
    tracing::info!(
        "🚀 [Continuity] Starting job '{}' for agent '{}' (budget: ${:.3})",
        job.name,
        job.agent_id,
        job.budget_usd
    );

    // 1. Skip if agent is busy (prevents overlapping missions) - ATOMIC CLAIM
    let claimed = crate::agent::persistence::claim_agent(&state.resources.pool, &job.agent_id).await?;

    if !claimed {
        tracing::warn!(
            "⏭ [Continuity] Skipping job '{}' — agent '{}' is currently busy or claim failed.",
            job.name,
            job.agent_id
        );

        let run = create_job_run(&state.resources.pool, &job.id).await?;
        complete_job_run(&state.resources.pool, &run.id, None, JobRunStatus::Skipped,
            0.0, Some("Agent was busy when job was scheduled.")).await?;
        return Ok(());
    }

    // 2. Create run record
    let run = match create_job_run(&state.resources.pool, &job.id).await {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(
                "❌ [Continuity] Failed to create run record for job '{}': {}",
                job.id,
                e
            );
            if let Err(error) = sqlx::query("UPDATE agents SET status = 'idle' WHERE id = ? AND status = 'busy'")
                .bind(&job.agent_id).execute(&state.resources.pool).await {
                tracing::error!(%error, "[Continuity] Failed to release claim after run-record failure");
            }
            return Err(e);
        }
    };

    // 3. Branch: Workflow vs Atomic Agent Mission
    let execution = if let Some(workflow_id) = &job.workflow_id {
        let engine = super::workflow::WorkflowEngine::new(Arc::clone(&state));
        let workflow_id = workflow_id.clone();
        crate::routes::agent::spawn_agent_execution(state.clone(), job.agent_id.clone(), async move {
            engine.run_workflow(&workflow_id, serde_json::json!({})).await
        })
    } else {
        let payload = TaskPayload {
            message: job.prompt.clone(), department: Some(format!("Continuity: {}", job.name)),
            budget_usd: Some(job.budget_usd), safe_mode: Some(false), analysis: Some(false),
            ..Default::default()
        };
        crate::routes::agent::spawn_agent_runner(state.clone(), job.agent_id.clone(), payload)
    };
    if let Some(mut handle) = state.comms.active_runners.get_mut(&job.agent_id) {
        handle.job_run_id = Some(run.id.clone());
    }
    drop(lifecycle);
    let execution_id = execution.id();
    let result = match execution.await {
        Ok(result) => result,
        Err(error) => {
            // Retained by mission identity even when a replacement has already
            // taken the agent's tracking slot. Both recovery paths use this receipt.
            let received_context = state.comms.mission_contexts.iter()
                .find_map(|ctx| (ctx.accounting.task_id == Some(execution_id)).then(|| ctx.clone()));
            // Aborting a Tokio task skips its ordinary runner cleanup. Recover only
            // this task's mission; a replacement runner must keep its state/handle.
            let context = if received_context.is_some() { received_context } else {
                let _lifecycle = state.registry.lock_agent(&job.agent_id).await;
                let owns_runner = state.comms.active_runners.get(&job.agent_id)
                    .is_some_and(|handle| handle.task_id == execution_id);
                let agent = owns_runner.then(|| state.registry.agents.get(&job.agent_id).map(|agent| agent.clone())).flatten();
                if let Some(agent) = agent {
                    if let Some(mission_id) = agent.state.active_mission.as_ref()
                        .and_then(|mission| mission.get("id")).and_then(|id| id.as_str()) {
                        let mut context = state.comms.mission_contexts.get(mission_id).map(|ctx| ctx.clone())
                            .unwrap_or_else(|| agent.resolve_provider_context(state.base_dir.clone()));
                        context.mission_id = mission_id.to_string();
                        Some(context)
                    } else {
                        if let Err(cleanup_error) = crate::agent::persistence::update_and_persist_agent(&state, &job.agent_id, |agent| {
                            if agent.health.status != "suspended" { agent.health.status = "idle".to_string(); }
                            agent.state.current_task = None;
                        }).await {
                            tracing::error!(%cleanup_error, "[Continuity] Cancelled claim cleanup failed");
                        }
                        None
                    }
                } else { None }
            };
            if let Some(context) = context {
                // The runner's Drop guard may already have finalized this mission.
                // Terminal persistence rejects a replaced or already terminal mission.
                if let Err(cleanup_error) = crate::agent::runner::AgentRunner::new(state.clone())
                    .fail_mission_aborted(&context).await {
                    if !matches!(cleanup_error, AppError::Conflict(_) | AppError::NotFound(_)) {
                        tracing::error!(%cleanup_error, "[Continuity] Cancelled mission cleanup failed");
                    }
                }
            }
            state.comms.active_runners.remove_if(&job.agent_id, |_, handle| handle.task_id == execution_id);
            Err(AppError::InternalServerError(format!("Scheduled execution cancelled or panicked: {}", error)))
        }
    };

    // 4. Finalise run record and advance cron
    let (success, output_summary, status) = match result {
        Ok(output) => {
            let summary = output.chars().take(500).collect::<String>();
            (
                true,
                Some(summary),
                JobRunStatus::Completed,
            )
        }
        Err(e) => {
            tracing::error!("❌ [Continuity] Job '{}' failed: {}", job.name, e);
            let msg = format!("Error: {}", e);
            (false, Some(msg), JobRunStatus::Failed)
        }
    };

    // Terminal transactions reconcile usage against this exact server-created run.
    let (mission_id, cost_usd): (Option<String>, f64) = sqlx::query_as(
        "SELECT mission_id, cost_usd FROM scheduled_job_runs WHERE id = ?"
    ).bind(&run.id).fetch_one(&state.resources.pool).await?;

    complete_job_run(
        &state.resources.pool,
        &run.id,
        mission_id.as_deref(),
        status,
        cost_usd,
        output_summary.as_deref(),
    )
    .await?;

    record_job_tick(
        &state.resources.pool,
        &job.id,
        &job.cron_expr,
        success,
        job.max_failures,
    )
    .await?;

    // Emit WebSocket event
    state.emit_event(serde_json::json!({
        "type": "engine:scheduled_job_complete",
        "data": {
            "job_id": job.id,
            "job_name": job.name,
            "agent_id": job.agent_id,
            "success": success,
            "timestamp": now.to_rfc3339(),
        }
    }));

    tracing::info!(
        "✅ [Continuity] Job '{}' finished (success={})",
        job.name,
        success
    );
    Ok(())
}

#[allow(dead_code)]
pub async fn execute_job_by_id(state: Arc<AppState>, job_id: &str) -> Result<(), AppError> {
    let job_opt = super::scheduler::get_job_by_id(&state.resources.pool, job_id).await?;
    if let Some(job) = job_opt {
        if !job.enabled { return Err(AppError::Forbidden("Continuity job is disabled".to_string())); }
        execute_job(state, job).await?;
        Ok(())
    } else {
        Err(AppError::NotFound(format!("Continuity job '{}' not found", job_id)))
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_db;
    use crate::agent::continuity::scheduler::create_job;
    use crate::agent::continuity::types::CreateJobRequest;
    use crate::test_support::{state_with_agent, ProviderStub};
    use axum::http::StatusCode;
    use std::sync::atomic::Ordering;

    async fn regression_job(state: &AppState) -> super::super::types::ScheduledJob {
        create_job(&state.resources.pool, CreateJobRequest {
            agent_id: "scheduled-agent".to_string(), workflow_id: None,
            name: "Worker regression".to_string(), prompt: "Say hello".to_string(),
            cron_expr: "* * * * *".to_string(), budget_usd: Some(0.1),
            max_failures: None, metadata: None,
        }).await.unwrap()
    }

    #[tokio::test]
    async fn worker_regression_run_insert_failure_releases_claim() {
        let state = state_with_agent("scheduled-agent", "http://127.0.0.1:1/v1").await;
        let job = regression_job(&state).await;
        sqlx::query("CREATE TRIGGER reject_scheduled_run BEFORE INSERT ON scheduled_job_runs BEGIN SELECT RAISE(ABORT, 'injected run insertion failure'); END")
            .execute(&state.resources.pool).await.unwrap();

        assert!(matches!(execute_job(state.clone(), job).await, Err(AppError::Sqlx(_))));
        let status: String = sqlx::query_scalar("SELECT status FROM agents WHERE id = 'scheduled-agent'")
            .fetch_one(&state.resources.pool).await.unwrap();
        assert_eq!(status, "idle");
        assert_eq!(state.registry.agents.get("scheduled-agent").unwrap().health.status, "idle");
        assert!(state.comms.active_runners.is_empty());
        assert_eq!(sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM scheduled_job_runs")
            .fetch_one(&state.resources.pool).await.unwrap(), 0);
        assert!(crate::agent::persistence::claim_agent(&state.resources.pool, "scheduled-agent").await.unwrap());
    }

    #[tokio::test]
    async fn worker_regression_cancelled_execution_records_failure_and_cleans_up() {
        let provider = ProviderStub::start(StatusCode::OK, "Hello", true).await;
        let state = state_with_agent("scheduled-agent", &provider.base_url).await;
        let job = regression_job(&state).await;
        let executing = tokio::spawn(execute_job(state.clone(), job.clone()));
        provider.wait_for_call().await;
        let mission_id = state.registry.agents.get("scheduled-agent").unwrap()
            .state.active_mission.as_ref().unwrap()["id"].as_str().unwrap().to_string();
        state.comms.active_runners.get("scheduled-agent").unwrap().abort();

        tokio::time::timeout(Duration::from_secs(10), executing).await.unwrap().unwrap().unwrap();
        let (status, completed, summary): (String, Option<String>, Option<String>) = sqlx::query_as(
            "SELECT status, completed_at, output_summary FROM scheduled_job_runs WHERE job_id = ?"
        ).bind(&job.id).fetch_one(&state.resources.pool).await.unwrap();
        assert_eq!(status, "failed");
        assert!(completed.is_some());
        assert!(summary.unwrap().contains("cancelled or panicked"));
        assert_eq!(sqlx::query_scalar::<_, i64>("SELECT consecutive_failures FROM scheduled_jobs WHERE id = ?")
            .bind(&job.id).fetch_one(&state.resources.pool).await.unwrap(), 1);
        assert!(state.comms.active_runners.is_empty(), "cancelled runner must be evicted");
        assert_eq!(sqlx::query_scalar::<_, String>("SELECT status FROM agents WHERE id = 'scheduled-agent'")
            .fetch_one(&state.resources.pool).await.unwrap(), "idle");
        assert_eq!(sqlx::query_scalar::<_, String>("SELECT status FROM mission_history WHERE id = ?")
            .bind(mission_id).fetch_one(&state.resources.pool).await.unwrap(), "failed");
        assert!(state.registry.agents.get("scheduled-agent").unwrap().state.active_mission.is_none());
    }

    async fn cancelled_paid_execution(workflow: bool, preempt: bool, reject_recovery: bool) {
        let acknowledged_calls = if workflow { 2 } else { 1 };
        let provider = ProviderStub::start_blocking_at(StatusCode::OK, "step completed", acknowledged_calls).await;
        let state = state_with_agent("scheduled-agent", &provider.base_url).await;
        let mut job = regression_job(&state).await;
        if workflow {
            state.registry.agents.get_mut("scheduled-agent").unwrap().capabilities.workflows = vec!["paid-cancel".to_string()];
            let directory = state.base_dir.join("data/workflows");
            tokio::fs::create_dir_all(&directory).await.unwrap();
            tokio::fs::write(directory.join("paid-cancel.md"), "## First\nComplete first.\n## Second\nComplete second.\n").await.unwrap();
            job.prompt = "/workflow paid-cancel".to_string();
        }
        let executing = tokio::spawn(execute_job(state.clone(), job.clone()));
        provider.wait_for_calls(acknowledged_calls + 1).await;
        let mission_id = state.registry.agents.get("scheduled-agent").unwrap()
            .state.active_mission.as_ref().unwrap()["id"].as_str().unwrap().to_string();
        if reject_recovery {
            sqlx::query("CREATE TRIGGER reject_cancellation BEFORE UPDATE ON mission_history WHEN NEW.status = 'failed' BEGIN SELECT RAISE(ABORT, 'injected cancellation write failure'); END")
                .execute(&state.resources.pool).await.unwrap();
        }
        let replacement = if preempt {
            let lifecycle = state.registry.lock_agent("scheduled-agent").await;
            let execution = crate::routes::agent::spawn_agent_runner(state.clone(), "scheduled-agent".to_string(),
                TaskPayload { message: "Complete replacement".to_string(), ..Default::default() });
            drop(lifecycle);
            Some(execution)
        } else {
            state.comms.active_runners.get("scheduled-agent").unwrap().abort();
            None
        };
        tokio::time::timeout(Duration::from_secs(10), executing).await.unwrap().unwrap().unwrap();
        if let Some(replacement) = replacement {
            tokio::time::timeout(Duration::from_secs(10), replacement).await.unwrap().unwrap().unwrap();
        }
        if reject_recovery {
            assert_eq!(state.comms.mission_contexts.len(), 1, "failed cleanup must retain its received usage");
            assert!(state.registry.agents.get("scheduled-agent").unwrap().state.active_mission.is_some());
            assert_eq!(state.security.budget_guard.get_global_stats().1, 0.0);
            sqlx::query("DROP TRIGGER reject_cancellation").execute(&state.resources.pool).await.unwrap();
            let stale = chrono::Utc::now() - chrono::Duration::minutes(10);
            let lifecycle = state.registry.lock_agent("scheduled-agent").await;
            crate::agent::persistence::update_and_persist_agent(&state, "scheduled-agent", |agent| {
                agent.health.heartbeat_at = Some(stale);
            }).await.unwrap();
            drop(lifecycle);
            assert_eq!(crate::agent::persistence::reap_stale_agents(&state, 300).await.unwrap(), 1);
        }
        let expected_cost = acknowledged_calls as f64 * 0.00002;
        let (status, recorded_mission, cost): (String, Option<String>, f64) = sqlx::query_as(
            "SELECT status, mission_id, cost_usd FROM scheduled_job_runs WHERE job_id = ?"
        ).bind(&job.id).fetch_one(&state.resources.pool).await.unwrap();
        assert_eq!(status, "failed");
        assert_eq!(recorded_mission.as_deref(), Some(mission_id.as_str()));
        assert!((cost - expected_cost).abs() < 1e-12, "cancelled run lost received usage: {cost}");
        let (status, cost): (String, f64) = sqlx::query_as("SELECT status, cost_usd FROM mission_history WHERE id = ?")
            .bind(&mission_id).fetch_one(&state.resources.pool).await.unwrap();
        assert_eq!(status, "failed");
        assert!((cost - expected_cost).abs() < 1e-12);
        let total_calls = acknowledged_calls + if preempt { 2 } else { 0 };
        let agent = state.registry.agents.get("scheduled-agent").unwrap().clone();
        assert_eq!(agent.economics.tokens_used, total_calls as u32 * 15);
        assert!((agent.economics.cost_usd - total_calls as f64 * 0.00002).abs() < 1e-12);
        assert!((state.security.budget_guard.get_global_stats().1 - agent.economics.cost_usd).abs() < 1e-12);
        assert!(state.comms.mission_contexts.is_empty());
        assert!(state.comms.active_runners.is_empty());
        assert!(agent.state.active_mission.is_none());
        assert_eq!(agent.health.status, "idle");
    }

    #[tokio::test]
    async fn finding_regression_cancelled_partial_turn_retains_received_usage() {
        cancelled_paid_execution(false, false, false).await;
    }

    #[tokio::test]
    async fn finding_regression_cancelled_paid_workflow_retains_completed_step_usage() {
        cancelled_paid_execution(true, false, false).await;
    }

    #[tokio::test]
    async fn finding_regression_preemption_settles_old_usage_before_replacement() {
        cancelled_paid_execution(true, true, false).await;
    }

    #[tokio::test]
    async fn finding_regression_failed_cancellation_storage_retries_received_usage_once() {
        cancelled_paid_execution(false, false, true).await;
    }

    #[tokio::test]
    async fn worker_regression_cancelled_cleanup_preserves_a_replacement_runner() {
        let provider = ProviderStub::start(StatusCode::OK, "Hello", true).await;
        let state = state_with_agent("scheduled-agent", &provider.base_url).await;
        let job = regression_job(&state).await;
        let executing = tokio::spawn(execute_job(state.clone(), job.clone()));
        provider.wait_for_call().await;

        let lifecycle = state.registry.lock_agent("scheduled-agent").await;
        sqlx::query("UPDATE mission_history SET status = 'failed' WHERE agent_id = 'scheduled-agent'")
            .execute(&state.resources.pool).await.unwrap();
        sqlx::query("INSERT INTO mission_history (id, agent_id, title, status, budget_usd, cost_usd) VALUES ('replacement-mission', 'scheduled-agent', 'Replacement', 'active', 0.1, 0.0)")
            .execute(&state.resources.pool).await.unwrap();
        crate::agent::persistence::update_and_persist_agent(&state, "scheduled-agent", |agent| {
            agent.state.active_mission = Some(serde_json::json!({"id": "replacement-mission"}));
            agent.state.current_task = None;
            agent.health.status = "busy".to_string();
        }).await.unwrap();
        // The gateway aborts the old execution and registers this replacement.
        let replacement = crate::routes::agent::spawn_agent_execution(state.clone(), "scheduled-agent".to_string(),
            std::future::pending::<Result<String, AppError>>());
        let replacement_id = replacement.id();
        drop(lifecycle);

        tokio::time::timeout(Duration::from_secs(10), executing).await.unwrap().unwrap().unwrap();
        assert_eq!(state.comms.active_runners.get("scheduled-agent").unwrap().task_id, replacement_id);
        let agent = state.registry.agents.get("scheduled-agent").unwrap().clone();
        assert_eq!(agent.health.status, "busy");
        assert_eq!(agent.state.active_mission.unwrap()["id"], "replacement-mission");
        assert_eq!(sqlx::query_scalar::<_, String>("SELECT status FROM mission_history WHERE id = 'replacement-mission'")
            .fetch_one(&state.resources.pool).await.unwrap(), "active");
        assert_eq!(sqlx::query_scalar::<_, String>("SELECT status FROM agents WHERE id = 'scheduled-agent'")
            .fetch_one(&state.resources.pool).await.unwrap(), "busy");
        assert_eq!(sqlx::query_scalar::<_, String>("SELECT status FROM scheduled_job_runs WHERE job_id = ?")
            .bind(&job.id).fetch_one(&state.resources.pool).await.unwrap(), "failed");
        replacement.abort();
        assert!(replacement.await.unwrap_err().is_cancelled());
    }

    #[tokio::test]
    async fn worker_regression_competing_executions_do_not_preempt_the_owner() {
        let provider = ProviderStub::start(StatusCode::OK, "Hello", true).await;
        let state = state_with_agent("scheduled-agent", &provider.base_url).await;
        let job = regression_job(&state).await;
        let owner = tokio::spawn(execute_job(state.clone(), job.clone()));
        provider.wait_for_call().await;
        let task_id = state.comms.active_runners.get("scheduled-agent").unwrap().task_id;
        let mut competitors = Vec::new();
        for _ in 0..8 {
            competitors.push(tokio::spawn(execute_job(state.clone(), job.clone())));
        }
        for competitor in competitors {
            tokio::time::timeout(Duration::from_secs(10), competitor).await.unwrap().unwrap().unwrap();
        }
        assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
        assert_eq!(state.comms.active_runners.get("scheduled-agent").unwrap().task_id, task_id);
        let statuses: Vec<(String, i64)> = sqlx::query_as(
            "SELECT status, COUNT(*) FROM scheduled_job_runs GROUP BY status ORDER BY status"
        ).fetch_all(&state.resources.pool).await.unwrap();
        assert_eq!(statuses, vec![("running".to_string(), 1), ("skipped".to_string(), 8)]);
        provider.respond();
        tokio::time::timeout(Duration::from_secs(10), owner).await.unwrap().unwrap().unwrap();
        assert!(state.comms.active_runners.is_empty());
    }

    #[tokio::test]
    async fn worker_regression_success_records_usage_and_releases_agent() {
        let provider = ProviderStub::start(StatusCode::OK, "Hello", false).await;
        let state = state_with_agent("scheduled-agent", &provider.base_url).await;
        let job = regression_job(&state).await;
        tokio::time::timeout(Duration::from_secs(10), execute_job(state.clone(), job.clone())).await.unwrap().unwrap();
        let (status, mission_id, cost): (String, Option<String>, f64) = sqlx::query_as(
            "SELECT status, mission_id, cost_usd FROM scheduled_job_runs WHERE job_id = ?"
        ).bind(&job.id).fetch_one(&state.resources.pool).await.unwrap();
        assert_eq!(status, "completed");
        assert!(mission_id.is_some());
        let expected_cost = provider.calls.load(Ordering::SeqCst) as f64 * 0.00002;
        assert!(expected_cost > 0.0);
        assert!((cost - expected_cost).abs() < 1e-12, "reported cost {cost}, expected {expected_cost}");
        assert!(state.comms.active_runners.is_empty());
        assert_eq!(state.registry.agents.get("scheduled-agent").unwrap().health.status, "idle");
        assert_eq!(sqlx::query_scalar::<_, String>("SELECT status FROM agents WHERE id = 'scheduled-agent'")
            .fetch_one(&state.resources.pool).await.unwrap(), "idle");
    }

    #[tokio::test]
    async fn test_execute_job_busy_skip() -> Result<(), Box<dyn std::error::Error>> {
        let pool = init_db("sqlite::memory:").await?;
        let state = Arc::new(AppState::with_pool(pool.clone()).await);
        
        let req = CreateJobRequest {
            agent_id: "agent-1".to_string(),
            workflow_id: None,
            name: "Test Job".to_string(),
            prompt: "Say hello".to_string(),
            cron_expr: "* * * * *".to_string(),
            budget_usd: None,
            max_failures: None,
            metadata: None,
        };
        let job = create_job(&pool, req).await?;

        // 1. Manually claim agent (make it busy)
        sqlx::query("INSERT INTO agents (id, name, role, department, description, status, skills, workflows, mcp_tools, metadata) \
                     VALUES (?1, 'Agent 1', 'Specialist', 'Core', 'Desc', 'busy', '[]', '[]', '[]', '{}')")
            .bind(&job.agent_id)
            .execute(&pool)
            .await?;

        // 2. Execute job - should skip
        execute_job(Arc::clone(&state), job.clone()).await?;

        // 3. Verify run record is 'skipped'
        let run_status: String = sqlx::query_scalar("SELECT status FROM scheduled_job_runs WHERE job_id = ?1")
            .bind(&job.id)
            .fetch_one(&pool)
            .await?;
        assert_eq!(run_status, "skipped");

        Ok(())
    }
}





// Metadata: [executor]

//! @docs ARCHITECTURE:Persistence
//!
//! ### AI Assist Note
//! Atomic admission and lifecycle-owned stale execution recovery.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Storage failure, invalid input, or ownership conflict.
//! - **Telemetry Link**: Search `[Persistence]` in server logs.

use crate::error::AppError;
use sqlx::SqlitePool;

/// ### 🔗 Orchestration: Atomic Resource Claiming
/// Atomically claims an agent for a mission by setting its status to 'busy'.
/// Returns `Ok(true)` if the agent was successfully claimed via the atomic locking 
/// mechanism, or `Ok(false)` if the agent is already engaged in another reasoning turn.
pub async fn claim_agent(pool: &SqlitePool, agent_id: &str) -> Result<bool, AppError> {
    let now = chrono::Utc::now();
    let res = sqlx::query(
        "UPDATE agents SET status = 'busy', heartbeat_at = ?1 
         WHERE id = ?2 AND status IN ('idle', 'active')"
    )
    .bind(now)
    .bind(agent_id)
    .execute(pool)
    .await?;

    Ok(res.rows_affected() > 0)
}

/// ### ⚖️ Governance Rationale: The Swarm Reaper
/// Recovers stale orphaned executions under agent lifecycle ownership.
/// 
/// Finished or untracked executions can return to the available pool after
/// their terminal state is persisted. Live tracked tasks retain ownership;
/// provider/tool deadlines and explicit cancellation end those executions.
/// 
/// Prevents permanent "Busy" or "Working" locks in the database (LIF-03) and ensuring 
/// swarm availability across high-concurrency mission cycles.
pub async fn reap_stale_agents(state: &std::sync::Arc<crate::state::AppState>, threshold_secs: i64) -> Result<u64, AppError> {
    if threshold_secs <= 0 { return Err(AppError::BadRequest("Stale threshold must be positive".to_string())); }
    let state = state.clone();
    // Own recovery through commit/publication even if the polling caller stops.
    tokio::spawn(async move { reap_stale_agents_owned(&state, threshold_secs).await })
        .await.map_err(|error| AppError::InternalServerError(format!("Stale recovery task failed: {error}")))?
}

async fn reap_stale_agents_owned(state: &std::sync::Arc<crate::state::AppState>, threshold_secs: i64) -> Result<u64, AppError> {
    let threshold_time = chrono::Utc::now() - chrono::Duration::seconds(threshold_secs);
    let candidates: Vec<String> = sqlx::query_scalar("SELECT id FROM agents WHERE status IN ('busy', 'working', 'thinking') AND ((heartbeat_at IS NOT NULL AND heartbeat_at < ?1) OR (heartbeat_at IS NULL AND (created_at IS NULL OR created_at < ?1))) ORDER BY id")
        .bind(threshold_time).fetch_all(&state.resources.pool).await?;
    let mut reaped = 0;
    let mut first_error = None;
    for agent_id in candidates {
        match recover_stale_agent(state, &agent_id, threshold_time).await {
            Ok(true) => reaped += 1,
            Ok(false) => {},
            Err(error) => {
                tracing::error!(%agent_id, %error, "[Persistence] Stale recovery failed; continuing other agents");
                if first_error.is_none() { first_error = Some(error); }
            }
        }
    }
    if reaped > 0 { tracing::info!("♻️ [Persistence] Reaped {} stale agent runs.", reaped); }
    match first_error { Some(error) => Err(error), None => Ok(reaped) }
}

async fn recover_stale_agent(
    state: &std::sync::Arc<crate::state::AppState>, agent_id: &str, threshold_time: chrono::DateTime<chrono::Utc>,
) -> Result<bool, AppError> {
    let pool = &state.resources.pool;
    let runner = crate::agent::runner::AgentRunner::new(state.clone());
    let _lifecycle = state.registry.lock_agent(agent_id).await;
    let tracked = state.comms.active_runners.get(agent_id).map(|handle| handle.clone());
    // Provider/tool deadlines own live execution expiry. A stale heartbeat
    // cannot authorize a second admission or preempt a valid slow request.
    if tracked.as_ref().is_some_and(|handle| !handle.abort_handle.is_finished()) { return Ok(false); }
    let Some(agent) = state.registry.agents.get(agent_id).map(|agent| agent.clone()) else { return Ok(false); };
    if !matches!(agent.health.status.as_str(), "busy" | "working" | "thinking") { return Ok(false); }
    if agent.health.heartbeat_at.is_some_and(|heartbeat| heartbeat >= threshold_time) { return Ok(false); }
    let still_stale: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM agents WHERE id = ? AND status IN ('busy', 'working', 'thinking') AND ((heartbeat_at IS NOT NULL AND heartbeat_at < ?) OR (heartbeat_at IS NULL AND (created_at IS NULL OR created_at < ?))))")
        .bind(agent_id).bind(threshold_time).bind(threshold_time).fetch_one(pool).await?;
    if !still_stale { return Ok(false); }
    if let Some(mission_id) = agent.state.active_mission.as_ref().and_then(|mission| mission.get("id")).and_then(|id| id.as_str()) {
        let mut ctx = state.comms.mission_contexts.get(mission_id).map(|ctx| ctx.clone())
            .unwrap_or_else(|| agent.resolve_provider_context(state.base_dir.clone()));
        ctx.mission_id = mission_id.to_string();
        runner.persist_terminal_state_locked(&ctx, &ctx.accounting.usage(), true, "Stale execution recovered").await?;
    } else {
        super::update_and_persist_agent(state, agent_id, |agent| {
            agent.health.status = "idle".to_string();
            agent.state.current_task = None;
            agent.state.active_mission = None;
        }).await?;
    }
    if let Some(tracked) = tracked {
        state.comms.active_runners.remove_if(agent_id, |_, handle| handle.task_id == tracked.task_id);
    }
    Ok(true)
}

/// ### 📡 Telemetry: Heartbeat Propagation
/// Updates the heartbeat timestamp in the database for the specified agent.
/// This prevents the `reap_stale_agents` safety valve from prematurely harvesting
/// an active long-running mission.
pub async fn update_agent_heartbeat(pool: &SqlitePool, agent_id: &str) -> Result<(), AppError> {
    sqlx::query("UPDATE agents SET heartbeat_at = ? WHERE id = ?")
        .bind(chrono::Utc::now())
        .bind(agent_id)
        .execute(pool)
        .await?;
    Ok(())
}


// Metadata: [recovery]

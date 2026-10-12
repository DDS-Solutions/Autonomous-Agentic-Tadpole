//! @docs ARCHITECTURE:Networking
//!
//! ### AI Assist Note
//! **Agent Gateway Orchestrator**: Manages the REST surface for autonomous
//! agent registration, configuration, and task dispatching. Features
//! **HATEOAS-Compliant Discovery**: responses include `_links` for
//! self-discovery and related actions. Implements **Async Task
//! Dispatch**: high-level text tasks are acknowledged with `202 ACCEPTED`
//! and spawned into background `AgentRunner` instances. Enforces **W3C
//! Traceparent Propagation** to ensure end-to-end observability from the
//! UI request to the final tool execution (AGNT-01).
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: 404 on valid agent IDs due to registry cache
//!   staling, 400 on suspended agent tasks, or zombie runner tasks
//!   failing silently after process restarts.
//! - **Telemetry Link**: Search for `[agent]` in `tracing` logs for
//!   dispatch/sync events.
//! - **Trace Scope**: `server-rs::routes::agent`

use crate::agent::mission::get_swarm_graph;
use crate::{
    agent::{
        runner::AgentRunner,
        types::{EngineAgent, TaskPayload},
    },
    error::AppError,
    routes::pagination::{PaginatedResponse, PaginationParams},
    state::AppState,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use std::sync::Arc;
use serde::Serialize;

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AgentResponse {
    pub id: String,
    pub name: String,
    pub role: String,
    pub department: String,
    pub status: String,
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    pub model_config: crate::agent::types::ModelConfig,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_2: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_config2: Option<crate::agent::types::ModelConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_3: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_config3: Option<crate::agent::types::ModelConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_model_slot: Option<i32>,
    pub provider: String,
    pub budget_usd: f64,
    pub cost_usd: f64,
    pub is_healthy: bool,
    pub is_bankrupt: bool,
    pub skills: Vec<String>,
    pub workflows: Vec<String>,
    pub mcp_tools: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme_color: Option<String>,
    pub requires_oversight: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_task: Option<String>,
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
    pub version: u32,
}

impl From<&EngineAgent> for AgentResponse {
    fn from(agent: &EngineAgent) -> Self {
        let model_name = if agent.models.model.model_id.trim().is_empty() {
            agent.models.model_id.as_deref().unwrap_or_default().to_string()
        } else {
            agent.models.model.model_id.clone()
        };

        Self {
            id: agent.identity.id.clone(),
            name: agent.identity.name.clone(),
            role: agent.identity.role.clone(),
            department: agent.identity.department.clone(),
            status: agent.health.status.clone(),
            model: model_name,
            model_id: agent.models.model_id.clone(),
            model_config: agent.models.model.clone(),
            model_2: agent.models.model_2.clone(),
            model_config2: agent.models.model_config2.clone(),
            model_3: agent.models.model_3.clone(),
            model_config3: agent.models.model_config3.clone(),
            active_model_slot: agent.models.active_model_slot,
            provider: agent.models.model.provider.to_string(),
            budget_usd: agent.economics.budget_usd,
            cost_usd: agent.economics.cost_usd,
            is_healthy: agent.health.failure_count < 5 && agent.health.status != "suspended",
            is_bankrupt: agent.economics.cost_usd >= agent.economics.budget_usd && agent.economics.budget_usd > 0.0,
            skills: agent.capabilities.skills.clone(),
            workflows: agent.capabilities.workflows.clone(),
            mcp_tools: agent.capabilities.mcp_tools.clone(),
            theme_color: agent.identity.theme_color.clone(),
            requires_oversight: agent.requires_oversight,
            current_task: agent.state.current_task.clone(),
            created_at: agent.created_at,
            version: agent.version,
        }
    }
}

pub(crate) use crate::agent::persistence::update_and_persist_agent;

/// GET /v1/agents
///
/// Retrieves the list of all registered agents in the swarm. Implements
/// HATEOAS-compliant pagination to allow for efficient UI rendering and discovery.
///
/// ### 🛰️ Registry Introspection
/// This handler pulls directly from the engine's memory-mapped `AgentRegistry`. 
/// It maps raw back-end models into a clean, RESTful representation for 
/// dashboard consumption.
///
/// @docs API_REFERENCE:GetAgents
pub async fn get_agents(
    State(state): State<Arc<AppState>>,
    Query(params): Query<PaginationParams>,
) -> Result<impl IntoResponse, AppError> {
    let mut agents: Vec<AgentResponse> = state
        .registry
        .agents
        .iter()
        .map(|kv| AgentResponse::from(kv.value()))
        .collect();

    // Deterministic sorting by agent ID before pagination slicing
    agents.sort_by(|a, b| a.id.cmp(&b.id));

    Ok(Json(PaginatedResponse::from_vec(
        agents,
        &params,
        "/v1/agents",
    )))
}

/// @docs API_REFERENCE:GetAgent
pub async fn get_agent(
    State(state): State<Arc<AppState>>,
    Path(agent_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let agent = state
        .registry
        .agents
        .get(&agent_id)
        .map(|entry| AgentResponse::from(entry.value()))
        .ok_or_else(|| AppError::NotFound(format!("Agent '{}' not found", agent_id)))?;

    Ok(Json(agent))
}

pub(crate) fn spawn_agent_runner(state: Arc<AppState>, agent_id: String, payload: TaskPayload) -> tokio::task::JoinHandle<Result<String, AppError>> {
    let runner = AgentRunner::new(state.clone());
    let run_id = agent_id.clone();
    spawn_agent_execution(state, agent_id, async move { runner.run(run_id, payload).await })
}

/// Caller holds the agent lifecycle lock until this tracking handle is registered.
pub(crate) fn spawn_agent_execution<F>(state: Arc<AppState>, agent_id: String, execution: F) -> tokio::task::JoinHandle<Result<String, AppError>>
where F: std::future::Future<Output = Result<String, AppError>> + Send + 'static {

    // Proactive Abort-on-New Policy: Terminate any existing task for this agent
    if let Some((_, old_handle)) = state.comms.active_runners.remove(&agent_id) {
        tracing::info!("🔄 [Gateway] Aborting existing task for agent {} to prioritize new request.", agent_id);
        old_handle.abort();
        state.emit_event(serde_json::json!({
            "type": "agent:task_preempted",
            "agent_id": agent_id.clone(),
            "message": "Task preempted by incoming dispatch directive"
        }));
    }

    // Spawn Runner with AbortHandle registration barrier (C4 / H4)
    let agent_id_for_spawn = agent_id.clone();
    let state_clone = state.clone();
    let (started_tx, started_rx) = tokio::sync::oneshot::channel::<()>();

    let join_handle = tokio::spawn(async move {
        // Guarantee parent thread inserts RunnerHandle before subtask can evict it
        let _ = started_rx.await;
        let my_id = tokio::task::try_id();
        let mut result = execution.await;
        // Workflow execution may complete without running this owning agent's mission loop.
        if result.is_ok() {
            let _lifecycle = state_clone.registry.lock_agent(&agent_id_for_spawn).await;
            let owns_runner = state_clone.comms.active_runners.get(&agent_id_for_spawn)
                .is_some_and(|handle| Some(handle.task_id) == my_id);
            let needs_release = state_clone.registry.agents.get(&agent_id_for_spawn)
                .is_some_and(|agent| agent.state.active_mission.is_none() && agent.health.status == "busy");
            if owns_runner && needs_release {
                if let Err(error) = update_and_persist_agent(&state_clone, &agent_id_for_spawn, |agent| {
                    agent.health.status = "idle".to_string();
                }).await { result = Err(error); }
            }
        }
        if let Err(ref e) = result {
            tracing::error!("❌ [Runner] Agent {} failed: {}", agent_id_for_spawn, e);
            
            let _lifecycle = state_clone.registry.lock_agent(&agent_id_for_spawn).await;
            // An old runner must not reset a replacement runner or a deleted identity.
            if state_clone.comms.active_runners.get(&agent_id_for_spawn)
                .is_some_and(|handle| Some(handle.task_id) == my_id) {
                if let Err(error) = update_and_persist_agent(&state_clone, &agent_id_for_spawn, |agent| {
                    // Retain mission ownership if terminal persistence failed;
                    // owned cleanup or stale recovery must finalize it first.
                    if agent.state.active_mission.is_none() {
                        if agent.health.status != "suspended" { agent.health.status = "idle".to_string(); }
                        agent.state.current_task = None;
                    }
                }).await {
                    tracing::error!(%error, "[Runner] Failure recovery was not persisted");
                }
            }

            // Async Failure Feedback with structured RFC 9457 support
            let error_data = serde_json::json!({
                "type": e.type_slug(),
                "title": e.type_slug().replace(['-', ':'], " ").to_uppercase(),
                "status": e.status_code().as_u16(),
                "detail": e.to_string(),
                "error_code": e.type_slug().to_uppercase()
            });

            state_clone.emit_event(serde_json::json!({
                "type": "agent:task_failed",
                "agent_id": agent_id_for_spawn.clone(),
                "error": error_data
            }));
        }
        
        // Auto-cleanup handle: Conditionally evict only if the handle still matches this execution task ID
        if let Some(current_id) = my_id {
            state_clone.comms.active_runners.remove_if(&agent_id_for_spawn, |_, handle| {
                handle.task_id == current_id
            });
        }
        result
    });

    state.comms.active_runners.insert(
        agent_id,
        crate::state::hubs::comm::RunnerHandle::new(join_handle.abort_handle(), join_handle.id()),
    );
    let _ = started_tx.send(());
    join_handle
}

/// POST /v1/agents/:id/tasks
///
/// Dispatches a high-level text task to a specific autonomous agent.
/// Automatically handles distributed trace propagation (via W3C `traceparent`)
/// and validates agent existence before dispatch.
///
/// ### 🔦 Distributed Tracing (AGNT-01)
/// If a `traceparent` header is present in the UI request, it is parsed 
/// and injected into the mission payload. This ensures that the engine's 
/// background `AgentRunner` spans are correctly linked to the front-end 
/// session in our Jaeger/OTel traces.
///
/// @docs API_REFERENCE:SendTask
#[tracing::instrument(skip(state, headers, payload), fields(agent_id = %agent_id), name = "agent_gateway::dispatch")]
pub async fn send_task(
    Path(agent_id): Path<String>,
    State(state): State<Arc<AppState>>,
    axum::Extension(_operator): axum::Extension<crate::middleware::auth::AuthenticatedOperator>,
    headers: axum::http::HeaderMap,
    Json(mut payload): Json<TaskPayload>,
) -> Result<impl IntoResponse, AppError> {
    let _lifecycle = state.registry.lock_agent(&agent_id).await;

    // Authoritative edge header takes precedence over body
    if let Some(tp) = headers.get("traceparent").and_then(|v| v.to_str().ok()) {
        payload.traceparent = Some(tp.to_string());
    }

    // Auth & Existence Check & Auto-Wakeup
    match state.registry.agents.get(&agent_id) {
        None => return Err(AppError::NotFound(format!("Agent '{}' not found", agent_id))),
        Some(agent) if agent.health.status == "suspended" => {
            // The typed extension is supplied by token validation. auto_resume
            // is an operator preference, never proof of identity or role.
            if payload.auto_resume.unwrap_or(true) {
                drop(agent);
                tracing::info!("🔋 [AgentDispatch] Suspended agent {} auto-resumed by operator directive", agent_id);
                update_and_persist_agent(&state, &agent_id, |a| {
                    a.health.status = "idle".to_string();
                }).await?;
            } else {
                return Err(AppError::BadRequest(format!("Agent '{}' is currently suspended.", agent_id)));
            }
        },
        Some(agent) if agent.health.status == "offline" => {
            drop(agent);
            tracing::info!("🔋 [AgentDispatch] Agent {} auto-awakened from offline state", agent_id);
            update_and_persist_agent(&state, &agent_id, |a| {
                a.health.status = "idle".to_string();
            }).await?;
        },
        Some(_) => {} // All systems go
    }

    // 🛡️ [M26: Atomic Claim CAS Lock] Route task dispatch through claim_agent
    let mut claimed = crate::agent::persistence::claim_agent(&state.resources.pool, &agent_id).await?;
    if !claimed {
        // Proactive Abort-on-New Policy: terminate any running task, reset status, and re-claim
        if state.comms.active_runners.contains_key(&agent_id) {
            // Save the terminal transition before canceling the old execution.
            update_and_persist_agent(&state, &agent_id, |agent| {
                agent.health.status = "idle".to_string();
                agent.state.active_mission = None;
                agent.state.current_task = None;
            }).await?;
            claimed = crate::agent::persistence::claim_agent(&state.resources.pool, &agent_id).await?;
        } else {
            // Cold/Orphaned Recovery Fallback: if no active runner exists in memory
            // but the agent is locked in database with a stale heartbeat, reclaim it.
            let stale_threshold = chrono::Utc::now() - chrono::Duration::seconds(60);
            let auto_reap = sqlx::query(
                "UPDATE agents SET status = 'idle' WHERE id = ? AND status IN ('busy', 'working') AND (heartbeat_at IS NULL OR heartbeat_at < ?)"
            )
            .bind(&agent_id)
            .bind(stale_threshold)
            .execute(&state.resources.pool)
            .await?;
            {
                if auto_reap.rows_affected() > 0 {
                    tracing::info!("🔄 [Gateway] Auto-cleared stale orphaned agent lock for {} prior to dispatch.", agent_id);
                    claimed = crate::agent::persistence::claim_agent(&state.resources.pool, &agent_id).await?;
                }
            }
        }
    }

    if !claimed {
        return Err(AppError::Conflict(format!(
            "Agent '{}' is currently engaged in another mission and could not be claimed.",
            agent_id
        )));
    }

    if let Some(mut agent) = state.registry.agents.get_mut(&agent_id) {
        agent.health.status = "busy".to_string();
    }

    tracing::info!("📡 [Gateway] Task dispatched to Agent {}", agent_id);
    spawn_agent_runner(state.clone(), agent_id.clone(), payload);


    Ok((
        StatusCode::ACCEPTED,
        Json(serde_json::json!({
            "status": "accepted",
            "agent_id": agent_id
        })),
    ))
}

/// POST /agents
///
/// Registers a new agent in the system and triggers persistence.
#[tracing::instrument(skip(state, new_agent), fields(agent_id = %new_agent.identity.id), name = "agent_registry::create")]
pub async fn create_agent(
    State(state): State<Arc<AppState>>,
    Json(mut new_agent): Json<EngineAgent>,
) -> Result<impl IntoResponse, AppError> {
    let agent_id_owned = new_agent.identity.id.trim().to_string();
    if agent_id_owned.is_empty() {
        return Err(AppError::BadRequest("Agent ID cannot be empty.".to_string()));
    }


    let _lifecycle = state.registry.lock_agent(&agent_id_owned).await;
    // 🛡️ [M17: Collision Guard] Reject duplicate agent ID registrations in registry
    if state.registry.agents.contains_key(&agent_id_owned) {
        return Err(AppError::Conflict(format!(
            "Agent with ID '{}' already exists in the swarm registry.",
            agent_id_owned
        )));
    }

    // 🛡️ Sanitize system-controlled properties against mass assignment (H5)
    new_agent.identity.id = agent_id_owned.clone();
    new_agent.economics.cost_usd = 0.0;
    new_agent.health.failure_count = 0;
    new_agent.health.last_failure_at = None;
    new_agent.health.status = "idle".to_string();
    new_agent.health.heartbeat_at = None;
    new_agent.state.current_task = None;
    new_agent.state.active_mission = None;
    new_agent.economics.tokens_used = 0;
    new_agent.economics.token_usage = Default::default();
    new_agent.version = 1;

    let new_ver = crate::agent::persistence::create_agent_db(&state.resources.pool, &new_agent)
        .await?;
    new_agent.version = new_ver;

    state.registry.agents.insert(agent_id_owned.clone(), new_agent.clone());

    let agent_path = format!("/v1/agents/{}", agent_id_owned);
    state.emit_event(serde_json::json!({
        "type": "agent:create",
        "agent_id": agent_id_owned.clone(),
        "data": new_agent.clone()
    }));
    Ok((
        StatusCode::CREATED,
        [(axum::http::header::LOCATION, agent_path.clone())],
        Json(serde_json::json!({
            "status": "ok",
            "agent_id": agent_id_owned,
            "_links": {
                "self":    { "href": agent_path.clone(), "method": "GET" },
                "tasks":   { "href": format!("{}/tasks", agent_path), "method": "POST" },
                "collection": { "href": "/v1/agents", "method": "GET" }
            }
        })),
    ))
}

/// PUT /agents/:id
///
/// Updates an existing agent's configuration, metadata, or role.
#[tracing::instrument(skip(state, update), fields(agent_id = %agent_id), name = "agent_registry::update")]
pub async fn update_agent(
    Path(agent_id): Path<String>,
    State(state): State<Arc<AppState>>,
    Json(update): Json<crate::agent::types::AgentConfigUpdate>,
) -> Result<impl IntoResponse, AppError> {
    let _lifecycle = state.registry.lock_agent(&agent_id).await;

    update_and_persist_agent(&state, &agent_id, |agent| {
        update.apply_to(agent);
    }).await?;

    Ok(Json(serde_json::json!({ "status": "ok" })))
}


/// POST /agents/:id/pause
#[tracing::instrument(skip(state), fields(agent_id = %agent_id), name = "agent_registry::pause")]
pub async fn pause_agent(
    Path(agent_id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    let _lifecycle = state.registry.lock_agent(&agent_id).await;

    update_and_persist_agent(&state, &agent_id, |agent| {
        agent.pause();
    }).await?;

    // Zombie Task Termination
    if let Some((_, abort_handle)) = state.comms.active_runners.remove(&agent_id) {
        tracing::info!("🛑 [Gateway] Aborting active runner for suspended agent: {}", agent_id);
        abort_handle.abort();
    }

    Ok(Json(serde_json::json!({ "status": "ok" })))
}


/// POST /agents/:id/resume
#[tracing::instrument(skip(state), fields(agent_id = %agent_id), name = "agent_registry::resume")]
pub async fn resume_agent(
    Path(agent_id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    let _lifecycle = state.registry.lock_agent(&agent_id).await;

    update_and_persist_agent(&state, &agent_id, |agent| {
        agent.resume();
    }).await?;

    Ok(Json(serde_json::json!({ "status": "ok" })))
}


/// POST /agents/:id/reset
///
/// @docs API_REFERENCE:ResetAgent
/// Resets an agent's failure count and returns it to idle status.
/// Used to clear "Self-heal cooldowns" after configuration fixes.
#[tracing::instrument(skip(state), fields(agent_id = %agent_id), name = "agent_registry::reset")]
pub async fn reset_agent(
    Path(agent_id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    let _lifecycle = state.registry.lock_agent(&agent_id).await;

    update_and_persist_agent(&state, &agent_id, |agent| {
        agent.reset();
    }).await?;

    // Zombie Task Termination on Reset
    if let Some((_, abort_handle)) = state.comms.active_runners.remove(&agent_id) {
        tracing::info!("🛑 [Gateway] Aborting specific runner for reset agent: {}", agent_id);
        abort_handle.abort();
    }

    Ok(Json(
        serde_json::json!({ "status": "ok", "message": "Failure count reset and tasks terminated." }),
    ))
}


/// POST /agents/:id/mission
///
/// Synchronizes a mission objective to an agent's active mission state.
#[tracing::instrument(skip(state, mission), fields(agent_id = %id), name = "agent_registry::sync_mission")]
pub async fn sync_mission(
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
    Json(mission): Json<serde_json::Value>,
) -> Result<impl IntoResponse, AppError> {
    let _lifecycle = state.registry.lock_agent(&id).await;

    update_and_persist_agent(&state, &id, |agent| {
        agent.set_mission(mission);
    }).await?;

    Ok(Json(serde_json::json!({ "status": "ok" })))
}


/// GET /v1/agents/graph
///
/// Retrieves the complete knowledge graph of agents, missions, and their
/// relationships for real-time visualization in the dashboard.
pub async fn get_swarm_graph_handler(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    let graph = get_swarm_graph(&state)
        .await?;
    Ok(Json(graph))
}

/// Scans the database on startup and resumes runners for agents found in an active ("busy") state.
pub async fn recover_active_agents(state: Arc<AppState>) {
    use futures::StreamExt;

    let agents: Vec<EngineAgent> = state
        .registry
        .agents
        .iter()
        .map(|kv| kv.value().clone())
        .collect();

    let busy_agents: Vec<_> = agents
        .into_iter()
        .filter(|a| a.health.status == "busy" || a.health.status == "working")
        .collect();

    if busy_agents.is_empty() {
        return;
    }

    futures::stream::iter(busy_agents)
        .for_each_concurrent(8, |agent| {
            let state = state.clone();
            async move {
                if let Some(task) = agent.state.current_task.clone() {
                    let is_ephemeral_status = task.starts_with("Executing tool:")
                        || task.starts_with("Consulting intelligence")
                        || task.starts_with("Evaluating mutation")
                        || task.trim().is_empty();

                    if !is_ephemeral_status {
                        let agent_id = agent.identity.id.clone();
                        let cluster_id = agent
                            .state
                            .active_mission
                            .as_ref()
                            .and_then(|m| m.get("id"))
                            .and_then(|id| id.as_str())
                            .map(|s| s.to_string());

                        tracing::info!(
                            "🔄 [State Recovery] Recovering active agent {} for task: {}",
                            agent_id,
                            task
                        );

                        let payload = TaskPayload {
                            message: task,
                            cluster_id,
                            ..Default::default()
                        };

                        let _lifecycle = state.registry.lock_agent(&agent_id).await;
                        if state.registry.agents.contains_key(&agent_id) && !state.comms.active_runners.contains_key(&agent_id) {
                            spawn_agent_runner(state.clone(), agent_id.clone(), payload);
                        }
                    } else {
                        // Reset to idle since task is ephemeral progress status or empty.
                        reset_agent_to_idle(&state, &agent.identity.id).await;
                    }
                } else {
                    reset_agent_to_idle(&state, &agent.identity.id).await;
                }
            }
        })
        .await;
}

/// Helper to reset an agent to idle and persist the updated state.
async fn reset_agent_to_idle(state: &Arc<AppState>, aid: &str) {
    let _lifecycle = state.registry.lock_agent(aid).await;
    let mut clone = state.registry.agents.get(aid).map(|e| e.value().clone());

    if let Some(ref mut a) = clone {
        a.health.status = "idle".to_string();
        match crate::agent::persistence::save_agent_db(&state.resources.pool, a).await {
            Ok(new_ver) => {
                a.version = new_ver;
                if let Some(mut entry) = state.registry.agents.get_mut(aid) {
                    *entry = a.clone();
                }
            }
            Err(e) => {
                tracing::error!(
                    "❌ [State Recovery] Failed to reset agent {} to idle: {}. Memory NOT modified.",
                    aid, e
                );
            }
        }
    }
}

/// DELETE /v1/agents/:id — Transactionally deletes an agent and cascades all metadata.
#[tracing::instrument(skip(state), name = "agent::delete_agent")]
pub async fn delete_agent(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let _lifecycle = state.registry.lock_agent(&id).await;

    // Commit deletion before changing live state. A rejected delete must leave
    // the agent and its runner intact (for example, when ledger history exists).
    crate::agent::persistence::delete_agent_cascade(&state.resources.pool, &id).await?;

    if let Some((_, abort_handle)) = state.comms.active_runners.remove(&id) {
        tracing::info!("🛑 [Gateway] Aborting active runner for deleted agent: {}", id);
        abort_handle.abort();
    }

    // Remove from the in-memory registry after durable deletion succeeds.
    state.registry.agents.remove(&id);
    state.comms.mission_contexts.retain(|_, ctx| ctx.agent_id != id);

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "success",
            "message": format!("Agent {} and all associated data deleted successfully.", id)
        })),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::types::{EngineAgent, TaskPayload};

    #[tokio::test]
    async fn stale_saves_cannot_recreate_deleted_agents_or_overwrite_duplicates() {
        let state = Arc::new(AppState::new_minimal_mock().await);
        let mut agent = EngineAgent::default();
        agent.identity.id = "late-save".to_string();
        agent.version = crate::agent::persistence::create_agent_db(&state.resources.pool, &agent).await.unwrap();
        assert!(matches!(crate::agent::persistence::create_agent_db(&state.resources.pool, &agent).await, Err(AppError::Conflict(_))));
        state.registry.agents.insert(agent.identity.id.clone(), agent.clone());
        delete_agent(State(state.clone()), Path(agent.identity.id.clone())).await.unwrap();
        assert!(matches!(crate::agent::persistence::save_agent_db(&state.resources.pool, &agent).await, Err(AppError::Conflict(_))));
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM agents WHERE id = ?")
            .bind(&agent.identity.id).fetch_one(&state.resources.pool).await.unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn batch_conflict_rolls_back_and_never_adopts_a_newer_version() {
        let state = Arc::new(AppState::new_minimal_mock().await);
        for id in ["batch-a", "batch-b"] {
            let mut agent = EngineAgent::default();
            agent.identity.id = id.to_string();
            agent.version = crate::agent::persistence::create_agent_db(&state.resources.pool, &agent).await.unwrap();
            state.registry.agents.insert(id.to_string(), agent);
        }
        sqlx::query("UPDATE agents SET name = 'durable-newer-name', version = version + 1 WHERE id = 'batch-b'")
            .execute(&state.resources.pool).await.unwrap();
        for _ in 0..2 {
            assert!(matches!(state.save_agents().await, Err(AppError::Conflict(_))));
            for id in ["batch-a", "batch-b"] { assert_eq!(state.registry.agents.get(id).unwrap().version, 1); }
        }
        let row: (String, i64) = sqlx::query_as("SELECT name, version FROM agents WHERE id = 'batch-b'")
            .fetch_one(&state.resources.pool).await.unwrap();
        assert_eq!(row, ("durable-newer-name".to_string(), 2));
        let version: i64 = sqlx::query_scalar("SELECT version FROM agents WHERE id = 'batch-a'")
            .fetch_one(&state.resources.pool).await.unwrap();
        assert_eq!(version, 1);
    }

    #[tokio::test]
    async fn concurrent_creation_has_exactly_one_winner() {
        let state = Arc::new(AppState::new_minimal_mock().await);
        let mut attempts = tokio::task::JoinSet::new();
        for _ in 0..32 {
            let state = state.clone();
            attempts.spawn(async move {
                let mut agent = EngineAgent::default();
                agent.identity.id = "contended-id".to_string();
                create_agent(State(state), Json(agent)).await.map(|_| ())
            });
        }
        let mut successes = 0;
        while let Some(result) = attempts.join_next().await {
            match result.unwrap() { Ok(()) => successes += 1, Err(AppError::Conflict(_)) => {}, Err(error) => panic!("{error}") }
        }
        assert_eq!(successes, 1);
        assert!(state.registry.agents.contains_key("contended-id"));
        assert_eq!(sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM agents WHERE id = 'contended-id'")
            .fetch_one(&state.resources.pool).await.unwrap(), 1);
    }

    #[tokio::test]
    async fn immediate_execution_cleans_registered_handle() {
        let state = Arc::new(AppState::new_minimal_mock().await);
        let lock = state.registry.lock_agent("instant").await;
        let execution = spawn_agent_execution(state.clone(), "instant".to_string(), async { Ok("done".to_string()) });
        drop(lock);
        assert_eq!(execution.await.unwrap().unwrap(), "done");
        assert!(!state.comms.active_runners.contains_key("instant"));
    }

    #[tokio::test]
    async fn test_delete_conflict_preserves_runner_and_success_aborts_it() {
        let state = Arc::new(AppState::new_minimal_mock().await);
        for id in ["delete-runner", "ledger-peer"] {
            let mut agent = EngineAgent::default();
            agent.identity.id = id.to_string();
            agent.version = crate::agent::persistence::create_agent_db(&state.resources.pool, &agent).await.unwrap();
            state.registry.agents.insert(id.to_string(), agent);
        }
        sqlx::query("INSERT INTO a2a_ledger (tx_id, debit_agent_id, credit_agent_id, amount_micros, lock_id) VALUES ('delete-tx', 'delete-runner', 'ledger-peer', 1, 'delete-lock')")
            .execute(&state.resources.pool).await.unwrap();
        let runner = tokio::spawn(std::future::pending::<()>());
        state.comms.active_runners.insert(
            "delete-runner".to_string(),
            crate::state::hubs::comm::RunnerHandle::new(runner.abort_handle(), runner.id()),
        );

        let result = delete_agent(State(state.clone()), Path("delete-runner".to_string())).await;
        assert!(matches!(result, Err(AppError::Conflict(_))));
        assert!(state.registry.agents.contains_key("delete-runner"));
        assert!(state.comms.active_runners.contains_key("delete-runner"));
        assert!(!runner.is_finished());

        sqlx::query("DELETE FROM a2a_ledger WHERE tx_id = 'delete-tx'").execute(&state.resources.pool).await.unwrap();
        assert!(delete_agent(State(state.clone()), Path("delete-runner".to_string())).await.is_ok());
        assert!(!state.registry.agents.contains_key("delete-runner"));
        assert!(!state.comms.active_runners.contains_key("delete-runner"));
        assert!(runner.await.unwrap_err().is_cancelled());
    }

    #[test]
    fn test_agent_response_suspended_is_not_healthy() {
        let mut agent = EngineAgent::default();
        agent.health.status = "suspended".to_string();
        agent.health.failure_count = 0;

        let res = AgentResponse::from(&agent);
        assert!(!res.is_healthy, "Suspended agent must have is_healthy == false even with 0 failures");
        assert_eq!(res.status, "suspended");
    }

    #[test]
    fn test_agent_response_idle_is_healthy() {
        let mut agent = EngineAgent::default();
        agent.health.status = "idle".to_string();
        agent.health.failure_count = 0;

        let res = AgentResponse::from(&agent);
        assert!(res.is_healthy, "Idle agent with 0 failures must be healthy");
        assert_eq!(res.status, "idle");
    }

    #[test]
    fn test_agent_response_failure_count_threshold() {
        let mut agent = EngineAgent::default();
        agent.health.status = "idle".to_string();
        agent.health.failure_count = 5;

        let res = AgentResponse::from(&agent);
        assert!(!res.is_healthy, "Agent with 5 failures must not be healthy");
    }

    #[test]
    fn test_task_payload_auto_resume_deserialization() {
        let json_camel = r#"{"message": "hello", "autoResume": true, "userId": "0"}"#;
        let payload: TaskPayload = serde_json::from_str(json_camel).unwrap();
        assert_eq!(payload.auto_resume, Some(true));
        assert_eq!(payload.user_id.as_deref(), Some("0"));

        let json_snake = r#"{"message": "hello", "auto_resume": true}"#;
        let payload_snake: TaskPayload = serde_json::from_str(json_snake).unwrap();
        assert_eq!(payload_snake.auto_resume, Some(true));
    }
}

// Metadata: [agent]


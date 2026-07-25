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
    pub provider: String,
    pub budget_usd: f64,
    pub cost_usd: f64,
    pub is_healthy: bool,
    pub is_bankrupt: bool,
    pub skills: Vec<String>,
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
            provider: agent.models.model.provider.to_string(),
            budget_usd: agent.economics.budget_usd,
            cost_usd: agent.economics.cost_usd,
            is_healthy: agent.health.failure_count < 5,
            is_bankrupt: agent.economics.cost_usd >= agent.economics.budget_usd && agent.economics.budget_usd > 0.0,
            skills: agent.capabilities.skills.clone(),
            created_at: agent.created_at,
            version: agent.version,
        }
    }
}

/// Centralized persistence and broadcast helper for agent updates.
async fn update_and_persist_agent<F>(
    state: &Arc<AppState>,
    agent_id: &str,
    f: F,
) -> Result<EngineAgent, AppError> 
where
    F: FnOnce(&mut EngineAgent),
{
    let mut agent = state
        .registry
        .agents
        .get_mut(agent_id)
        .ok_or_else(|| AppError::NotFound(format!("Agent {} not found", agent_id)))?;

    f(&mut agent);

    // Sync to DB
    let agent_clone = agent.clone();
    drop(agent); // Release DashMap lock before async I/O

    crate::agent::persistence::save_agent_db(&state.resources.pool, &agent_clone)
        .await?;

    // Broadcast update
    state.emit_event(serde_json::json!({
        "type": "agent:update",
        "agent_id": agent_id,
        "data": agent_clone
    }));

    Ok(agent_clone)
}

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
    let agents: Vec<AgentResponse> = state
        .registry
        .agents
        .iter()
        .map(|kv| AgentResponse::from(kv.value()))
        .collect();
    Ok(Json(PaginatedResponse::from_vec(
        agents,
        &params,
        "/v1/agents",
    )))
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
    headers: axum::http::HeaderMap,
    Json(mut payload): Json<TaskPayload>,
) -> Result<impl IntoResponse, AppError> {
    // Forward traceparent for distributed tracing
    if payload.traceparent.is_none() {
        if let Some(tp) = headers.get("traceparent").and_then(|v| v.to_str().ok()) {
            payload.traceparent = Some(tp.to_string());
        }
    }

    // Auth & Existence Check & Auto-Wakeup
    match state.registry.agents.get(&agent_id) {
        None => return Err(AppError::NotFound(format!("Agent '{}' not found", agent_id))),
        Some(agent) if agent.health.status == "suspended" => {
            return Err(AppError::BadRequest(format!("Agent '{}' is currently suspended.", agent_id)));
        },
        Some(agent) if agent.health.status == "offline" => {
            drop(agent);
            tracing::info!("🔋 [AgentDispatch] Agent {} auto-awakened from offline state", agent_id);
            let _ = update_and_persist_agent(&state, &agent_id, |a| {
                a.health.status = "active".to_string();
            }).await;
        },
        Some(_) => {} // All systems go
    }

    tracing::info!("📡 [Gateway] Task dispatched to Agent {}", agent_id);

    // Proactive Abort-on-New Policy: Terminate any existing task for this agent
    if let Some((_, old_handle)) = state.comms.active_runners.remove(&agent_id) {
        tracing::info!("🔄 [Gateway] Aborting existing task for agent {} to prioritize new request.", agent_id);
        old_handle.abort();
    }

    // Spawn Runner with AbortHandle registration
    let agent_id_for_spawn = agent_id.clone();
    let state_clone = state.clone();
    let join_handle = tokio::spawn(async move {
        let runner = AgentRunner::new(state_clone.clone());
        if let Err(e) = runner.run(agent_id_for_spawn.clone(), payload).await {
            tracing::error!("❌ [Runner] Agent {} failed: {}", agent_id_for_spawn, e);
            
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
        
        // Auto-cleanup handle
        state_clone.comms.active_runners.remove(&agent_id_for_spawn);
    });

    state.comms.active_runners.insert(agent_id.clone(), join_handle.abort_handle());


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
    Json(new_agent): Json<EngineAgent>,
) -> Result<impl IntoResponse, AppError> {
    crate::agent::persistence::save_agent_db(&state.resources.pool, &new_agent)
        .await?;
 
    let agent_id = new_agent.identity.id.clone();
    state
        .registry
        .agents
        .insert(agent_id.clone(), new_agent.clone());

    let agent_path = format!("/v1/agents/{}", agent_id);
    state.emit_event(serde_json::json!({
        "type": "agent:create",
        "agent_id": agent_id.clone(),
        "data": new_agent.clone()
    }));
    Ok((
        StatusCode::CREATED,
        [(axum::http::header::LOCATION, agent_path.clone())],
        Json(serde_json::json!({
            "status": "ok",
            "agent_id": agent_id,
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
        .filter(|a| a.health.status == "busy")
        .collect();

    if busy_agents.is_empty() {
        return;
    }

    futures::stream::iter(busy_agents)
        .for_each_concurrent(8, |agent| {
            let state = state.clone();
            async move {
                if let Some(task) = agent.state.current_task.clone() {
                    if !task.is_empty() {
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

                        let state_clone = state.clone();
                        let agent_id_for_spawn = agent_id.clone();
                        let join_handle = tokio::spawn(async move {
                            let runner = AgentRunner::new(state_clone.clone());
                            if let Err(e) = runner.run(agent_id_for_spawn.clone(), payload).await {
                                tracing::error!("❌ [Runner] Agent {} failed: {}", agent_id_for_spawn, e);
                                
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
                            
                            // Auto-cleanup handle
                            state_clone.comms.active_runners.remove(&agent_id_for_spawn);
                        });

                        state.comms.active_runners.insert(agent_id.clone(), join_handle.abort_handle());

                        let _ = join_handle.await;
                    } else {
                        // Reset to idle since task is empty or missing.
                        let aid = agent.identity.id.clone();
                        let mut clone = state.registry.agents.get(&aid).map(|e| e.value().clone());

                        if let Some(ref mut a) = clone {
                            a.health.status = "idle".to_string();
                            match crate::agent::persistence::save_agent_db(&state.resources.pool, a).await {
                                Ok(()) => {
                                    if let Some(mut entry) = state.registry.agents.get_mut(&aid) {
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
                } else {
                    let aid = agent.identity.id.clone();
                    let mut clone = state.registry.agents.get(&aid).map(|e| e.value().clone());

                    if let Some(ref mut a) = clone {
                        a.health.status = "idle".to_string();
                        match crate::agent::persistence::save_agent_db(&state.resources.pool, a).await {
                            Ok(()) => {
                                if let Some(mut entry) = state.registry.agents.get_mut(&aid) {
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
            }
        })
        .await;
}

/// DELETE /v1/agents/:id — Transactionally deletes an agent and cascades all metadata.
#[tracing::instrument(skip(state), name = "agent::delete_agent")]
pub async fn delete_agent(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    // 1. Remove from SQLite (cascading deletes)
    crate::agent::persistence::delete_agent_cascade(&state.resources.pool, &id).await?;

    // 2. Remove from the in-memory registry
    state.registry.agents.remove(&id);

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "success",
            "message": format!("Agent {} and all associated data deleted successfully.", id)
        })),
    ))
}





// Metadata: [agent]

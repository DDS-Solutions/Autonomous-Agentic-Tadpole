//! @docs ARCHITECTURE:Gateways
//! 
//! ### AI Assist Note
//! **Core technical module for the Tadpole OS hardened engine.**
//! This module implements high-fidelity logic for the Sovereign Reality layer.
//! 
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Runtime logic error, state desynchronization, or resource exhaustion.
//! - **Telemetry Link**: Search `[system]` in tracing logs.

use crate::error::AppError;
use crate::state::AppState;
use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use std::sync::Arc;

/// Exposes the hardware profile of the Tadpole OS engine for sovereign compute telemetry.
#[tracing::instrument(skip(state), name = "system::compute_profile")]
pub async fn get_compute_profile(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    let profile = state.resources.hardware_profiler.get_profile();
    Ok((StatusCode::OK, Json(profile)))
}

#[derive(serde::Serialize)]
pub struct ServiceDebugInfo {
    pub phases: std::collections::HashMap<String, String>,
    pub uptime_seconds: u64,
}

#[derive(serde::Serialize)]
pub struct QueueDebugInfo {
    pub ingestion_queue_depth: usize,
    pub continuity_jobs_pending: usize,
    pub mcp_message_queue: usize,
}

#[tracing::instrument(skip(state), name = "system::debug_services")]
pub async fn debug_services(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    let phases = state
        .resources
        .initialization_registry
        .iter()
        .map(|kv| (kv.key().clone(), format!("{:?}", kv.value())))
        .collect();

    let uptime_seconds = (chrono::Utc::now() - state.start_time).num_seconds().max(0) as u64;

    Ok((
        StatusCode::OK,
        Json(ServiceDebugInfo {
            phases,
            uptime_seconds,
        }),
    ))
}

#[tracing::instrument(skip(state), name = "system::debug_queues")]
pub async fn debug_queues(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    let ingestion_queue_depth = state.comms.active_runners.len();
    
    // Query database for running jobs count
    let continuity_jobs_pending = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM scheduled_job_runs WHERE status = 'running'"
    )
    .fetch_one(&state.resources.pool)
    .await
    .unwrap_or(0) as usize;

    let mcp_message_queue = 0;

    Ok((
        StatusCode::OK,
        Json(QueueDebugInfo {
            ingestion_queue_depth,
            continuity_jobs_pending,
            mcp_message_queue,
        }),
    ))
}




// Metadata: [system]

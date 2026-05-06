//! Sovereign State API — Multiversal Session Tree
//!
//! Provides high-fidelity access to branched session history and 
//! state evolution.

use crate::error::AppError;
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

#[derive(Deserialize)]
pub struct AppendNodeRequest {
    pub parent_id: Option<String>,
    pub role: String,
    pub content: String,
    pub metadata: Option<serde_json::Value>,
}

/// GET /v1/sovereign/missions/:mission_id/nodes/:leaf_id/history
///
/// Reconstructs the linear history for a specific branch tip.
pub async fn get_session_history(
    Path((_mission_id, leaf_id)): Path<(String, String)>,
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    let history = state.traverse_session_history_sovereign(&leaf_id).await?;
    
    Ok(Json(json!({
        "status": "success",
        "history": history,
        "leaf_id": leaf_id
    })))
}

/// GET /v1/sovereign/missions/:mission_id/leaves
///
/// Returns all leaf nodes (tips) for a specific mission.
pub async fn get_mission_leaves(
    Path(mission_id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    let leaves = sqlx::query(
            r#"
            SELECT id, role, content, created_at 
            FROM mission_nodes 
            WHERE mission_id = ?1 AND id NOT IN (SELECT parent_id FROM mission_nodes WHERE parent_id IS NOT NULL)
            ORDER BY created_at DESC
            "#,
        )
        .bind(&mission_id)
        .fetch_all(&state.resources.pool)
        .await
        .map_err(|e| AppError::InternalServerError(format!("Failed to fetch leaves: {}", e)))?;

    let result: Vec<_> = leaves.into_iter().map(|l| {
        use sqlx::Row;
        json!({
            "id": l.get::<String, _>("id"),
            "role": l.get::<String, _>("role"),
            "content": l.get::<String, _>("content"),
            "created_at": l.get::<chrono::DateTime<chrono::Utc>, _>("created_at")
        })
    }).collect();

    Ok(Json(json!({
        "status": "success",
        "leaves": result
    })))
}

/// POST /v1/sovereign/missions/:mission_id/nodes
///
/// Appends a new node to the session tree (creates a fork if parent_id is an ancestor).
pub async fn append_session_node(
    Path(mission_id): Path<String>,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<AppendNodeRequest>,
) -> Result<impl IntoResponse, AppError> {
    let node_id = state.append_session_node_sovereign(
        &mission_id,
        payload.parent_id,
        &payload.role,
        &payload.content,
        payload.metadata,
    ).await?;

    Ok(Json(json!({
        "status": "success",
        "node_id": node_id
    })))
}

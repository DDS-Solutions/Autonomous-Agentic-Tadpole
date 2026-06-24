//! @docs ARCHITECTURE:Governance
//! @docs ARCHITECTURE:Core
//!
//! ### AI Assist Note
//! **Inter-Agent Communication Protocol (IACP)**: Orchestrates peer-to-peer agent
//! interaction, negotiation, and budget delegation. Enables agents to autonomously
//! "hire" other agents for sub-tasks and manage micro-budgets through the GovernanceHub.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Insufficient budget for hiring agent, nonexistent hiring/target agent,
//!   or database connection failures.
//! - **Telemetry Link**: Search `[IACP]` in server traces.

use crate::error::AppError;
use crate::state::AppState;
use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct NegotiateRequest {
    pub hiring_agent_id: String,
    pub target_agent_id: String,
    pub budget: f64,
    pub task_description: String,
}

#[derive(Debug, Deserialize)]
pub struct HireRequest {
    pub hiring_agent_id: String,
    pub target_agent_id: String,
    pub budget: f64,
    pub task_description: String,
}

#[derive(Debug, Serialize)]
pub struct NegotiateResponse {
    pub status: String,
    pub proposal_id: String,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct HireResponse {
    pub status: String,
    pub hire_id: String,
    pub message: String,
}

/// POST /v1/iacp/negotiate
pub async fn negotiate_hire(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<NegotiateRequest>,
) -> Result<impl IntoResponse, AppError> {
    // 1. Verify agents exist
    if !state.registry.agents.contains_key(&payload.hiring_agent_id) {
        return Err(AppError::NotFound(format!(
            "Hiring agent {} not found",
            payload.hiring_agent_id
        )));
    }
    if !state.registry.agents.contains_key(&payload.target_agent_id) {
        return Err(AppError::NotFound(format!(
            "Target agent {} not found",
            payload.target_agent_id
        )));
    }

    // 2. Simple budget validation
    let hiring_agent_budget: f64 =
        sqlx::query_scalar("SELECT budget_usd - cost_usd FROM agents WHERE id = ?")
            .bind(&payload.hiring_agent_id)
            .fetch_one(&state.resources.pool)
            .await
            .map_err(AppError::Sqlx)?;

    if hiring_agent_budget < payload.budget {
        return Err(AppError::BadRequest(format!(
            "Insufficient budget. Agent {} has ${:.2} available, proposed hire costs ${:.2}",
            payload.hiring_agent_id, hiring_agent_budget, payload.budget
        )));
    }

    let proposal_id = Uuid::new_v4().to_string();
    tracing::info!(
        "🤝 [IACP] Agent {} proposed hiring agent {} for ${:.2} (Proposal: {})",
        payload.hiring_agent_id,
        payload.target_agent_id,
        payload.budget,
        proposal_id
    );

    Ok((
        StatusCode::OK,
        Json(NegotiateResponse {
            status: "negotiated".to_string(),
            proposal_id,
            message: "Hiring proposal accepted for peer-review.".to_string(),
        }),
    ))
}

/// POST /v1/iacp/hire
pub async fn execute_hire(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<HireRequest>,
) -> Result<impl IntoResponse, AppError> {
    // 1. Verify agents exist
    if !state.registry.agents.contains_key(&payload.hiring_agent_id) {
        return Err(AppError::NotFound(format!(
            "Hiring agent {} not found",
            payload.hiring_agent_id
        )));
    }
    if !state.registry.agents.contains_key(&payload.target_agent_id) {
        return Err(AppError::NotFound(format!(
            "Target agent {} not found",
            payload.target_agent_id
        )));
    }

    // 2. Perform atomic transaction: validate budget, transfer funds, log contract
    let mut tx = state.resources.pool.begin().await.map_err(AppError::Sqlx)?;

    let hiring_agent_budget: f64 =
        sqlx::query_scalar("SELECT budget_usd - cost_usd FROM agents WHERE id = ?")
            .bind(&payload.hiring_agent_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(AppError::Sqlx)?;

    if hiring_agent_budget < payload.budget {
        return Err(AppError::BadRequest(format!(
            "Insufficient budget. Agent {} has ${:.2} available, hire costs ${:.2}",
            payload.hiring_agent_id, hiring_agent_budget, payload.budget
        )));
    }

    // Transfer budget: subtract from hiring, add to target
    sqlx::query("UPDATE agents SET budget_usd = budget_usd - ? WHERE id = ?")
        .bind(payload.budget)
        .bind(&payload.hiring_agent_id)
        .execute(&mut *tx)
        .await
        .map_err(AppError::Sqlx)?;

    sqlx::query("UPDATE agents SET budget_usd = budget_usd + ? WHERE id = ?")
        .bind(payload.budget)
        .bind(&payload.target_agent_id)
        .execute(&mut *tx)
        .await
        .map_err(AppError::Sqlx)?;

    let hire_id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO agent_hires (id, hiring_agent_id, target_agent_id, budget, task_description, status)
         VALUES (?, ?, ?, ?, ?, 'active')"
    )
    .bind(&hire_id)
    .bind(&payload.hiring_agent_id)
    .bind(&payload.target_agent_id)
    .bind(payload.budget)
    .bind(&payload.task_description)
    .execute(&mut *tx)
    .await
    .map_err(AppError::Sqlx)?;

    tx.commit().await.map_err(AppError::Sqlx)?;

    state.broadcast_sys(
        &format!(
            "🤝 [IACP] Agent {} hired Agent {} for sub-task",
            payload.hiring_agent_id, payload.target_agent_id
        ),
        "success",
        None,
    );

    Ok((
        StatusCode::CREATED,
        Json(HireResponse {
            status: "active".to_string(),
            hire_id,
            message: "Hiring finalized. Budget transferred successfully.".to_string(),
        }),
    ))
}

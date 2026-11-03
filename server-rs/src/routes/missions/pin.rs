//! @docs ARCHITECTURE:Storage
//!
//! ### AI Assist Note
//! **Mission Pin Handler**: Sets or toggles mission retention status atomically in SQLite,
//! prevents state clobbering, verifies authenticated operator trust boundaries, and emits lifecycle events.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Mission not found or SQL execution failure.
//! - **Telemetry Link**: Search `[missions_pin]` in tracing logs.
//! - **Trace Scope**: `server-rs::routes::missions::pin`

use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde_json::json;
use std::sync::Arc;

use crate::error::AppError;
use crate::middleware::auth::AuthenticatedOperator;
use crate::state::AppState;
use super::models::PinMissionRequest;

/// POST /v1/missions/:id/pin
///
/// Sets or toggles the `is_pinned` retention flag for a mission atomically.
/// Accepts an optional `{ "is_pinned": true | false }` JSON payload for idempotent updates.
#[tracing::instrument(skip(state, operator, payload), name = "missions::toggle_mission_pin")]
pub async fn toggle_mission_pin(
    State(state): State<Arc<AppState>>,
    operator: Option<Extension<AuthenticatedOperator>>,
    Path(id): Path<String>,
    payload: Option<Json<PinMissionRequest>>,
) -> Result<impl IntoResponse, AppError> {
    let pool = &state.resources.pool;
    let mission_id = id.trim();
    if mission_id.is_empty() {
        return Err(AppError::BadRequest(
            "Mission ID cannot be empty".to_string(),
        ));
    }

    let desired_pin = payload.and_then(|Json(req)| req.is_pinned);

    // Atomic update without corrupting updated_at
    let updated: Option<(i64,)> = if let Some(pin_target) = desired_pin {
        sqlx::query_as(
            r#"UPDATE mission_history 
            SET is_pinned = ?1 
            WHERE id = ?2 
            RETURNING is_pinned"#,
        )
        .bind(if pin_target { 1_i64 } else { 0_i64 })
        .bind(mission_id)
        .fetch_optional(pool)
        .await?
    } else {
        sqlx::query_as(
            r#"UPDATE mission_history 
            SET is_pinned = CASE WHEN COALESCE(is_pinned, 0) = 0 THEN 1 ELSE 0 END 
            WHERE id = ?1 
            RETURNING is_pinned"#,
        )
        .bind(mission_id)
        .fetch_optional(pool)
        .await?
    };

    let (is_pinned_val,) = match updated {
        Some(v) => v,
        None => {
            return Err(AppError::NotFound(format!(
                "Mission not found: {}",
                mission_id
            )))
        }
    };

    let new_pinned = is_pinned_val == 1;
    // Gate 4 Trust Boundary: Never trust client-supplied X-Actor-Id / X-Principal headers.
    let actor = if operator.is_some() {
        "authenticated_operator"
    } else {
        "local_operator"
    };

    state.emit_event(serde_json::json!({
        "type": "mission_update",
        "mission_id": mission_id,
        "action": "pin_toggled",
        "is_pinned": new_pinned,
        "actor": actor,
        "timestamp": chrono::Utc::now().to_rfc3339()
    }));

    Ok((
        StatusCode::OK,
        Json(json!({
            "success": true,
            "mission_id": mission_id,
            "is_pinned": new_pinned,
            "actor": actor
        })),
    ))
}

// Metadata: [missions_pin]


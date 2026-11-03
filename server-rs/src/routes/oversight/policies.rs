//! @docs ARCHITECTURE:Networking
//!
//! ### AI Assist Note
//! **Policies Subsystem**: Governs declarative permission policies for tools across agents,
//! maintaining atomic deduplication in SQLite and synchronized in-memory caching.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Database permission write failure, cache refresh contention, or duplicate policy key.
//! - **Telemetry Link**: Search for `[policies]` in tracing logs.
//! - **Trace Scope**: `server-rs::routes::oversight::policies`

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::Row;
use std::sync::Arc;

use crate::error::AppError;
use crate::routes::oversight::principal::{validate_identifier, OversightPrincipal};
use crate::state::AppState;

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum PolicyMode {
    Allow,
    Deny,
    Prompt,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct UpdatePolicyPayload {
    pub tool_name: String,
    #[serde(default)]
    pub agent_id: Option<String>,
    pub mode: PolicyMode,
}

/// GET /v1/oversight/security/policies
#[tracing::instrument(skip(state), name = "governance::get_policies")]
pub async fn get_policies(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    let rows = sqlx::query(
        "SELECT tool_name, agent_id, mode FROM permission_policies ORDER BY tool_name ASC, agent_id ASC",
    )
    .fetch_all(&state.resources.pool)
    .await
    .map_err(AppError::Sqlx)?;

    let mut policies = Vec::with_capacity(rows.len());
    for row in rows {
        let name: String = row.try_get("tool_name").map_err(AppError::Sqlx)?;
        let agent_id: Option<String> = row
            .try_get::<Option<String>, _>("agent_id")
            .map_err(AppError::Sqlx)?
            .filter(|id| !id.trim().is_empty());
        let mode: String = row.try_get("mode").map_err(AppError::Sqlx)?;
        policies.push(json!({
            "tool_name": name,
            "agent_id": agent_id,
            "mode": mode
        }));
    }

    Ok(Json(policies))
}

/// PUT /v1/oversight/security/policies
///
/// Atomically upserts a tool permission policy (properly deduplicating global `agent_id IS NULL`
/// policies in SQLite) and records the policy change in the tamper-evident audit ledger.
#[tracing::instrument(
    skip(principal, state, payload),
    fields(tool_name = %payload.tool_name, operator_id = %principal.id),
    name = "governance::update_policy"
)]
pub async fn update_policy(
    principal: OversightPrincipal,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<UpdatePolicyPayload>,
) -> Result<impl IntoResponse, AppError> {
    let actor = principal.authorize_global()?.to_string();
    let validated_tool_name = validate_identifier(&payload.tool_name, "tool_name")?;
    let normalized_agent_id = match payload.agent_id.as_deref() {
        Some(raw) if !raw.trim().is_empty() => Some(validate_identifier(raw, "agent_id")?),
        _ => None,
    };

    let mode_str = match payload.mode {
        PolicyMode::Allow => "allow",
        PolicyMode::Deny => "deny",
        PolicyMode::Prompt => "prompt",
    };

    let mut tx = state.resources.pool.begin().await.map_err(AppError::Sqlx)?;

    if let Some(ref agent_id) = normalized_agent_id {
        sqlx::query(
            "INSERT INTO permission_policies (tool_name, agent_id, mode, updated_at) VALUES (?, ?, ?, CURRENT_TIMESTAMP) \
             ON CONFLICT(tool_name, agent_id) DO UPDATE SET mode = excluded.mode, updated_at = CURRENT_TIMESTAMP",
        )
        .bind(&validated_tool_name)
        .bind(agent_id)
        .bind(mode_str)
        .execute(&mut *tx)
        .await
        .map_err(AppError::Sqlx)?;
    } else {
        // SQLite treats NULL != NULL in UNIQUE(tool_name, agent_id), so we atomically
        // remove existing global row(s) for `tool_name` inside the transaction before inserting.
        sqlx::query(
            "DELETE FROM permission_policies WHERE tool_name = ? AND (agent_id IS NULL OR agent_id = '')",
        )
        .bind(&validated_tool_name)
        .execute(&mut *tx)
        .await
        .map_err(AppError::Sqlx)?;

        sqlx::query(
            "INSERT INTO permission_policies (tool_name, agent_id, mode, updated_at) VALUES (?, NULL, ?, CURRENT_TIMESTAMP)",
        )
        .bind(&validated_tool_name)
        .bind(mode_str)
        .execute(&mut *tx)
        .await
        .map_err(AppError::Sqlx)?;
    }

    tx.commit().await.map_err(AppError::Sqlx)?;

    state
        .security
        .permission_policy
        .refresh_cache()
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to refresh permission policy cache");
            AppError::InternalServerError("Failed to refresh permission cache".to_string())
        })?;

    let audit_params = json!({
        "tool_name": validated_tool_name,
        "agent_id": normalized_agent_id,
        "mode": mode_str,
        "updated_by": actor
    });
    state
        .security
        .audit_trail
        .record(
            "oversight",
            None,
            Some(&actor),
            "permission_policy_updated",
            &audit_params.to_string(),
        )
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to record policy change in audit trail");
            AppError::InternalServerError("Failed to record policy audit entry".to_string())
        })?;

    state.broadcast_sys(
        &format!(
            "🛡️ Security Policy Updated by {}: {} -> {}",
            actor, validated_tool_name, mode_str
        ),
        "info",
        None,
    );

    Ok((StatusCode::OK, Json(json!({ "status": "ok" }))))
}

// Metadata: [policies]



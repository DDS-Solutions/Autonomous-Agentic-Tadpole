//! @docs ARCHITECTURE:Networking
//!
//! ### AI Assist Note
//! **Governance Settings Subsystem**: Manages cluster-wide governance parameters,
//! privacy mode toggling, auto-approval thresholds, and atomic SQLite/memory state synchronization.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Governance setting ceiling violation, persistence failure, or cache synchronization mismatch.
//! - **Telemetry Link**: Search for `[settings]` in tracing logs.
//! - **Trace Scope**: `server-rs::routes::oversight::settings`

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use sqlx::Row;
use std::sync::{atomic::Ordering, Arc};

use crate::error::AppError;
use crate::routes::oversight::principal::{validate_oversight_settings, OversightPrincipal};
use crate::state::AppState;

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct OversightSettingsPayload {
    #[serde(default)]
    pub auto_approve_safe_skills: Option<bool>,
    pub privacy_mode: Option<bool>,
    pub max_agents: Option<u32>,
    pub max_clusters: Option<u32>,
    pub max_swarm_depth: Option<u32>,
    pub max_task_length: Option<usize>,
    pub default_budget_usd: Option<f64>,
}

pub async fn persist_governance_settings(
    state: &AppState,
    payload: &OversightSettingsPayload,
    updated_by: &str,
) -> Result<(bool, bool, u32, u32, u32, usize, f64), AppError> {
    let pool = &state.resources.pool;
    let initial_auto_approve = state.governance.auto_approve_safe_skills.load(Ordering::SeqCst);
    let initial_privacy = state.governance.privacy_mode.load(Ordering::SeqCst);
    let initial_max_agents = state.governance.max_agents.load(Ordering::SeqCst);
    let initial_max_clusters = state.governance.max_clusters.load(Ordering::SeqCst);
    let initial_max_depth = state.governance.max_swarm_depth.load(Ordering::SeqCst);
    let initial_max_task_len = state.governance.max_task_length.load(Ordering::SeqCst);
    let initial_budget = *state.governance.default_budget_usd.read();

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS governance_settings (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            auto_approve_safe_skills INTEGER NOT NULL,
            privacy_mode INTEGER NOT NULL,
            max_agents INTEGER NOT NULL,
            max_clusters INTEGER NOT NULL,
            max_swarm_depth INTEGER NOT NULL,
            max_task_length INTEGER NOT NULL,
            default_budget_usd REAL NOT NULL,
            updated_by TEXT NOT NULL,
            updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
        )",
    )
    .execute(pool)
    .await
    .map_err(AppError::Sqlx)?;

    sqlx::query(
        "INSERT OR IGNORE INTO governance_settings (
            id, auto_approve_safe_skills, privacy_mode, max_agents, max_clusters,
            max_swarm_depth, max_task_length, default_budget_usd, updated_by, updated_at
        ) VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, 'system', CURRENT_TIMESTAMP)",
    )
    .bind(i64::from(initial_auto_approve))
    .bind(i64::from(initial_privacy))
    .bind(i64::from(initial_max_agents))
    .bind(i64::from(initial_max_clusters))
    .bind(i64::from(initial_max_depth))
    .bind(initial_max_task_len as i64)
    .bind(initial_budget)
    .execute(pool)
    .await
    .map_err(AppError::Sqlx)?;

    let row = sqlx::query(
        "UPDATE governance_settings SET
            auto_approve_safe_skills = COALESCE(?1, auto_approve_safe_skills),
            privacy_mode = COALESCE(?2, privacy_mode),
            max_agents = COALESCE(?3, max_agents),
            max_clusters = COALESCE(?4, max_clusters),
            max_swarm_depth = COALESCE(?5, max_swarm_depth),
            max_task_length = COALESCE(?6, max_task_length),
            default_budget_usd = COALESCE(?7, default_budget_usd),
            updated_by = ?8,
            updated_at = CURRENT_TIMESTAMP
        WHERE id = 1
        RETURNING auto_approve_safe_skills, privacy_mode, max_agents, max_clusters, max_swarm_depth, max_task_length, default_budget_usd",
    )
    .bind(payload.auto_approve_safe_skills.map(i64::from))
    .bind(payload.privacy_mode.map(i64::from))
    .bind(payload.max_agents.map(i64::from))
    .bind(payload.max_clusters.map(i64::from))
    .bind(payload.max_swarm_depth.map(i64::from))
    .bind(payload.max_task_length.map(|l| l as i64))
    .bind(payload.default_budget_usd)
    .bind(updated_by)
    .fetch_one(pool)
    .await
    .map_err(AppError::Sqlx)?;

    let auto_approve: bool = row.try_get::<i64, _>("auto_approve_safe_skills").map_err(AppError::Sqlx)? != 0;
    let privacy_mode: bool = row.try_get::<i64, _>("privacy_mode").map_err(AppError::Sqlx)? != 0;
    let max_agents: u32 = row.try_get::<i64, _>("max_agents").map_err(AppError::Sqlx)? as u32;
    let max_clusters: u32 = row.try_get::<i64, _>("max_clusters").map_err(AppError::Sqlx)? as u32;
    let max_swarm_depth: u32 = row.try_get::<i64, _>("max_swarm_depth").map_err(AppError::Sqlx)? as u32;
    let max_task_length: usize = row.try_get::<i64, _>("max_task_length").map_err(AppError::Sqlx)? as usize;
    let default_budget_usd: f64 = row.try_get("default_budget_usd").map_err(AppError::Sqlx)?;

    Ok((
        auto_approve,
        privacy_mode,
        max_agents,
        max_clusters,
        max_swarm_depth,
        max_task_length,
        default_budget_usd,
    ))
}

/// GET /v1/oversight/settings
///
/// Returns current global governance constraints.
///
/// @docs API_REFERENCE:GetGovernanceSettings
#[tracing::instrument(skip(state), name = "governance::get_settings")]
pub async fn get_settings(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    let auto_approve = state.governance.auto_approve_safe_skills.load(Ordering::SeqCst);
    let privacy = state.governance.privacy_mode.load(Ordering::SeqCst);
    let max_agents = state.governance.max_agents.load(Ordering::SeqCst);
    let max_clusters = state.governance.max_clusters.load(Ordering::SeqCst);
    let max_depth = state.governance.max_swarm_depth.load(Ordering::SeqCst);
    let observed_max_depth = state.governance.observed_max_depth.load(Ordering::SeqCst);
    let max_task_len = state.governance.max_task_length.load(Ordering::SeqCst);
    let default_budget = *state.governance.default_budget_usd.read();

    Ok(Json(json!({
        "status": "ok",
        "auto_approve_safe_skills": auto_approve,
        "privacy_mode": privacy,
        "max_agents": max_agents,
        "max_clusters": max_clusters,
        "max_swarm_depth": max_depth,
        "observed_max_depth": observed_max_depth,
        "max_task_length": max_task_len,
        "default_budget_usd": default_budget,
    })))
}

/// PUT /v1/oversight/settings
///
/// Updates global governance constraints, including swarm depth,
/// auto-approval policies, and budget limitations. Persists to SQLite and
/// records the mutation in the tamper-evident audit ledger.
///
/// @docs OPERATIONS_MANUAL:GovernanceSettings
#[tracing::instrument(
    skip(principal, state, payload),
    fields(operator_id = %principal.id),
    name = "governance::update_settings"
)]
pub async fn update_settings(
    principal: OversightPrincipal,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<OversightSettingsPayload>,
) -> Result<impl IntoResponse, AppError> {
    let actor = principal.authorize_global()?.to_string();
    validate_oversight_settings(&payload)?;

    // 1. Authoritative DB persist FIRST with atomic COALESCE update & RETURNING
    let (
        current_auto_approve,
        current_privacy,
        current_max_agents,
        current_max_clusters,
        current_max_depth,
        current_max_task_len,
        current_budget,
    ) = persist_governance_settings(&state, &payload, &actor).await?;

    // 2. Memory atomics updated directly from the merged, authoritative DB row
    state
        .governance
        .auto_approve_safe_skills
        .store(current_auto_approve, Ordering::SeqCst);
    state
        .governance
        .privacy_mode
        .store(current_privacy, Ordering::SeqCst);
    state
        .governance
        .max_agents
        .store(current_max_agents, Ordering::SeqCst);
    state
        .governance
        .max_clusters
        .store(current_max_clusters, Ordering::SeqCst);
    state
        .governance
        .max_swarm_depth
        .store(current_max_depth, Ordering::SeqCst);
    state
        .governance
        .max_task_length
        .store(current_max_task_len, Ordering::SeqCst);
    *state.governance.default_budget_usd.write() = current_budget;

    if let Some(val) = payload.privacy_mode {
        if val {
            tracing::info!(
                operator = %actor,
                "Privacy Shield ENABLED: Hard-blocking external API requests (OpenAI/Anthropic/Google)."
            );
        }

        state.broadcast_sys(
            &format!(
                "🛡️ Privacy Shield: Mode set to {}. {}",
                if val {
                    "ON (Local-First)"
                } else {
                    "OFF (Hybrid)"
                },
                if val {
                    "External APIs are hard-blocked."
                } else {
                    ""
                }
            ),
            if val { "success" } else { "info" },
            None,
        );
    }

    let current_auto_approve = state
        .governance
        .auto_approve_safe_skills
        .load(Ordering::SeqCst);
    let current_privacy = state.governance.privacy_mode.load(Ordering::SeqCst);
    let current_max_agents = state.governance.max_agents.load(Ordering::SeqCst);
    let current_max_clusters = state.governance.max_clusters.load(Ordering::SeqCst);
    let current_max_depth = state.governance.max_swarm_depth.load(Ordering::SeqCst);
    let current_observed_depth = state.governance.observed_max_depth.load(Ordering::SeqCst);
    let current_max_task_len = state.governance.max_task_length.load(Ordering::SeqCst);
    let current_budget = *state.governance.default_budget_usd.read();

    let snapshot = json!({
        "auto_approve_safe_skills": current_auto_approve,
        "privacy_mode": current_privacy,
        "max_agents": current_max_agents,
        "max_clusters": current_max_clusters,
        "max_swarm_depth": current_max_depth,
        "max_task_length": current_max_task_len,
        "default_budget_usd": current_budget,
        "updated_by": actor
    });

    state
        .security
        .audit_trail
        .record(
            "oversight",
            None,
            Some(&actor),
            "governance_settings_updated",
            &snapshot.to_string(),
        )
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to record governance settings update in audit trail");
            AppError::InternalServerError("Failed to record governance audit entry".to_string())
        })?;

    state.emit_event(json!({
        "type": "oversight:settings_updated",
        "data": snapshot
    }));

    tracing::info!(
        operator = %actor,
        auto_approve_safe_skills = current_auto_approve,
        privacy_mode = current_privacy,
        max_agents = current_max_agents,
        max_clusters = current_max_clusters,
        max_swarm_depth = current_max_depth,
        "Governance settings updated"
    );

    Ok((
        StatusCode::OK,
        Json(json!({
            "status": "ok",
            "auto_approve_safe_skills": current_auto_approve,
            "max_agents": current_max_agents,
            "max_clusters": current_max_clusters,
            "max_swarm_depth": current_max_depth,
            "observed_max_depth": current_observed_depth,
            "max_task_length": current_max_task_len,
            "default_budget_usd": current_budget,
            "privacy_mode": current_privacy
        })),
    ))
}

// Metadata: [settings]


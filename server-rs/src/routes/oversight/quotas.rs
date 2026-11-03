//! @docs ARCHITECTURE:Networking
//!
//! ### AI Assist Note
//! **Quotas Subsystem**: Enforces and queries agent and mission budget limits, reset schedules,
//! and Merkle security stats across the swarm.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Negative budget bounds, database quota update failure, or reset period parse error.
//! - **Telemetry Link**: Search for `[quotas]` in tracing logs.
//! - **Trace Scope**: `server-rs::routes::oversight::quotas`

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

use crate::error::AppError;
use crate::routes::oversight::principal::{
    validate_identifier, validate_quota_budget, OversightPrincipal,
};
use crate::security::metering::{Quota, ResetPeriod};
use crate::state::AppState;

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct UpdateQuotaPayload {
    pub budget_usd: f64,
    pub reset_period: Option<ResetPeriod>,
}

/// PUT /v1/oversight/security/quotas/:entity_id
///
/// Updates the budget quota and reset period for a specific agent.
///
/// @docs API_REFERENCE:UpdateAgentQuota
#[tracing::instrument(
    skip(principal, state, payload),
    fields(entity_id = %entity_id, operator_id = %principal.id),
    name = "governance::update_agent_quota"
)]
pub async fn update_agent_quota(
    principal: OversightPrincipal,
    Path(entity_id): Path<String>,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<UpdateQuotaPayload>,
) -> Result<impl IntoResponse, AppError> {
    let actor = principal.authorize_global()?.to_string();
    let validated_entity_id = validate_identifier(&entity_id, "entity_id")?;
    validate_quota_budget(payload.budget_usd)?;

    state
        .security
        .budget_guard
        .update_quota(&validated_entity_id, payload.budget_usd, payload.reset_period)
        .await
        .map_err(|e| {
            tracing::error!(
                entity_id = %validated_entity_id,
                operator = %actor,
                error = %e,
                "Failed to update agent budget quota"
            );
            AppError::InternalServerError("Failed to update agent budget quota".to_string())
        })?;

    let audit_params = json!({
        "entity_id": validated_entity_id,
        "budget_usd": payload.budget_usd,
        "reset_period": payload.reset_period,
        "updated_by": actor,
    });
    state
        .security
        .audit_trail
        .record(
            "oversight",
            None,
            Some(&actor),
            "agent_quota_updated",
            &audit_params.to_string(),
        )
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to record agent quota audit entry");
            AppError::InternalServerError("Failed to record quota audit entry".to_string())
        })?;

    tracing::info!(
        entity_id = %validated_entity_id,
        budget_usd = payload.budget_usd,
        operator = %actor,
        "[Budget] Agent quota updated"
    );
    Ok((StatusCode::OK, Json(json!({ "status": "ok" }))))
}

/// GET /v1/oversight/security/quotas
///
/// Returns global budget telemetry, including total spent, remaining,
/// and system defense metrics. Never conflates audit verification errors with tampering.
///
/// @docs API_REFERENCE:GetQuotas
#[tracing::instrument(skip(state), name = "security_governance::get_quotas")]
pub async fn get_security_quotas(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    let mut total_budget = 0.0;
    let mut total_spent = 0.0;

    for entry in state.registry.agents.iter() {
        total_budget += entry.value().economics.budget_usd;
        total_spent += entry.value().economics.cost_usd;
    }

    let agent_quotas: Vec<Quota> = state
        .security
        .budget_guard
        .get_all_quotas()
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to retrieve agent quotas");
            AppError::InternalServerError("Failed to retrieve agent quotas".to_string())
        })?;

    let system_defense = state.security.system_monitor.get_system_defense_stats();

    let (merkle_integrity, merkle_status) =
        match state.security.audit_trail.verify_last_n(10, None).await {
            Ok((v, t)) if t > 0 => {
                let score = (v as f64 / t as f64).min(1.0);
                let status = if v == t { "VERIFIED" } else { "TAMPERED" };
                (Some(score), status)
            }
            Ok(_) => (None, "EMPTY"),
            Err(e) => {
                tracing::warn!(error = %e, "Merkle integrity verification unavailable");
                (None, "UNAVAILABLE")
            }
        };

    tracing::debug!(
        agents = state.registry.agents.len(),
        total_spent = total_spent,
        total_budget = total_budget,
        remaining = total_budget - total_spent,
        "[Oversight] Quotas evaluated"
    );

    Ok(Json(json!({
        "total_budget": total_budget,
        "total_spent": total_spent,
        "remaining": total_budget - total_spent,
        "efficiency": if total_budget > 0.0 { (total_spent / total_budget) * 100.0 } else { 0.0 },
        "agent_quotas": agent_quotas,
        "system_defense": {
            "memory_pressure": system_defense.memory_pressure,
            "cpu_load": system_defense.cpu_load,
            "sandbox_status": system_defense.sandbox_status,
            "sandbox_type": system_defense.sandbox_type,
            "merkle_integrity": merkle_integrity,
            "merkle_status": merkle_status
        }
    })))
}

/// GET /v1/oversight/security/missions/quotas
///
/// Reads mission quotas from the authoritative `BudgetGuard` store (including
/// debounced in-memory usage) with explicit column selection and safe `try_get` decoding.
#[tracing::instrument(skip(state), name = "security_governance::get_mission_quotas")]
pub async fn get_mission_quotas(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    let quotas = state
        .security
        .budget_guard
        .get_all_mission_quotas()
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to retrieve mission quotas");
            AppError::InternalServerError("Failed to retrieve mission quotas".to_string())
        })?;

    Ok(Json(quotas))
}

/// PUT /v1/oversight/security/missions/:id/quota
#[tracing::instrument(
    skip(principal, state, payload),
    fields(cluster_id = %cluster_id, operator_id = %principal.id),
    name = "security_governance::update_mission_quota"
)]
pub async fn update_mission_quota(
    principal: OversightPrincipal,
    Path(cluster_id): Path<String>,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<UpdateQuotaPayload>,
) -> Result<impl IntoResponse, AppError> {
    let validated_cluster_id = validate_identifier(&cluster_id, "cluster_id")?;
    let actor = principal
        .authorize_mission(Some(&validated_cluster_id))?
        .to_string();
    validate_quota_budget(payload.budget_usd)?;

    state
        .security
        .budget_guard
        .update_mission_quota(&validated_cluster_id, payload.budget_usd, payload.reset_period)
        .await
        .map_err(|e| {
            tracing::error!(
                cluster_id = %validated_cluster_id,
                operator = %actor,
                error = %e,
                "Failed to update mission quota"
            );
            AppError::InternalServerError("Failed to update mission quota".to_string())
        })?;

    let audit_params = json!({
        "cluster_id": validated_cluster_id,
        "budget_usd": payload.budget_usd,
        "reset_period": payload.reset_period,
        "updated_by": actor,
    });
    state
        .security
        .audit_trail
        .record(
            "oversight",
            Some(&validated_cluster_id),
            Some(&actor),
            "mission_quota_updated",
            &audit_params.to_string(),
        )
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to record mission quota audit entry");
            AppError::InternalServerError("Failed to record mission quota audit entry".to_string())
        })?;

    tracing::info!(
        cluster_id = %validated_cluster_id,
        budget_usd = payload.budget_usd,
        operator = %actor,
        "[Budget] Mission quota updated"
    );
    Ok((StatusCode::OK, Json(json!({ "status": "ok" }))))
}

// Metadata: [quotas]



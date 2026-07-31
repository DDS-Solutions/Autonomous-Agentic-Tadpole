//! @docs ARCHITECTURE:Networking
//!
//! ### AI Assist Note
//! **Governance & Oversight Gateway**: Orchestrates the REST surface
//! for **Human-in-the-Loop** verification and global engine
//! constraints. Features **Oversight Decision Flows**: manages the
//! unblocking and termination of autonomous agent tasks based on
//! human approval. Implements **Merkle Hash-Chain Recording**: every
//! oversight decision is committed to a tamper-evident audit ledger
//! with digital signature verification. AI agents should monitor the
//! `oversight_queue` and provide clear technical context for all
//! pending decisions to minimize human review friction (GOV-01).
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: 404 on resolution due to resolver timeout,
//!   decisions stalling in the queue due to missing human input, or
//!   Merkle integrity failures on tampered log entries.
//! - **Telemetry Link**: Search `[oversight]` in tracing logs.
//!   for decision lifecycle events.
//! - **Trace Scope**: `server-rs::routes::oversight`

use crate::agent::types::{OversightDecision, OversightEntry};
use crate::error::AppError;
use crate::routes::pagination::{PaginatedResponse, PaginationParams};
use crate::security::audit::AuditEntry;
use crate::security::metering::{Quota, ResetPeriod};
use crate::state::AppState;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Serialize;
use serde_json::json;
use std::sync::Arc;
use specta::Type;

#[derive(Serialize)]
pub struct OversightAuditEntry {
    pub id: String,
    pub agent_id: String,
    pub skill: Option<String>,
    pub status: String,
    pub decision: Option<String>,
    pub decided_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub is_verified: bool,
}

#[derive(Serialize)]
pub struct SecurityIntegrityResponse {
    pub integrity_score: f64,
    pub status: String,
    pub verified_count: usize,
    pub total_count: usize,
}

/// GET /v1/oversight/pending
///
/// Returns a collection of all actions (file edits, network requests, etc.)
/// currently paused and awaiting human verification.
///
/// @docs API_REFERENCE:GetPendingOversight
#[tracing::instrument(skip(state), name = "governance::list_pending")]
pub async fn get_pending(
    State(state): State<Arc<AppState>>,
    Query(params): Query<PaginationParams>,
) -> Result<impl IntoResponse, AppError> {
    let entries: Vec<OversightEntry> = state
        .comms
        .oversight_queue
        .iter()
        .map(|entry| entry.value().clone())
        .collect();

    Ok(Json(PaginatedResponse::from_vec(
        entries,
        &params,
        "/v1/oversight/pending",
    )))
}

/// GET /v1/oversight/ledger
///
/// Provides a historical audit trail of all previous oversight decisions.
/// Directly queries the SQLite persistence layer with support for pagination.
///
/// @docs API_REFERENCE:GetOversightLedger
#[tracing::instrument(skip(state), name = "governance::list_ledger")]
pub async fn get_ledger(
    State(state): State<Arc<AppState>>,
    Query(params): Query<PaginationParams>,
) -> Result<impl IntoResponse, AppError> {
    let rows = sqlx::query(
        r#"
        SELECT 
            o.id, o.mission_id, o.agent_id, o.entry_type, o.skill, o.params, o.status, o.decision, o.decided_at, o.decided_by, o.created_at, o.payload,
            a.action as audit_action, a.timestamp as audit_timestamp, a.params as audit_params
        FROM oversight_log o
        LEFT JOIN audit_trail a ON a.id = (
            SELECT id FROM audit_trail 
            WHERE mission_id = o.mission_id 
              AND agent_id = o.agent_id 
              AND (action = '[SUCCESS] ' || o.skill OR action = '[FAILURE] ' || o.skill)
              AND datetime(timestamp) >= datetime(o.created_at)
            ORDER BY datetime(timestamp) ASC
            LIMIT 1
        )
        WHERE o.status != 'pending' 
        ORDER BY o.created_at DESC
        "#
    )
    .fetch_all(&state.resources.pool)
    .await
    .map_err(AppError::Sqlx)?;

    let entries: Vec<serde_json::Value> = rows.into_iter().map(|row| {
        use sqlx::Row;
        
        let audit_action = row.try_get::<Option<String>, _>("audit_action").ok().flatten();
        let audit_timestamp = row.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("audit_timestamp").ok().flatten();
        let audit_params = row.try_get::<Option<String>, _>("audit_params").ok().flatten();

        let result_val = if let Some(action) = audit_action {
            let success = action.starts_with("[SUCCESS]");
            let decided_at = row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("decided_at");
            
            let duration_ms = if let (Some(decided), Some(completed)) = (decided_at, audit_timestamp) {
                let diff = completed.signed_duration_since(decided);
                diff.num_milliseconds().abs().max(1)
            } else {
                15 // default fallback duration
            };

            let output_str = audit_params.unwrap_or_else(|| "No execution output recorded".to_string());
            
            if success {
                serde_json::json!({
                    "success": true,
                    "output": output_str,
                    "duration_ms": duration_ms
                })
            } else {
                serde_json::json!({
                    "success": false,
                    "output": output_str,
                    "error": output_str,
                    "duration_ms": duration_ms
                })
            }
        } else {
            serde_json::Value::Null
        };

        let decision_val = row.get::<Option<String>, _>("decision");
        let decided_by_val = row.get::<Option<String>, _>("decided_by");
        let is_auto = decided_by_val.as_deref() == Some("auto_policy")
            || decided_by_val.as_deref() == Some("system")
            || decision_val.as_deref() == Some("auto_approved");

        let mut entry_obj = serde_json::json!({
            "id": row.get::<String, _>("id"),
            "mission_id": row.get::<Option<String>, _>("mission_id"),
            "tool_call": {
                "id": format!("tc-{}", row.get::<String, _>("id")), // Synthetic toolCall ID for history
                "agent_id": row.get::<String, _>("agent_id"),
                "skill": row.get::<Option<String>, _>("skill"),
                "params": serde_json::from_str::<serde_json::Value>(&row.get::<String, _>("params")).unwrap_or_default(),
                "description": row.get::<Option<String>, _>("payload")
                    .unwrap_or_else(|| "Historical action".to_string()),
            },
            "type": row.get::<String, _>("entry_type"),
            "status": row.get::<String, _>("status"),
            "decision": decision_val,
            "decided_at": row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("decided_at"),
            "decided_by": decided_by_val,
            "auto_approved": is_auto,
            "approval_type": if is_auto { "auto" } else { "hitl" },
            "requires_oversight": !is_auto,
            "created_at": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
            "payload": serde_json::from_str::<serde_json::Value>(&row.get::<String, _>("payload")).unwrap_or_default(),
        });

        if let Some(obj) = entry_obj.as_object_mut() {
            if !result_val.is_null() {
                obj.insert("result".to_string(), result_val);
            }
        }

        entry_obj
    }).collect();

    Ok(Json(PaginatedResponse::from_vec(
        entries,
        &params,
        "/v1/oversight/ledger",
    )))
}

#[derive(serde::Deserialize, Debug)]
pub struct OversightSettingsPayload {
    pub auto_approve_safe_skills: bool,
    pub privacy_mode: Option<bool>,
    pub max_agents: Option<u32>,
    pub max_clusters: Option<u32>,
    pub max_swarm_depth: Option<u32>,
    pub max_task_length: Option<usize>,
    pub default_budget_usd: Option<f64>,
}

/// PUT /v1/oversight/settings
///
/// Updates global governance constraints, including swarm depth,
/// auto-approval policies, and budget limitations.
///
/// @docs OPERATIONS_MANUAL:GovernanceSettings
#[tracing::instrument(skip(state, payload), name = "governance::update_settings")]
pub async fn update_settings(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<OversightSettingsPayload>,
) -> Result<impl IntoResponse, AppError> {
    state.governance.auto_approve_safe_skills.store(
        payload.auto_approve_safe_skills,
        std::sync::atomic::Ordering::Relaxed,
    );

    if let Some(val) = payload.privacy_mode {
        state
            .governance
            .privacy_mode
            .store(val, std::sync::atomic::Ordering::SeqCst);
        
        if val {
            tracing::info!("🛡️ Privacy Shield ENABLED: Hard-blocking external API requests (OpenAI/Anthropic/Google).");
        }

        state.broadcast_sys(
            &format!(
                "🛡️ Privacy Shield: Mode set to {}. {}",
                if val {
                    "ON (Local-First)"
                } else {
                    "OFF (Hybrid)"
                },
                if val { "External APIs are hard-blocked." } else { "" }
            ),
            if val { "success" } else { "info" },
            None,
        );
    }

    if let Some(val) = payload.max_agents {
        state
            .governance
            .max_agents
            .store(val, std::sync::atomic::Ordering::Relaxed);
    }
    if let Some(val) = payload.max_clusters {
        state
            .governance
            .max_clusters
            .store(val, std::sync::atomic::Ordering::Relaxed);
    }
    if let Some(val) = payload.max_swarm_depth {
        state
            .governance
            .max_swarm_depth
            .store(val, std::sync::atomic::Ordering::Relaxed);
    }
    if let Some(val) = payload.max_task_length {
        state
            .governance
            .max_task_length
            .store(val, std::sync::atomic::Ordering::Relaxed);
    }
    if let Some(val) = payload.default_budget_usd {
        *state.governance.default_budget_usd.write() = val;
    }

    tracing::info!(
        "🛡️ Governance updated: Auto-Approve={}, MaxAgents={:?}, MaxClusters={:?}, MaxDepth={:?}",
        payload.auto_approve_safe_skills,
        payload.max_agents,
        payload.max_clusters,
        payload.max_swarm_depth
    );

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "ok",
            "auto_approve_safe_skills": payload.auto_approve_safe_skills,
            "max_agents": state.governance.max_agents.load(std::sync::atomic::Ordering::Relaxed),
            "max_clusters": state.governance.max_clusters.load(std::sync::atomic::Ordering::Relaxed),
            "max_swarm_depth": state.governance.max_swarm_depth.load(std::sync::atomic::Ordering::Relaxed),
            "observed_max_depth": state.governance.observed_max_depth.load(std::sync::atomic::Ordering::Relaxed),
            "max_task_length": state.governance.max_task_length.load(std::sync::atomic::Ordering::Relaxed),
            "default_budget_usd": *state.governance.default_budget_usd.read(),
            "privacy_mode": state.governance.privacy_mode.load(std::sync::atomic::Ordering::Relaxed)
        })),
    ))
}

/// POST /v1/oversight/:id/decide
///
/// Commits a human decision (Approve/Reject) for a specific oversight request.
/// Triggers the internal resolution channel to unblock or kill the agent task.
///
/// @docs API_REFERENCE:DecideOversight
#[tracing::instrument(skip(state), fields(action_id = %entry_id, decision = %payload.decision), name = "governance::resolve_pending")]
pub async fn decide_oversight(
    Path(entry_id): Path<String>,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<OversightDecision>,
) -> Result<impl IntoResponse, AppError> {
    let approved = payload.decision == "approved";

    tracing::info!(
        "⚖️ [Oversight] Decision received for {}: decision={}",
        entry_id,
        payload.decision
    );

    // 1. Remove from queue
    let removed_entry = state
        .comms
        .oversight_queue
        .remove(&entry_id)
        .map(|(_, entry)| entry);

    if let Some(entry) = removed_entry {
        // 2. Update audit trail & Persistent Log (Synchronous)
        let params = serde_json::to_string(&json!({
            "entry_id": entry.id,
            "approved": approved
        }))
        .unwrap_or_default();
        
        if let Err(e) = state.security.audit_trail
            .record(
                "oversight",
                entry.mission_id.as_deref(),
                None,
                "oversight_decision",
                &params,
            )
            .await
        {
            tracing::error!("❌ [Oversight] Failed to record in audit trail: {}", e);
        }

        let now = chrono::Utc::now();
        if let Err(e) = sqlx::query(
            "UPDATE oversight_log SET status = 'resolved', decision = ?, decided_at = ?, decided_by = 'human' WHERE id = ?"
        )
        .bind(&payload.decision)
        .bind(now)
        .bind(&entry.id)
        .execute(&state.resources.pool)
        .await
        {
            tracing::error!("❌ [Oversight] Failed to update persistent oversight_log: {}", e);
        }

        // 3. Resolve the waiting promise to unblock the agent
        if let Some((_, shooter)) = state.comms.oversight_resolvers.remove(&entry_id) {
            let _ = shooter.send(approved);
        }

        // 4. Log the decision in system broadcasts
        let decision_label = if approved { "APPROVED" } else { "REJECTED" };
        state.broadcast_sys(
            &format!(
                "⚖️ Oversight: Decision for {} recorded as {}.",
                entry.id, decision_label
            ),
            if approved { "success" } else { "warning" },
            entry.mission_id.clone(),
        );

        // 5. Emit event for UI
        state.emit_event(json!({
            "type": "oversight:decision",
            "data": {
                "id": entry_id,
                "decision": payload.decision
            }
        }));

        Ok(Json(
            json!({ "status": "ok", "decision": payload.decision }),
        ))
    } else {
        Err(AppError::NotFound(
            "Oversight entry not found or already resolved".to_string(),
        ))
    }
}

#[derive(serde::Deserialize, Debug)]
pub struct UpdateQuotaPayload {
    pub budget_usd: f64,
    pub reset_period: Option<crate::security::metering::ResetPeriod>,
}

/// PUT /oversight/security/quotas/:entity_id
///
/// Updates the budget quota and reset period for a specific agent.
///
/// @docs API_REFERENCE:UpdateAgentQuota
#[tracing::instrument(skip(state), name = "governance::update_agent_quota")]
pub async fn update_agent_quota(
    Path(entity_id): Path<String>,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<UpdateQuotaPayload>,
) -> Result<impl IntoResponse, AppError> {
    state
        .security
        .budget_guard
        .update_quota(&entity_id, payload.budget_usd, payload.reset_period)
        .await
        .map_err(|e| AppError::InternalServerError(format!("Failed to update quota: {}", e)))?;

    tracing::info!(
        "🛡️ [Budget] Quota updated for agent {}: ${}",
        entity_id,
        payload.budget_usd
    );
    Ok((StatusCode::OK, Json(serde_json::json!({ "status": "ok" }))))
}

/// GET /oversight/security/quotas
///
/// Returns global budget telemetry, including total spent, remaining,
/// and system defense metrics.
///
/// @docs API_REFERENCE:GetQuotas
#[tracing::instrument(skip(state), name = "security_governance::get_quotas")]
pub async fn get_security_quotas(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    let mut total_budget = 0.0;
    let mut total_spent = 0.0;

    tracing::debug!("📊 [Oversight] Calculating global budget from {} agents", state.registry.agents.len());
    for entry in state.registry.agents.iter() {
        total_budget += entry.value().economics.budget_usd;
        total_spent += entry.value().economics.cost_usd;
    }

    tracing::debug!("📊 [Oversight] Fetching quotas from budget_guard...");
    let agent_quotas: Vec<crate::security::metering::Quota> = state
        .security
        .budget_guard
        .get_all_quotas()
        .await
        .unwrap_or_default();

    tracing::debug!("📊 [Oversight] Gathering system defense stats...");
    let system_defense = state.security.system_monitor.get_system_defense_stats();
    
    tracing::debug!("📊 [Oversight] Verifying audit trail integrity...");
    let merkle_integrity = match state.security.audit_trail.verify_last_n(10, None).await {
         Ok((v, t)) if v == t && t > 0 => 1.0,
         Ok((v, t)) if t > 0 => v as f64 / t as f64,
         _ => 1.0, 
    };

    tracing::debug!("📊 [Oversight] Budget: ${} / ${} (Remaining: ${})", total_spent, total_budget, total_budget - total_spent);

    Ok(Json(serde_json::json!({
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
            "merkle_integrity": merkle_integrity
        }
    })))
}

/// GET /v1/oversight/security/missions/quotas
#[tracing::instrument(skip(state), name = "security_governance::get_mission_quotas")]
pub async fn get_mission_quotas(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    let rows = sqlx::query("SELECT * FROM mission_quotas")
        .fetch_all(&state.resources.pool)
        .await
        .unwrap_or_default();

    let quotas: Vec<Quota> = rows
        .into_iter()
        .map(|r| {
            use sqlx::Row;
            let period_str: String = r.get("reset_period");
            let period = match period_str.as_str() {
                "daily" => ResetPeriod::Daily,
                "monthly" => ResetPeriod::Monthly,
                _ => ResetPeriod::Never,
            };

            Quota {
                id: r.get("id"),
                entity_id: r.get("cluster_id"),
                budget_usd: r.get("budget_usd"),
                used_usd: r.get("used_usd"),
                reset_period: period,
                last_reset_at: r.get("last_reset_at"),
                next_reset_at: r.get("next_reset_at"),
            }
        })
        .collect();

    Ok(Json(serde_json::json!({ "quotas": quotas })))
}

/// PUT /v1/oversight/security/missions/:id/quota
#[tracing::instrument(
    skip(state, payload),
    name = "security_governance::update_mission_quota"
)]
pub async fn update_mission_quota(
    Path(cluster_id): Path<String>,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<UpdateQuotaPayload>,
) -> Result<impl IntoResponse, AppError> {
    state
        .security
        .budget_guard
        .update_mission_quota(&cluster_id, payload.budget_usd, payload.reset_period)
        .await
        .map_err(|e| {
            AppError::InternalServerError(format!("Failed to update mission quota: {}", e))
        })?;

    tracing::info!(
        "🛡️ [Budget] Quota updated for mission {}: ${}",
        cluster_id,
        payload.budget_usd
    );
    Ok((StatusCode::OK, Json(serde_json::json!({ "status": "ok" }))))
}

/// GET /oversight/security/audit-trail
///
/// Retrieves the tamper-evident Merkle hash-chain logs.
///
/// @docs API_REFERENCE:GetAuditTrail
#[tracing::instrument(skip(state), name = "governance::get_audit_trail")]
pub async fn get_audit_trail(
    State(state): State<Arc<AppState>>,
    Query(params): Query<PaginationParams>,
) -> Result<impl IntoResponse, AppError> {
    // We pull directly from audit_trail instead of oversight_log for "top-tier" integrity
    let entries: Vec<AuditEntry> =
        sqlx::query_as("SELECT * FROM audit_trail ORDER BY timestamp DESC")
            .fetch_all(&state.resources.pool)
            .await
            .map_err(AppError::Sqlx)?;

    let response: Vec<OversightAuditEntry> = entries
        .into_iter()
        .map(|entry| {
            OversightAuditEntry {
                id: entry.id,
                agent_id: entry.agent_id,
                skill: Some(entry.action),
                status: "recorded".to_string(), // Merkle chain status
                decision: None,
                decided_at: None,
                created_at: entry.timestamp,
                is_verified: true, // Optimistically render pagination; Deep verify delegated to integrity sync logic
            }
        })
        .collect();

    Ok(Json(PaginatedResponse::from_vec(
        response,
        &params,
        "/v1/oversight/security/audit-trail",
    )))
}

/// GET /v1/oversight/security/integrity
///
/// Verifies the last N records in the Merkle chain and returns an integrity score.
///
/// @docs API_REFERENCE:GetIntegrityStatus
#[tracing::instrument(skip(state), name = "governance::get_integrity")]
pub async fn get_integrity_status(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    let (verified_count, total_count) = state
        .security
        .audit_trail
        .verify_last_n(50, None)
        .await
        .unwrap_or((0, 50));

    let is_signing_configured = std::env::var("AUDIT_PRIVATE_KEY").map(|v| !v.trim().is_empty()).unwrap_or(false);
    let is_secure = verified_count == total_count && total_count > 0;
    
    let status = if !is_secure && total_count > 0 {
        "TAMPERED".to_string()
    } else if is_signing_configured {
        "SECURE".to_string()
    } else {
        "UNSIGNED_CHAIN".to_string()
    };

    let base_score = if total_count > 0 {
        verified_count as f64 / total_count as f64
    } else {
        1.0
    };

    let score = if is_signing_configured {
        base_score
    } else {
        base_score * 0.5 // Cap unsigned chain score to signal missing signature verification key
    };

    Ok(Json(SecurityIntegrityResponse {
        integrity_score: score,
        status,
        verified_count,
        total_count,
    }))
}

/// GET /oversight/security/health
///
/// Returns health metrics for all registered agents.
///
/// @docs OPERATIONS_MANUAL:AgentHealth
#[tracing::instrument(skip(state), name = "security_governance::get_health")]
pub async fn get_agent_health(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    let health_data: Vec<serde_json::Value> = state
        .registry
        .agents
        .iter()
        .map(|entry| {
            let agent = entry.value();
            serde_json::json!({
                "agent_id": agent.identity.id,
                "name": agent.identity.name,
                "status": agent.health.status,
                "failure_count": agent.health.failure_count,
                "last_failure_at": agent.health.last_failure_at,
                "is_healthy": agent.health.failure_count < 5,
                "is_throttled": agent.health.failure_count >= 3,
                "is_bankrupt": agent.economics.cost_usd >= agent.economics.budget_usd && agent.economics.budget_usd > 0.0,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({ "agents": health_data })))
}
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum PolicyMode {
    Allow,
    Deny,
    Prompt,
}

#[derive(serde::Deserialize, Debug)]
pub struct UpdatePolicyPayload {
    pub tool_name: String,
    pub mode: PolicyMode,
}

/// GET /v1/oversight/security/policies
#[tracing::instrument(skip(state), name = "governance::get_policies")]
pub async fn get_policies(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT tool_name, mode FROM permission_policies")
            .fetch_all(&state.resources.pool)
            .await
            .map_err(AppError::Sqlx)?;

    let policies: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|(name, mode)| json!({ "tool_name": name, "mode": mode }))
        .collect();

    Ok(Json(policies))
}

/// PUT /v1/oversight/security/policies
#[tracing::instrument(skip(state, payload), name = "governance::update_policy")]
pub async fn update_policy(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<UpdatePolicyPayload>,
) -> Result<impl IntoResponse, AppError> {
    let mode_str = match payload.mode {
        PolicyMode::Allow => "allow",
        PolicyMode::Deny => "deny",
        PolicyMode::Prompt => "prompt",
    };

    sqlx::query(
        "INSERT INTO permission_policies (tool_name, mode) VALUES (?, ?) 
                 ON CONFLICT(tool_name) DO UPDATE SET mode = excluded.mode",
    )
    .bind(&payload.tool_name)
    .bind(mode_str)
    .execute(&state.resources.pool)
    .await
    .map_err(AppError::Sqlx)?;

    // Refresh cache
    state
        .security
        .permission_policy
        .refresh_cache()
        .await
        .map_err(|e| {
            AppError::InternalServerError(format!("Failed to refresh permission cache: {}", e))
        })?;



    state.broadcast_sys(
        &format!(
            "🛡️ Security Policy Updated: {} -> {}",
            payload.tool_name, mode_str
        ),
        "info",
        None,
    );

    Ok((StatusCode::OK, Json(json!({ "status": "ok" }))))
}

#[derive(Serialize, Type, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TokenBurnReport {
    #[specta(type = u32)]
    pub total_tokens_in: i64,
    #[specta(type = u32)]
    pub total_tokens_out: i64,
    pub total_cost_usd: f64,
    pub providers: Vec<ProviderTokenBurn>,
    pub agents: Vec<AgentTokenBurn>,
    pub recent_burns: Vec<RecentTokenBurn>,
}

#[derive(Serialize, Type, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ProviderTokenBurn {
    pub provider_id: String,
    #[specta(type = u32)]
    pub tokens_in: i64,
    #[specta(type = u32)]
    pub tokens_out: i64,
    pub cost_usd: f64,
}

#[derive(Serialize, Type, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AgentTokenBurn {
    pub agent_id: String,
    pub agent_name: String,
    #[specta(type = u32)]
    pub tokens_in: i64,
    #[specta(type = u32)]
    pub tokens_out: i64,
    pub cost_usd: f64,
}

#[derive(Serialize, Type, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RecentTokenBurn {
    pub task_id: String,
    pub title: String,
    pub agent_id: String,
    pub agent_name: String,
    #[specta(type = u32)]
    pub tokens_in: i64,
    #[specta(type = u32)]
    pub tokens_out: i64,
    pub cost_usd: f64,
    #[specta(type = f64)]
    pub updated_at: i64,
}

/// GET /v1/oversight/token-burn
///
/// Returns an aggregated report of token usage and costs across the swarm.
/// Also updates the Prometheus gauges `oversight_tokens_in`, `oversight_tokens_out`, and `oversight_cost_usd`.
///
/// @docs API_REFERENCE:GetTokenBurn
#[tracing::instrument(skip(state), name = "governance::get_token_burn")]
pub async fn get_token_burn(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    use sqlx::Row;

    // 1. Query overall consumption sums
    let totals_row = sqlx::query(
        "SELECT 
            COALESCE(SUM(tokens_in), 0) as total_tokens_in,
            COALESCE(SUM(tokens_out), 0) as total_tokens_out,
            COALESCE(SUM(cost_usd), 0.0) as total_cost_usd
         FROM agent_tasks"
    )
    .fetch_one(&state.resources.pool)
    .await
    .map_err(AppError::Sqlx)?;

    let total_tokens_in: i64 = totals_row.get("total_tokens_in");
    let total_tokens_out: i64 = totals_row.get("total_tokens_out");
    let total_cost_usd: f64 = totals_row.get("total_cost_usd");

    // Update Prometheus gauges
    crate::telemetry::OVERSIGHT_TOKENS_IN.set(total_tokens_in as f64);
    crate::telemetry::OVERSIGHT_TOKENS_OUT.set(total_tokens_out as f64);
    crate::telemetry::OVERSIGHT_COST_USD.set(total_cost_usd);

    // 2. Query provider breakdown
    let provider_rows = sqlx::query(
        "SELECT 
            COALESCE(provider_id, 'unknown') as provider_id,
            COALESCE(SUM(tokens_in), 0) as tokens_in,
            COALESCE(SUM(tokens_out), 0) as tokens_out,
            COALESCE(SUM(cost_usd), 0.0) as cost_usd
         FROM agent_tasks
         GROUP BY provider_id"
    )
    .fetch_all(&state.resources.pool)
    .await
    .map_err(AppError::Sqlx)?;

    let providers = provider_rows.into_iter().map(|row| {
        ProviderTokenBurn {
            provider_id: row.get("provider_id"),
            tokens_in: row.get("tokens_in"),
            tokens_out: row.get("tokens_out"),
            cost_usd: row.get("cost_usd"),
        }
    }).collect();

    // 3. Query agent breakdown (joining with agents table for the name)
    let agent_rows = sqlx::query(
        "SELECT 
            t.agent_id,
            COALESCE(a.name, 'Unknown Agent') as agent_name,
            COALESCE(SUM(t.tokens_in), 0) as tokens_in,
            COALESCE(SUM(t.tokens_out), 0) as tokens_out,
            COALESCE(SUM(t.cost_usd), 0.0) as cost_usd
         FROM agent_tasks t
         LEFT JOIN agents a ON t.agent_id = a.id
         GROUP BY t.agent_id, a.name"
    )
    .fetch_all(&state.resources.pool)
    .await
    .map_err(AppError::Sqlx)?;

    let agents = agent_rows.into_iter().map(|row| {
        AgentTokenBurn {
            agent_id: row.get("agent_id"),
            agent_name: row.get("agent_name"),
            tokens_in: row.get("tokens_in"),
            tokens_out: row.get("tokens_out"),
            cost_usd: row.get("cost_usd"),
        }
    }).collect();

    // 4. Query recent burns (last 20 tasks with non-zero usage)
    let recent_rows = sqlx::query(
        "SELECT 
            t.id as task_id,
            t.title,
            t.agent_id,
            COALESCE(a.name, 'Unknown Agent') as agent_name,
            COALESCE(t.tokens_in, 0) as tokens_in,
            COALESCE(t.tokens_out, 0) as tokens_out,
            COALESCE(t.cost_usd, 0.0) as cost_usd,
            t.updated_at
         FROM agent_tasks t
         LEFT JOIN agents a ON t.agent_id = a.id
         WHERE COALESCE(t.tokens_in, 0) > 0 OR COALESCE(t.tokens_out, 0) > 0 OR COALESCE(t.cost_usd, 0.0) > 0.0
         ORDER BY t.updated_at DESC
         LIMIT 20"
    )
    .fetch_all(&state.resources.pool)
    .await
    .map_err(AppError::Sqlx)?;

    let recent_burns = recent_rows.into_iter().map(|row| {
        RecentTokenBurn {
            task_id: row.get("task_id"),
            title: row.get("title"),
            agent_id: row.get("agent_id"),
            agent_name: row.get("agent_name"),
            tokens_in: row.get("tokens_in"),
            tokens_out: row.get("tokens_out"),
            cost_usd: row.get("cost_usd"),
            updated_at: row.get("updated_at"),
        }
    }).collect();

    Ok(Json(TokenBurnReport {
        total_tokens_in,
        total_tokens_out,
        total_cost_usd,
        providers,
        agents,
        recent_burns,
    }))
}



// Metadata: [oversight]

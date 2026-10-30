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

use crate::agent::types::{OversightDecision, OversightEntry, ToolCallAudit};
use crate::error::AppError;
use crate::middleware::auth::AuthenticatedOperator;
use crate::routes::pagination::{PaginatedResponse, PaginationParams};
use crate::security::audit::AuditEntry;
use crate::security::metering::{Quota, ResetPeriod};
use crate::state::AppState;
use axum::{
    extract::{FromRequestParts, Path, Query, State},
    http::{request::Parts, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;
use sqlx::{Row, SqlitePool};
use std::collections::HashSet;
use std::sync::{atomic::Ordering, Arc};

/// Number of trailing audit records verified by `GET /v1/oversight/security/integrity`.
pub const INTEGRITY_VERIFY_WINDOW: usize = 50;

/// Score multiplier applied when the SHA-256 hash chain is intact but Ed25519 signing is not enabled.
pub const UNSIGNED_CHAIN_SCORE_MULTIPLIER: f64 = 0.5;

/// Upper-bound ceiling for `max_agents` governance setting.
pub const MAX_AGENTS_CEILING: u32 = 10_000;

/// Upper-bound ceiling for `max_clusters` governance setting.
pub const MAX_CLUSTERS_CEILING: u32 = 1_000;

/// Upper-bound ceiling for `max_swarm_depth` governance setting.
pub const MAX_SWARM_DEPTH_CEILING: u32 = 64;

/// Upper-bound ceiling for `max_task_length` governance setting (1 MiB).
pub const MAX_TASK_LENGTH_CEILING: usize = 1_048_576;

/// Upper-bound ceiling for `default_budget_usd` governance setting.
pub const MAX_DEFAULT_BUDGET_USD_CEILING: f64 = 1_000_000.0;

/// Failure count threshold below which a non-suspended agent is considered healthy.
pub const HEALTHY_FAILURE_LIMIT: u32 = 5;

/// Failure count threshold at or above which an agent is flagged as throttled.
pub const THROTTLED_FAILURE_THRESHOLD: u32 = 3;

/// Maximum length for path/body identifier strings (`entry_id`, `entity_id`, `cluster_id`, `tool_name`).
const MAX_IDENTIFIER_LEN: usize = 128;

/// Server-verified operator principal for mutating governance and oversight endpoints.
///
/// Extracted exclusively from server-verified request extensions (`OversightPrincipal` or
/// `AuthenticatedOperator` installed by `middleware::auth::validate_token`). Never constructed
/// from unauthenticated client headers or request body properties (Gate 4 Trust Boundary).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OversightPrincipal {
    /// Verified operator identifier recorded in `oversight_log.decided_by` and `audit_trail.user_id`.
    pub id: String,
    /// Optional mission/cluster scope whitelist. `None` grants global operator authority.
    pub allowed_missions: Option<HashSet<String>>,
}

impl OversightPrincipal {
    /// Constructs a global operator principal with unrestricted mission scope.
    pub fn global(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            allowed_missions: None,
        }
    }

    /// Constructs a mission-scoped principal restricted to specific `mission_id`s.
    pub fn scoped<I, S>(id: impl Into<String>, missions: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            id: id.into(),
            allowed_missions: Some(missions.into_iter().map(Into::into).collect()),
        }
    }

    fn ensure_authenticated(&self) -> Result<&str, AppError> {
        let trimmed = self.id.trim();
        if trimmed.is_empty() {
            return Err(AppError::Unauthorized(
                "Authentication required: operator principal ID cannot be empty".to_string(),
            ));
        }
        Ok(trimmed)
    }

    /// Verifies that the principal is authorized for global (cluster-wide) settings/policy mutations.
    pub fn authorize_global(&self) -> Result<&str, AppError> {
        let id = self.ensure_authenticated()?;
        if let Some(ref allowed) = self.allowed_missions {
            if !allowed.contains("*") {
                return Err(AppError::Forbidden(format!(
                    "Principal '{}' is mission-scoped and cannot mutate global governance policy",
                    id
                )));
            }
        }
        Ok(id)
    }

    /// Verifies that the principal has oversight authority for the target `mission_id`.
    pub fn authorize_mission(&self, mission_id: Option<&str>) -> Result<&str, AppError> {
        let id = self.ensure_authenticated()?;
        if let Some(ref allowed) = self.allowed_missions {
            if allowed.contains("*") {
                return Ok(id);
            }
            match mission_id.map(str::trim).filter(|m| !m.is_empty()) {
                Some(mid) if allowed.contains(mid) => Ok(id),
                Some(mid) => Err(AppError::Forbidden(format!(
                    "Principal '{}' is not authorized for mission '{}'",
                    id, mid
                ))),
                None => Err(AppError::Forbidden(format!(
                    "Principal '{}' is mission-scoped and cannot resolve global oversight entries",
                    id
                ))),
            }
        } else {
            Ok(id)
        }
    }
}

impl<S> FromRequestParts<S> for OversightPrincipal
where
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        if let Some(principal) = parts.extensions.get::<OversightPrincipal>() {
            principal.ensure_authenticated()?;
            return Ok(principal.clone());
        }
        if parts.extensions.get::<AuthenticatedOperator>().is_some() {
            return Ok(OversightPrincipal::global("authenticated_operator"));
        }
        Err(AppError::Unauthorized(
            "Authentication required: missing verified operator principal".to_string(),
        ))
    }
}

#[derive(Serialize, Deserialize, Type, Debug, Clone)]
pub struct OversightAuditEntry {
    pub id: String,
    pub agent_id: String,
    pub action: String,
    pub skill: Option<String>,
    pub status: String,
    pub decision: Option<String>,
    pub decided_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub prev_hash: Option<String>,
    pub current_hash: Option<String>,
    pub signature: Option<String>,
    pub is_verified: bool,
}

#[derive(Serialize, Deserialize, Type, Debug, Clone)]
pub struct SecurityIntegrityResponse {
    pub integrity_score: Option<f64>,
    pub status: String,
    pub verified_count: usize,
    pub total_count: usize,
    pub verified_window: usize,
    pub chain_head: Option<String>,
    pub checked_at: chrono::DateTime<chrono::Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Validates a path or payload identifier (`entry_id`, `entity_id`, `cluster_id`, `tool_name`).
pub(crate) fn validate_identifier(raw: &str, field_name: &str) -> Result<String, AppError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(AppError::BadRequest(format!(
            "{} cannot be empty",
            field_name
        )));
    }
    if trimmed.len() > MAX_IDENTIFIER_LEN {
        return Err(AppError::BadRequest(format!(
            "{} exceeds maximum length of {} characters",
            field_name, MAX_IDENTIFIER_LEN
        )));
    }
    if trimmed.chars().any(|c| c.is_control()) {
        return Err(AppError::BadRequest(format!(
            "{} contains invalid control characters",
            field_name
        )));
    }
    Ok(trimmed.to_string())
}

/// Validates that a budget quota is a non-negative finite floating-point number within ceiling bounds.
pub(crate) fn validate_quota_budget(budget_usd: f64) -> Result<(), AppError> {
    if !budget_usd.is_finite() || budget_usd < 0.0 || budget_usd > MAX_DEFAULT_BUDGET_USD_CEILING {
        return Err(AppError::BadRequest(
            "Budget quota must be a non-negative finite number within ceiling limits".to_string(),
        ));
    }
    Ok(())
}

/// Validates global governance settings prior to mutating `GovernanceHub` atomics.
pub(crate) fn validate_oversight_settings(
    payload: &OversightSettingsPayload,
) -> Result<(), AppError> {
    if let Some(budget) = payload.default_budget_usd {
        validate_quota_budget(budget)?;
    }
    if let Some(agents) = payload.max_agents {
        if agents == 0 || agents > MAX_AGENTS_CEILING {
            return Err(AppError::BadRequest(format!(
                "max_agents must be between 1 and {}",
                MAX_AGENTS_CEILING
            )));
        }
    }
    if let Some(clusters) = payload.max_clusters {
        if clusters == 0 || clusters > MAX_CLUSTERS_CEILING {
            return Err(AppError::BadRequest(format!(
                "max_clusters must be between 1 and {}",
                MAX_CLUSTERS_CEILING
            )));
        }
    }
    if let Some(depth) = payload.max_swarm_depth {
        if depth == 0 || depth > MAX_SWARM_DEPTH_CEILING {
            return Err(AppError::BadRequest(format!(
                "max_swarm_depth must be between 1 and {}",
                MAX_SWARM_DEPTH_CEILING
            )));
        }
    }
    if let Some(task_len) = payload.max_task_length {
        if task_len == 0 || task_len > MAX_TASK_LENGTH_CEILING {
            return Err(AppError::BadRequest(format!(
                "max_task_length must be between 1 and {}",
                MAX_TASK_LENGTH_CEILING
            )));
        }
    }
    Ok(())
}

/// Validates and normalizes an operator oversight decision token (`"approved"` or `"rejected"`).
pub(crate) fn parse_oversight_decision(raw: &str) -> Result<(&'static str, bool), AppError> {
    let trimmed = raw.trim();
    match trimmed.to_ascii_lowercase().as_str() {
        "approved" => Ok(("approved", true)),
        "rejected" => Ok(("rejected", false)),
        _ => {
            let safe_echo: String = trimmed
                .chars()
                .filter(|c| !c.is_control())
                .take(32)
                .collect();
            Err(AppError::BadRequest(format!(
                "Invalid oversight decision '{}': expected 'approved' or 'rejected'",
                safe_echo
            )))
        }
    }
}

fn is_sensitive_json_key(key: &str) -> bool {
    let lower = key.trim().to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "api_key"
            | "apikey"
            | "secret"
            | "password"
            | "token"
            | "credential"
            | "neural_token"
            | "access_token"
            | "refresh_token"
            | "authorization"
            | "x-api-key"
            | "private_key"
    ) || lower.ends_with("_api_key")
        || lower.ends_with("_token")
        || lower.ends_with("_secret")
}

/// Recursively scrubs sensitive keys and secret patterns across a JSON value tree.
pub(crate) fn redact_json_value(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::String(s) => {
            *s = crate::utils::security::redact_secrets(s);
        }
        serde_json::Value::Array(arr) => {
            for item in arr.iter_mut() {
                redact_json_value(item);
            }
        }
        serde_json::Value::Object(map) => {
            for (k, v) in map.iter_mut() {
                if is_sensitive_json_key(k) {
                    if let serde_json::Value::String(s) = v {
                        if !s.is_empty() {
                            *s = "[REDACTED]".to_string();
                            continue;
                        }
                    }
                }
                redact_json_value(v);
            }
        }
        _ => {}
    }
}

fn parse_and_redact_ledger_json(
    raw: Option<String>,
    field_name: &str,
    entry_id: &str,
) -> serde_json::Value {
    let Some(raw_str) = raw else {
        return serde_json::Value::Null;
    };
    let trimmed = raw_str.trim();
    if trimmed.is_empty() {
        return serde_json::Value::Null;
    }
    let mut parsed = serde_json::from_str::<serde_json::Value>(trimmed).unwrap_or_else(|e| {
        tracing::warn!(
            entry_id = %entry_id,
            field = %field_name,
            error = %e,
            "Malformed JSON in oversight_log"
        );
        json!({ "_parse_error": e.to_string() })
    });
    redact_json_value(&mut parsed);
    parsed
}

/// Maps a single SQLite row from the `oversight_log` + `audit_trail` join into the ledger JSON payload.
/// Uses `Row::try_get` exclusively to prevent panics on NULL or mistyped columns.
fn map_oversight_ledger_row(row: &sqlx::sqlite::SqliteRow) -> Result<serde_json::Value, AppError> {
    let id: String = row.try_get("id").map_err(AppError::Sqlx)?;
    let mission_id: Option<String> = row.try_get("mission_id").map_err(AppError::Sqlx)?;
    let agent_id: String = row
        .try_get::<Option<String>, _>("agent_id")
        .map_err(AppError::Sqlx)?
        .unwrap_or_else(|| "unknown".to_string());
    let entry_type: String = row
        .try_get::<Option<String>, _>("entry_type")
        .map_err(AppError::Sqlx)?
        .unwrap_or_else(|| "tool_call".to_string());
    let skill: Option<String> = row.try_get("skill").map_err(AppError::Sqlx)?;
    let raw_params: Option<String> = row.try_get("params").map_err(AppError::Sqlx)?;
    let raw_payload: Option<String> = row.try_get("payload").map_err(AppError::Sqlx)?;
    let status: String = row
        .try_get::<Option<String>, _>("status")
        .map_err(AppError::Sqlx)?
        .unwrap_or_else(|| "resolved".to_string());
    let decision_val: Option<String> = row.try_get("decision").map_err(AppError::Sqlx)?;
    let decided_at: Option<chrono::DateTime<chrono::Utc>> = row
        .try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("decided_at")
        .ok()
        .flatten();
    let decided_by_val: Option<String> = row.try_get("decided_by").map_err(AppError::Sqlx)?;
    let created_at: chrono::DateTime<chrono::Utc> = row
        .try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("created_at")
        .ok()
        .flatten()
        .unwrap_or_else(chrono::Utc::now);

    let audit_action = row
        .try_get::<Option<String>, _>("audit_action")
        .ok()
        .flatten();
    let audit_timestamp = row
        .try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("audit_timestamp")
        .ok()
        .flatten();
    let audit_params = row
        .try_get::<Option<String>, _>("audit_params")
        .ok()
        .flatten();

    let result_val = if let Some(action) = audit_action {
        let success = action.starts_with("[SUCCESS]");
        let (duration_ms, timestamp_anomaly, duration_unknown) =
            if let (Some(decided), Some(completed)) = (decided_at, audit_timestamp) {
                let ms = completed.signed_duration_since(decided).num_milliseconds();
                (Some(ms), ms < 0, false)
            } else {
                (None, false, true)
            };

        let output_str = audit_params
            .map(|p| crate::utils::security::redact_secrets(&p))
            .unwrap_or_else(|| "No execution output recorded".to_string());

        if success {
            json!({
                "success": true,
                "output": output_str,
                "duration_ms": duration_ms,
                "timestamp_anomaly": timestamp_anomaly,
                "duration_unknown": duration_unknown
            })
        } else {
            json!({
                "success": false,
                "output": output_str,
                "error": output_str,
                "duration_ms": duration_ms,
                "timestamp_anomaly": timestamp_anomaly,
                "duration_unknown": duration_unknown
            })
        }
    } else {
        serde_json::Value::Null
    };

    let is_auto = decided_by_val.as_deref() == Some("auto_policy")
        || decided_by_val.as_deref() == Some("system")
        || decision_val.as_deref() == Some("auto_approved");

    let redacted_params = parse_and_redact_ledger_json(raw_params, "params", &id);
    let redacted_payload = parse_and_redact_ledger_json(raw_payload.clone(), "payload", &id);
    let redacted_description = raw_payload
        .map(|p| crate::utils::security::redact_secrets(&p))
        .unwrap_or_else(|| "Historical action".to_string());

    let mut entry_obj = json!({
        "id": id,
        "mission_id": mission_id,
        "tool_call": {
            "id": format!("tc-{}", id),
            "agent_id": agent_id,
            "skill": skill,
            "params": redacted_params,
            "description": redacted_description,
        },
        "type": entry_type,
        "status": status,
        "decision": decision_val,
        "decided_at": decided_at,
        "decided_by": decided_by_val,
        "auto_approved": is_auto,
        "approval_type": if is_auto { "auto" } else { "hitl" },
        "requires_oversight": !is_auto,
        "created_at": created_at,
        "payload": redacted_payload,
    });

    if let Some(obj) = entry_obj.as_object_mut() {
        if !result_val.is_null() {
            obj.insert("result".to_string(), result_val);
        }
    }

    Ok(entry_obj)
}

#[derive(Debug, Deserialize)]
pub struct PendingOversightQuery {
    #[serde(default = "default_pending_page")]
    pub page: u32,
    #[serde(default = "default_pending_per_page")]
    pub per_page: u32,
    pub mission_id: Option<String>,
    pub agent_id: Option<String>,
    pub skill: Option<String>,
}

fn default_pending_page() -> u32 {
    1
}
fn default_pending_per_page() -> u32 {
    25
}

impl Default for PendingOversightQuery {
    fn default() -> Self {
        Self {
            page: 1,
            per_page: 25,
            mission_id: None,
            agent_id: None,
            skill: None,
        }
    }
}

impl PendingOversightQuery {
    fn to_pagination_params(&self) -> PaginationParams {
        PaginationParams {
            page: self.page,
            per_page: self.per_page,
        }
    }

    fn matches(&self, entry: &OversightEntry) -> bool {
        if let Some(ref want_mission) = self.mission_id {
            if entry.mission_id.as_deref() != Some(want_mission.as_str()) {
                return false;
            }
        }
        if let Some(ref want_agent) = self.agent_id {
            let entry_agent = entry.tool_call.as_ref().map(|tc| tc.agent_id.as_str());
            if entry_agent != Some(want_agent.as_str()) {
                return false;
            }
        }
        if let Some(ref want_skill) = self.skill {
            let entry_skill = entry
                .tool_call
                .as_ref()
                .map(|tc| tc.skill.as_str())
                .or_else(|| entry.skill_proposal.as_ref().map(|sp| sp.name.as_str()));
            if entry_skill != Some(want_skill.as_str()) {
                return false;
            }
        }
        true
    }
}

/// GET /v1/oversight/pending
///
/// Returns a collection of all actions (file edits, network requests, etc.)
/// currently paused and awaiting human verification, reconciling the in-memory
/// `oversight_queue` with durable `status = 'pending'` rows in `oversight_log`.
///
/// @docs API_REFERENCE:GetPendingOversight
#[tracing::instrument(skip(state), name = "governance::list_pending")]
pub async fn get_pending(
    State(state): State<Arc<AppState>>,
    Query(query): Query<PendingOversightQuery>,
) -> Result<impl IntoResponse, AppError> {
    let mut seen_ids = HashSet::new();
    let mut entries: Vec<OversightEntry> = state
        .comms
        .oversight_queue
        .iter()
        .filter(|entry| query.matches(entry.value()))
        .map(|entry| {
            seen_ids.insert(entry.key().clone());
            entry.value().clone()
        })
        .collect();

    if let Ok(db_rows) = sqlx::query(
        "SELECT id, mission_id, agent_id, skill, params, status, created_at \
         FROM oversight_log WHERE status = 'pending' ORDER BY created_at DESC LIMIT 500",
    )
    .fetch_all(&state.resources.pool)
    .await
    {
        for row in db_rows {
            let Ok(id) = row.try_get::<String, _>("id") else {
                continue;
            };
            if seen_ids.contains(&id) {
                continue;
            }
            let mission_id: Option<String> = row.try_get("mission_id").ok().flatten();
            let agent_id: String = row
                .try_get::<Option<String>, _>("agent_id")
                .ok()
                .flatten()
                .unwrap_or_else(|| "unknown".to_string());
            let skill: String = row
                .try_get::<Option<String>, _>("skill")
                .ok()
                .flatten()
                .unwrap_or_else(|| "unknown".to_string());
            let raw_params: Option<String> = row.try_get("params").ok().flatten();
            let params_val = parse_and_redact_ledger_json(raw_params, "params", &id);
            let created_at = row
                .try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("created_at")
                .ok()
                .flatten()
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());

            let reconstructed = OversightEntry {
                id: id.clone(),
                mission_id: mission_id.clone(),
                tool_call: Some(ToolCallAudit {
                    id: format!("tc-{}", id),
                    agent_id,
                    mission_id,
                    skill,
                    params: params_val,
                    department: "governance".to_string(),
                    description: "Reconstructed pending oversight action".to_string(),
                    timestamp: created_at.clone(),
                }),
                skill_proposal: None,
                status: "pending".to_string(),
                created_at,
            };
            if query.matches(&reconstructed) {
                seen_ids.insert(id);
                entries.push(reconstructed);
            }
        }
    }

    entries.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| a.id.cmp(&b.id)));

    let pagination_params = query.to_pagination_params();
    Ok(Json(PaginatedResponse::from_vec(
        entries,
        &pagination_params,
        "/v1/oversight/pending",
    )))
}

/// GET /v1/oversight/ledger
///
/// Provides a historical audit trail of all previous oversight decisions.
/// Directly queries the SQLite persistence layer inside a consistent read transaction.
///
/// @docs API_REFERENCE:GetOversightLedger
#[tracing::instrument(skip(state), name = "governance::list_ledger")]
pub async fn get_ledger(
    State(state): State<Arc<AppState>>,
    Query(params): Query<PaginationParams>,
) -> Result<impl IntoResponse, AppError> {
    let (page, per_page) = params.sanitize();
    let limit = i64::from(per_page);
    let offset = i64::from(page.saturating_sub(1)).saturating_mul(limit);

    let mut tx = state.resources.pool.begin().await.map_err(AppError::Sqlx)?;

    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM oversight_log WHERE status IS NULL OR status != 'pending'",
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(AppError::Sqlx)?;

    let rows = sqlx::query(
        r#"
        SELECT 
            o.id, o.mission_id, o.agent_id, o.entry_type, o.skill, o.params, o.status, o.decision, o.decided_at, o.decided_by, o.created_at, o.payload,
            a.action as audit_action, a.timestamp as audit_timestamp, a.params as audit_params
        FROM oversight_log o
        LEFT JOIN audit_trail a ON a.id = (
            SELECT id FROM audit_trail 
            WHERE o.skill IS NOT NULL
              AND (mission_id = o.mission_id OR (mission_id IS NULL AND o.mission_id IS NULL))
              AND agent_id = o.agent_id 
              AND (
                  action = '[SUCCESS] ' || o.skill
                  OR action = '[FAILURE] ' || o.skill
                  OR action LIKE '[SUCCESS] ' || o.skill || ' %'
                  OR action LIKE '[FAILURE] ' || o.skill || ' %'
              )
              AND datetime(timestamp) >= datetime(o.created_at)
            ORDER BY datetime(timestamp) ASC
            LIMIT 1
        )
        WHERE o.status IS NULL OR o.status != 'pending'
        ORDER BY o.created_at DESC, o.id ASC
        LIMIT ? OFFSET ?
        "#,
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(&mut *tx)
    .await
    .map_err(AppError::Sqlx)?;

    tx.commit().await.map_err(AppError::Sqlx)?;

    let mut entries = Vec::with_capacity(rows.len());
    for row in rows {
        entries.push(map_oversight_ledger_row(&row)?);
    }

    let clamped_total = u32::try_from(total.max(0)).unwrap_or(u32::MAX);
    Ok(Json(PaginatedResponse::from_parts(
        entries,
        clamped_total,
        &params,
        "/v1/oversight/ledger",
    )))
}

#[derive(Deserialize, Debug, Default, Clone)]
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

async fn persist_governance_settings(
    pool: &SqlitePool,
    auto_approve: bool,
    privacy_mode: bool,
    max_agents: u32,
    max_clusters: u32,
    max_swarm_depth: u32,
    max_task_length: usize,
    default_budget_usd: f64,
    updated_by: &str,
) -> Result<(), AppError> {
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
        "INSERT INTO governance_settings (
            id, auto_approve_safe_skills, privacy_mode, max_agents, max_clusters,
            max_swarm_depth, max_task_length, default_budget_usd, updated_by, updated_at
        ) VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, CURRENT_TIMESTAMP)
        ON CONFLICT(id) DO UPDATE SET
            auto_approve_safe_skills = excluded.auto_approve_safe_skills,
            privacy_mode = excluded.privacy_mode,
            max_agents = excluded.max_agents,
            max_clusters = excluded.max_clusters,
            max_swarm_depth = excluded.max_swarm_depth,
            max_task_length = excluded.max_task_length,
            default_budget_usd = excluded.default_budget_usd,
            updated_by = excluded.updated_by,
            updated_at = CURRENT_TIMESTAMP",
    )
    .bind(i64::from(auto_approve))
    .bind(i64::from(privacy_mode))
    .bind(i64::from(max_agents))
    .bind(i64::from(max_clusters))
    .bind(i64::from(max_swarm_depth))
    .bind(max_task_length as i64)
    .bind(default_budget_usd)
    .bind(updated_by)
    .execute(pool)
    .await
    .map_err(AppError::Sqlx)?;

    Ok(())
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

    let auto_approve = match payload.auto_approve_safe_skills {
        Some(val) => {
            state
                .governance
                .auto_approve_safe_skills
                .store(val, Ordering::SeqCst);
            val
        }
        None => state
            .governance
            .auto_approve_safe_skills
            .load(Ordering::SeqCst),
    };

    if let Some(val) = payload.privacy_mode {
        state
            .governance
            .privacy_mode
            .store(val, Ordering::SeqCst);

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

    if let Some(val) = payload.max_agents {
        state.governance.max_agents.store(val, Ordering::SeqCst);
    }
    if let Some(val) = payload.max_clusters {
        state.governance.max_clusters.store(val, Ordering::SeqCst);
    }
    if let Some(val) = payload.max_swarm_depth {
        state
            .governance
            .max_swarm_depth
            .store(val, Ordering::SeqCst);
    }
    if let Some(val) = payload.max_task_length {
        state
            .governance
            .max_task_length
            .store(val, Ordering::SeqCst);
    }
    if let Some(val) = payload.default_budget_usd {
        *state.governance.default_budget_usd.write() = val;
    }

    let current_privacy = state.governance.privacy_mode.load(Ordering::SeqCst);
    let current_max_agents = state.governance.max_agents.load(Ordering::SeqCst);
    let current_max_clusters = state.governance.max_clusters.load(Ordering::SeqCst);
    let current_max_depth = state.governance.max_swarm_depth.load(Ordering::SeqCst);
    let current_observed_depth = state.governance.observed_max_depth.load(Ordering::SeqCst);
    let current_max_task_len = state.governance.max_task_length.load(Ordering::SeqCst);
    let current_budget = *state.governance.default_budget_usd.read();

    persist_governance_settings(
        &state.resources.pool,
        auto_approve,
        current_privacy,
        current_max_agents,
        current_max_clusters,
        current_max_depth,
        current_max_task_len,
        current_budget,
        &actor,
    )
    .await?;

    let snapshot = json!({
        "auto_approve_safe_skills": auto_approve,
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
        auto_approve_safe_skills = auto_approve,
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
            "auto_approve_safe_skills": auto_approve,
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

/// Ensures a pending row exists in `oversight_log` for entries that were enqueued directly
/// into `state.comms.oversight_queue` (e.g., pre-flight security flags or test fixtures),
/// or loads the existing DB record when the in-memory queue was cleared after a restart.
async fn resolve_or_seed_pending_entry(
    state: &AppState,
    entry_id: &str,
) -> Result<OversightEntry, AppError> {
    let queue_entry = state
        .comms
        .oversight_queue
        .get(entry_id)
        .map(|entry| entry.value().clone());

    let db_row = sqlx::query(
        "SELECT id, mission_id, agent_id, skill, params, status, created_at FROM oversight_log WHERE id = ?",
    )
    .bind(entry_id)
    .fetch_optional(&state.resources.pool)
    .await
    .map_err(AppError::Sqlx)?;

    match (queue_entry, db_row) {
        (Some(entry), Some(row)) => {
            let status: String = row
                .try_get::<Option<String>, _>("status")
                .map_err(AppError::Sqlx)?
                .unwrap_or_else(|| "pending".to_string());
            if status != "pending" {
                return Err(AppError::Conflict(format!(
                    "Oversight entry '{}' is already resolved ({})",
                    entry.id, status
                )));
            }
            Ok(entry)
        }
        (Some(entry), None) => {
            let valid_mission_id = if let Some(ref mid) = entry.mission_id {
                let exists: bool = sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM mission_history WHERE id = ?)",
                )
                .bind(mid)
                .fetch_one(&state.resources.pool)
                .await
                .unwrap_or(false);
                if exists {
                    Some(mid.clone())
                } else {
                    None
                }
            } else {
                None
            };

            let agent_id = entry
                .tool_call
                .as_ref()
                .map(|tc| tc.agent_id.as_str())
                .unwrap_or("system");
            let skill = entry
                .tool_call
                .as_ref()
                .map(|tc| tc.skill.clone())
                .or_else(|| entry.skill_proposal.as_ref().map(|sp| sp.name.clone()));
            let params_json = entry
                .tool_call
                .as_ref()
                .map(|tc| {
                    crate::utils::security::redact_secrets(
                        &serde_json::to_string(&tc.params).unwrap_or_else(|_| "{}".to_string()),
                    )
                })
                .unwrap_or_else(|| "{}".to_string());

            sqlx::query(
                "INSERT INTO oversight_log (id, mission_id, agent_id, entry_type, skill, params, status, payload) \
                 VALUES (?, ?, ?, 'tool_call', ?, ?, 'pending', ?)",
            )
            .bind(&entry.id)
            .bind(&valid_mission_id)
            .bind(agent_id)
            .bind(&skill)
            .bind(&params_json)
            .bind(&params_json)
            .execute(&state.resources.pool)
            .await
            .map_err(AppError::Sqlx)?;

            Ok(entry)
        }
        (None, Some(row)) => {
            let status: String = row
                .try_get::<Option<String>, _>("status")
                .map_err(AppError::Sqlx)?
                .unwrap_or_else(|| "pending".to_string());
            if status != "pending" {
                return Err(AppError::Conflict(format!(
                    "Oversight entry '{}' is already resolved ({})",
                    entry_id, status
                )));
            }
            let id: String = row.try_get("id").map_err(AppError::Sqlx)?;
            let mission_id: Option<String> = row.try_get("mission_id").map_err(AppError::Sqlx)?;
            let created_at = row
                .try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("created_at")
                .ok()
                .flatten()
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
            Ok(OversightEntry {
                id,
                mission_id,
                tool_call: None,
                skill_proposal: None,
                status,
                created_at,
            })
        }
        (None, None) => Err(AppError::NotFound(
            "Oversight entry not found or already resolved".to_string(),
        )),
    }
}

/// POST /v1/oversight/:id/decide
///
/// Commits a human decision (Approve/Reject) for a specific oversight request.
/// Persists the decision to `oversight_log` and `audit_trail` BEFORE removing
/// the queue item or unblocking the waiting agent task.
///
/// @docs API_REFERENCE:DecideOversight
#[tracing::instrument(
    skip(principal, state, payload),
    fields(
        entry_id = %entry_id,
        operator_id = %principal.id,
        decision = tracing::field::Empty
    ),
    name = "governance::resolve_pending"
)]
pub async fn decide_oversight(
    principal: OversightPrincipal,
    Path(entry_id): Path<String>,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<OversightDecision>,
) -> Result<impl IntoResponse, AppError> {
    let validated_entry_id = validate_identifier(&entry_id, "entry_id")?;
    let (normalized_decision, approved) = parse_oversight_decision(&payload.decision)?;
    tracing::Span::current().record("decision", normalized_decision);

    // 1. Resolve entry context without removing from queue yet
    let entry = resolve_or_seed_pending_entry(&state, &validated_entry_id).await?;
    let canonical_id = entry.id.clone();

    // 2. Authorize principal against the entry's mission scope
    let actor = principal
        .authorize_mission(entry.mission_id.as_deref())?
        .to_string();

    tracing::info!(
        entry_id = %canonical_id,
        decision = %normalized_decision,
        operator = %actor,
        "[Oversight] Decision received"
    );

    let sanitized_answer = payload
        .answer
        .as_ref()
        .map(|ans| crate::utils::security::redact_secrets(ans.trim()));

    // 3. Authoritative DB update FIRST with `AND status = 'pending'` OCC guard
    let now = chrono::Utc::now();
    let affected = sqlx::query(
        "UPDATE oversight_log SET status = 'resolved', decision = ?, decided_at = ?, decided_by = ? \
         WHERE id = ? AND status = 'pending'",
    )
    .bind(normalized_decision)
    .bind(now)
    .bind(&actor)
    .bind(&canonical_id)
    .execute(&state.resources.pool)
    .await
    .map_err(AppError::Sqlx)?;

    if affected.rows_affected() == 0 {
        return Err(AppError::Conflict(format!(
            "Oversight entry '{}' is already resolved",
            canonical_id
        )));
    }

    // 4. Record in tamper-evident Merkle audit trail BEFORE unblocking the agent
    let params = serde_json::to_string(&json!({
        "entry_id": canonical_id,
        "approved": approved,
        "decided_by": actor,
        "answer": sanitized_answer
    }))
    .unwrap_or_default();

    state
        .security
        .audit_trail
        .record(
            "oversight",
            entry.mission_id.as_deref(),
            Some(&actor),
            "oversight_decision",
            &params,
        )
        .await
        .map_err(|e| {
            tracing::error!(
                entry_id = %canonical_id,
                operator = %actor,
                error = %e,
                "Failed to record oversight decision in audit trail"
            );
            AppError::InternalServerError("Failed to record oversight audit entry".to_string())
        })?;

    // 5. Store operator guidance and remove from in-memory pending queue
    if let Some(ref ans) = sanitized_answer {
        if !ans.is_empty() {
            state
                .comms
                .oversight_answers
                .insert(canonical_id.clone(), ans.clone());
        }
    }
    state.comms.oversight_queue.remove(&canonical_id);

    // 6. Resolve the waiting promise to unblock the agent
    let delivered = match state.comms.oversight_resolvers.remove(&canonical_id) {
        Some((_, shooter)) => match shooter.send(approved) {
            Ok(()) => true,
            Err(_) => {
                tracing::warn!(
                    entry_id = %canonical_id,
                    decision = %normalized_decision,
                    operator = %actor,
                    "Waiting agent dropped oversight receiver before decision delivery"
                );
                false
            }
        },
        None => false,
    };

    // 7. Broadcast and emit UI telemetry event
    let decision_label = if approved { "APPROVED" } else { "REJECTED" };
    let guidance_notice = if sanitized_answer.as_ref().is_some_and(|a| !a.is_empty()) {
        " (with operator guidance)"
    } else {
        ""
    };
    state.broadcast_sys(
        &format!(
            "⚖️ Oversight: Decision for {} recorded as {} by {}{}.",
            canonical_id, decision_label, actor, guidance_notice
        ),
        if approved { "success" } else { "warning" },
        entry.mission_id.clone(),
    );

    state.emit_event(json!({
        "type": "oversight:decision",
        "data": {
            "id": canonical_id,
            "decision": normalized_decision,
            "decided_by": actor,
            "answer": sanitized_answer,
            "delivered": delivered
        }
    }));

    Ok(Json(json!({
        "status": "ok",
        "decision": normalized_decision,
        "decided_by": actor,
        "answer": sanitized_answer,
        "delivered": delivered
    })))
}

#[derive(Deserialize, Debug)]
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

    tracing::info!(
        cluster_id = %validated_cluster_id,
        budget_usd = payload.budget_usd,
        operator = %actor,
        "[Budget] Mission quota updated"
    );
    Ok((StatusCode::OK, Json(json!({ "status": "ok" }))))
}

/// GET /v1/oversight/security/audit-trail
///
/// Retrieves the tamper-evident Merkle hash-chain logs and cryptographically
/// verifies each entry's SHA-256 digest, digital signature, and predecessor linkage.
///
/// @docs API_REFERENCE:GetAuditTrail
#[tracing::instrument(skip(state), name = "governance::get_audit_trail")]
pub async fn get_audit_trail(
    State(state): State<Arc<AppState>>,
    Query(params): Query<PaginationParams>,
) -> Result<impl IntoResponse, AppError> {
    let (page, per_page) = params.sanitize();
    let limit = i64::from(per_page);
    let offset = i64::from(page.saturating_sub(1)).saturating_mul(limit);

    let mut tx = state.resources.pool.begin().await.map_err(AppError::Sqlx)?;

    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audit_trail")
        .fetch_one(&mut *tx)
        .await
        .map_err(AppError::Sqlx)?;

    // Fetch `limit + 1` rows so the oldest entry on the page can verify its `prev_hash`
    // against its immediate chronological predecessor.
    let entries: Vec<AuditEntry> = sqlx::query_as(
        "SELECT id, timestamp, agent_id, mission_id, user_id, action, params, prev_hash, current_hash, signature \
         FROM audit_trail ORDER BY timestamp DESC, rowid DESC LIMIT ? OFFSET ?",
    )
    .bind(limit.saturating_add(1))
    .bind(offset)
    .fetch_all(&mut *tx)
    .await
    .map_err(AppError::Sqlx)?;

    tx.commit().await.map_err(AppError::Sqlx)?;

    let page_len = entries.len().min(per_page as usize);
    let genesis_hash = "0".repeat(64);
    let mut response = Vec::with_capacity(page_len);

    for idx in 0..page_len {
        let entry = &entries[idx];
        let intrinsic_ok = state.security.audit_trail.verify_record(entry);
        let link_ok = if idx + 1 < entries.len() {
            entry.prev_hash == entries[idx + 1].current_hash
        } else {
            entry.prev_hash == genesis_hash
        };
        let is_verified = intrinsic_ok && link_ok;

        let decision = if entry.action == "oversight_decision" {
            serde_json::from_str::<serde_json::Value>(&entry.params)
                .ok()
                .and_then(|v| {
                    v.get("approved")
                        .and_then(|b| b.as_bool())
                        .map(|approved| {
                            if approved {
                                "approved".to_string()
                            } else {
                                "rejected".to_string()
                            }
                        })
                })
        } else {
            None
        };

        response.push(OversightAuditEntry {
            id: entry.id.clone(),
            agent_id: entry.agent_id.clone(),
            action: entry.action.clone(),
            skill: Some(entry.action.clone()),
            status: if is_verified {
                "verified".to_string()
            } else {
                "tampered".to_string()
            },
            decision,
            decided_at: if entry.action == "oversight_decision" {
                Some(entry.timestamp)
            } else {
                None
            },
            created_at: entry.timestamp,
            prev_hash: Some(entry.prev_hash.clone()),
            current_hash: Some(entry.current_hash.clone()),
            signature: entry.signature.clone(),
            is_verified,
        });
    }

    let clamped_total = u32::try_from(total.max(0)).unwrap_or(u32::MAX);
    Ok(Json(PaginatedResponse::from_parts(
        response,
        clamped_total,
        &params,
        "/v1/oversight/security/audit-trail",
    )))
}

/// GET /v1/oversight/security/integrity
///
/// Verifies the last `INTEGRITY_VERIFY_WINDOW` records in the Merkle chain and
/// returns an explicit integrity status (`VERIFIED`, `UNSIGNED_CHAIN`, `TAMPERED`,
/// `EMPTY`, or `UNAVAILABLE`). Never conflates backend errors or empty chains with tampering.
///
/// @docs API_REFERENCE:GetIntegrityStatus
#[tracing::instrument(skip(state), name = "governance::get_integrity")]
pub async fn get_integrity_status(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    let checked_at = chrono::Utc::now();
    let is_signing_configured = state.security.audit_trail.is_signing_enabled();

    match state
        .security
        .audit_trail
        .verify_last_n(INTEGRITY_VERIFY_WINDOW, None)
        .await
    {
        Err(e) => {
            tracing::warn!(error = %e, "Audit chain verification unavailable");
            Ok(Json(SecurityIntegrityResponse {
                integrity_score: None,
                status: "UNAVAILABLE".to_string(),
                verified_count: 0,
                total_count: 0,
                verified_window: INTEGRITY_VERIFY_WINDOW,
                chain_head: None,
                checked_at,
                error: Some(crate::utils::security::redact_secrets(&e.to_string())),
            }))
        }
        Ok((0, 0)) => Ok(Json(SecurityIntegrityResponse {
            integrity_score: None,
            status: "EMPTY".to_string(),
            verified_count: 0,
            total_count: 0,
            verified_window: INTEGRITY_VERIFY_WINDOW,
            chain_head: None,
            checked_at,
            error: None,
        })),
        Ok((verified_count, total_count)) => {
            let chain_head = state
                .security
                .audit_trail
                .get_latest_entry()
                .await
                .ok()
                .flatten()
                .map(|e| e.current_hash);

            let base_score = (verified_count as f64 / total_count as f64).min(1.0);
            let score = if is_signing_configured {
                base_score
            } else {
                base_score * UNSIGNED_CHAIN_SCORE_MULTIPLIER
            };

            let status = if verified_count < total_count {
                "TAMPERED".to_string()
            } else if is_signing_configured {
                "VERIFIED".to_string()
            } else {
                "UNSIGNED_CHAIN".to_string()
            };

            Ok(Json(SecurityIntegrityResponse {
                integrity_score: Some(score),
                status,
                verified_count,
                total_count,
                verified_window: INTEGRITY_VERIFY_WINDOW,
                chain_head,
                checked_at,
                error: None,
            }))
        }
    }
}

/// GET /v1/oversight/security/health
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
            let is_suspended = agent.health.status.eq_ignore_ascii_case("suspended");
            let is_bankrupt = if agent.economics.budget_usd <= 0.0 {
                agent.economics.cost_usd > 0.0
            } else {
                agent.economics.cost_usd >= agent.economics.budget_usd
            };
            json!({
                "agent_id": agent.identity.id,
                "name": agent.identity.name,
                "status": agent.health.status,
                "failure_count": agent.health.failure_count,
                "last_failure_at": agent.health.last_failure_at,
                "is_healthy": agent.health.failure_count < HEALTHY_FAILURE_LIMIT && !is_suspended,
                "is_suspended": is_suspended,
                "is_throttled": agent.health.failure_count >= THROTTLED_FAILURE_THRESHOLD,
                "is_bankrupt": is_bankrupt,
            })
        })
        .collect();

    Ok(Json(json!({ "agents": health_data })))
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum PolicyMode {
    Allow,
    Deny,
    Prompt,
}

#[derive(Deserialize, Debug)]
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

#[derive(Serialize, Type, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TokenBurnReport {
    pub source: &'static str,
    pub total_tokens_in: i64,
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
    pub tokens_in: i64,
    pub tokens_out: i64,
    pub cost_usd: f64,
}

#[derive(Serialize, Type, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AgentTokenBurn {
    pub agent_id: String,
    pub agent_name: String,
    pub tokens_in: i64,
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
    pub tokens_in: i64,
    pub tokens_out: i64,
    pub cost_usd: f64,
    pub updated_at: i64,
}

fn decode_timestamp_epoch(row: &sqlx::sqlite::SqliteRow, col: &str) -> i64 {
    if let Ok(Some(epoch)) = row.try_get::<Option<i64>, _>(col) {
        return epoch;
    }
    if let Ok(Some(dt)) = row.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>(col) {
        return dt.timestamp();
    }
    if let Ok(Some(raw)) = row.try_get::<Option<String>, _>(col) {
        let trimmed = raw.trim();
        if let Ok(epoch) = trimmed.parse::<i64>() {
            return epoch;
        }
        if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(trimmed) {
            return dt.timestamp();
        }
        if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%d %H:%M:%S") {
            return naive.and_utc().timestamp();
        }
    }
    0
}

/// Determines the single authoritative token-burn source (`"agent_tasks"` when tasks have
/// recorded usage/cost, falling back to `"agents"` otherwise) so totals and breakdowns reconcile.
async fn query_overall_token_totals(
    pool: &SqlitePool,
) -> Result<(&'static str, i64, i64, f64), AppError> {
    let totals_row = sqlx::query(
        "SELECT 
            COALESCE(SUM(tokens_in), 0) as total_tokens_in,
            COALESCE(SUM(tokens_out), 0) as total_tokens_out,
            COALESCE(SUM(cost_usd), 0.0) as total_cost_usd
         FROM agent_tasks",
    )
    .fetch_one(pool)
    .await
    .map_err(AppError::Sqlx)?;

    let total_tokens_in: i64 = totals_row
        .try_get("total_tokens_in")
        .map_err(AppError::Sqlx)?;
    let total_tokens_out: i64 = totals_row
        .try_get("total_tokens_out")
        .map_err(AppError::Sqlx)?;
    let total_cost_usd: f64 = totals_row
        .try_get("total_cost_usd")
        .map_err(AppError::Sqlx)?;

    if total_tokens_in > 0 || total_tokens_out > 0 || total_cost_usd > 0.0 {
        return Ok((
            "agent_tasks",
            total_tokens_in,
            total_tokens_out,
            total_cost_usd,
        ));
    }

    let agent_totals_row = sqlx::query(
        "SELECT 
            COALESCE(SUM(input_tokens), 0) as total_tokens_in,
            COALESCE(SUM(output_tokens), 0) as total_tokens_out,
            COALESCE(SUM(cost_usd), 0.0) as total_cost_usd
         FROM agents",
    )
    .fetch_one(pool)
    .await
    .map_err(AppError::Sqlx)?;

    let fallback_in: i64 = agent_totals_row
        .try_get("total_tokens_in")
        .map_err(AppError::Sqlx)?;
    let fallback_out: i64 = agent_totals_row
        .try_get("total_tokens_out")
        .map_err(AppError::Sqlx)?;
    let fallback_cost: f64 = agent_totals_row
        .try_get("total_cost_usd")
        .map_err(AppError::Sqlx)?;

    Ok(("agents", fallback_in, fallback_out, fallback_cost))
}

/// Queries provider-grouped token burn metrics from the same authoritative `source` table as totals.
async fn query_provider_token_burns(
    pool: &SqlitePool,
    source: &'static str,
) -> Result<Vec<ProviderTokenBurn>, AppError> {
    let query_sql = if source == "agent_tasks" {
        "SELECT 
            COALESCE(provider_id, 'unknown') as provider_id,
            COALESCE(SUM(tokens_in), 0) as tokens_in,
            COALESCE(SUM(tokens_out), 0) as tokens_out,
            COALESCE(SUM(cost_usd), 0.0) as cost_usd
         FROM agent_tasks
         WHERE COALESCE(tokens_in, 0) > 0 OR COALESCE(tokens_out, 0) > 0 OR COALESCE(cost_usd, 0.0) > 0.0
         GROUP BY provider_id"
    } else {
        "SELECT 
            COALESCE(provider, 'unknown') as provider_id,
            COALESCE(SUM(input_tokens), 0) as tokens_in,
            COALESCE(SUM(output_tokens), 0) as tokens_out,
            COALESCE(SUM(cost_usd), 0.0) as cost_usd
         FROM agents
         WHERE COALESCE(input_tokens, 0) > 0 OR COALESCE(output_tokens, 0) > 0 OR COALESCE(cost_usd, 0.0) > 0.0
         GROUP BY provider"
    };

    let rows = sqlx::query(query_sql)
        .fetch_all(pool)
        .await
        .map_err(AppError::Sqlx)?;

    let mut providers = Vec::with_capacity(rows.len());
    for row in rows {
        providers.push(ProviderTokenBurn {
            provider_id: row.try_get("provider_id").map_err(AppError::Sqlx)?,
            tokens_in: row.try_get("tokens_in").map_err(AppError::Sqlx)?,
            tokens_out: row.try_get("tokens_out").map_err(AppError::Sqlx)?,
            cost_usd: row.try_get("cost_usd").map_err(AppError::Sqlx)?,
        });
    }

    Ok(providers)
}

/// Queries per-agent token consumption from the same authoritative `source` table so parts sum to the total.
async fn query_agent_token_burns(
    pool: &SqlitePool,
    source: &'static str,
) -> Result<Vec<AgentTokenBurn>, AppError> {
    let query_sql = if source == "agent_tasks" {
        "SELECT 
            t.agent_id as agent_id,
            COALESCE(a.name, 'Unknown Agent') as agent_name,
            COALESCE(SUM(t.tokens_in), 0) as tokens_in,
            COALESCE(SUM(t.tokens_out), 0) as tokens_out,
            COALESCE(SUM(t.cost_usd), 0.0) as cost_usd
         FROM agent_tasks t
         LEFT JOIN agents a ON t.agent_id = a.id
         WHERE COALESCE(t.tokens_in, 0) > 0 OR COALESCE(t.tokens_out, 0) > 0 OR COALESCE(t.cost_usd, 0.0) > 0.0
         GROUP BY t.agent_id, a.name"
    } else {
        "SELECT 
            id as agent_id,
            name as agent_name,
            COALESCE(input_tokens, 0) as tokens_in,
            COALESCE(output_tokens, 0) as tokens_out,
            COALESCE(cost_usd, 0.0) as cost_usd
         FROM agents
         WHERE COALESCE(input_tokens, 0) > 0 OR COALESCE(output_tokens, 0) > 0 OR COALESCE(cost_usd, 0.0) > 0.0"
    };

    let rows = sqlx::query(query_sql)
        .fetch_all(pool)
        .await
        .map_err(AppError::Sqlx)?;

    let mut agents = Vec::with_capacity(rows.len());
    for row in rows {
        agents.push(AgentTokenBurn {
            agent_id: row.try_get("agent_id").map_err(AppError::Sqlx)?,
            agent_name: row.try_get("agent_name").map_err(AppError::Sqlx)?,
            tokens_in: row.try_get("tokens_in").map_err(AppError::Sqlx)?,
            tokens_out: row.try_get("tokens_out").map_err(AppError::Sqlx)?,
            cost_usd: row.try_get("cost_usd").map_err(AppError::Sqlx)?,
        });
    }

    Ok(agents)
}

/// Queries the 20 most recent tasks with non-zero token or cost burn.
async fn query_recent_token_burns(pool: &SqlitePool) -> Result<Vec<RecentTokenBurn>, AppError> {
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
         LIMIT 20",
    )
    .fetch_all(pool)
    .await
    .map_err(AppError::Sqlx)?;

    let mut recent = Vec::with_capacity(recent_rows.len());
    for row in recent_rows {
        recent.push(RecentTokenBurn {
            task_id: row.try_get("task_id").map_err(AppError::Sqlx)?,
            title: row.try_get("title").map_err(AppError::Sqlx)?,
            agent_id: row.try_get("agent_id").map_err(AppError::Sqlx)?,
            agent_name: row.try_get("agent_name").map_err(AppError::Sqlx)?,
            tokens_in: row.try_get("tokens_in").map_err(AppError::Sqlx)?,
            tokens_out: row.try_get("tokens_out").map_err(AppError::Sqlx)?,
            cost_usd: row.try_get("cost_usd").map_err(AppError::Sqlx)?,
            updated_at: decode_timestamp_epoch(&row, "updated_at"),
        });
    }

    Ok(recent)
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
    let pool = &state.resources.pool;
    let (source, total_tokens_in, total_tokens_out, total_cost_usd) =
        query_overall_token_totals(pool).await?;

    // Update Prometheus gauges
    crate::telemetry::OVERSIGHT_TOKENS_IN.set(total_tokens_in as f64);
    crate::telemetry::OVERSIGHT_TOKENS_OUT.set(total_tokens_out as f64);
    crate::telemetry::OVERSIGHT_COST_USD.set(total_cost_usd);

    let (providers, agents, recent_burns) = tokio::try_join!(
        query_provider_token_burns(pool, source),
        query_agent_token_burns(pool, source),
        query_recent_token_burns(pool),
    )?;

    Ok(Json(TokenBurnReport {
        source,
        total_tokens_in,
        total_tokens_out,
        total_cost_usd,
        providers,
        agents,
        recent_burns,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::metering::BudgetGuard;
    use axum::body::to_bytes;

    async fn response_json(resp: impl IntoResponse) -> serde_json::Value {
        let response = resp.into_response();
        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn test_update_agent_quota_rejects_negative_nan_and_unauthenticated() {
        let state = Arc::new(AppState::new_minimal_mock().await);
        let principal = OversightPrincipal::global("operator-1");

        // 0. Empty principal rejected with Unauthorized
        let res_unauth = update_agent_quota(
            OversightPrincipal::global("   "),
            Path("agent-1".to_string()),
            State(state.clone()),
            Json(UpdateQuotaPayload {
                budget_usd: 10.0,
                reset_period: None,
            }),
        )
        .await;
        assert!(matches!(res_unauth, Err(AppError::Unauthorized(_))));

        // 1. Negative budget
        let res_neg = update_agent_quota(
            principal.clone(),
            Path("agent-1".to_string()),
            State(state.clone()),
            Json(UpdateQuotaPayload {
                budget_usd: -50.0,
                reset_period: None,
            }),
        )
        .await;
        assert!(matches!(res_neg, Err(AppError::BadRequest(_))));

        // 2. NaN budget
        let res_nan = update_agent_quota(
            principal.clone(),
            Path("agent-1".to_string()),
            State(state.clone()),
            Json(UpdateQuotaPayload {
                budget_usd: f64::NAN,
                reset_period: None,
            }),
        )
        .await;
        assert!(matches!(res_nan, Err(AppError::BadRequest(_))));

        // 3. Infinite budget
        let res_inf = update_agent_quota(
            principal,
            Path("agent-1".to_string()),
            State(state.clone()),
            Json(UpdateQuotaPayload {
                budget_usd: f64::INFINITY,
                reset_period: None,
            }),
        )
        .await;
        assert!(matches!(res_inf, Err(AppError::BadRequest(_))));
    }

    #[test]
    fn test_oversight_decision_and_settings_validation_three_paths() {
        // (a) Happy Path: valid decisions & governance bounds
        assert_eq!(
            parse_oversight_decision("approved").unwrap(),
            ("approved", true)
        );
        assert_eq!(
            parse_oversight_decision("rejected").unwrap(),
            ("rejected", false)
        );
        let valid_settings = OversightSettingsPayload {
            auto_approve_safe_skills: Some(true),
            privacy_mode: Some(false),
            max_agents: Some(50),
            max_clusters: Some(10),
            max_swarm_depth: Some(5),
            max_task_length: Some(4096),
            default_budget_usd: Some(5.0),
        };
        assert!(validate_oversight_settings(&valid_settings).is_ok());

        // (b) Failure Path: invalid decision token, NaN budget, or exceeding ceiling limits
        assert!(matches!(
            parse_oversight_decision("maybe"),
            Err(AppError::BadRequest(_))
        ));
        let invalid_budget = OversightSettingsPayload {
            auto_approve_safe_skills: Some(false),
            privacy_mode: None,
            max_agents: None,
            max_clusters: None,
            max_swarm_depth: None,
            max_task_length: None,
            default_budget_usd: Some(f64::NAN),
        };
        assert!(matches!(
            validate_oversight_settings(&invalid_budget),
            Err(AppError::BadRequest(_))
        ));

        let above_ceiling = OversightSettingsPayload {
            auto_approve_safe_skills: None,
            privacy_mode: None,
            max_agents: Some(MAX_AGENTS_CEILING + 1),
            max_clusters: None,
            max_swarm_depth: None,
            max_task_length: None,
            default_budget_usd: None,
        };
        assert!(matches!(
            validate_oversight_settings(&above_ceiling),
            Err(AppError::BadRequest(_))
        ));

        // (c) Edge Case Path: case/whitespace-padded decision token and zero-valued governance limits
        assert_eq!(
            parse_oversight_decision("  APPROVED \n").unwrap(),
            ("approved", true)
        );
        let zero_depth = OversightSettingsPayload {
            auto_approve_safe_skills: None,
            privacy_mode: None,
            max_agents: None,
            max_clusters: None,
            max_swarm_depth: Some(0),
            max_task_length: None,
            default_budget_usd: Some(0.0),
        };
        assert!(matches!(
            validate_oversight_settings(&zero_depth),
            Err(AppError::BadRequest(_))
        ));
    }

    #[tokio::test]
    async fn test_decide_oversight_end_to_end_attribution_and_idempotency() {
        let state = Arc::new(AppState::new_minimal_mock().await);
        let entry_id = "entry-e2e-1".to_string();
        let (tx, rx) = tokio::sync::oneshot::channel();

        state.comms.oversight_resolvers.insert(entry_id.clone(), tx);
        state.comms.oversight_queue.insert(
            entry_id.clone(),
            OversightEntry {
                id: entry_id.clone(),
                mission_id: None,
                tool_call: Some(ToolCallAudit {
                    id: "tc-1".to_string(),
                    agent_id: "agent-alpha".to_string(),
                    mission_id: None,
                    skill: "write_file".to_string(),
                    params: json!({"path": "README.md"}),
                    department: "engineering".to_string(),
                    description: "Write README.md".to_string(),
                    timestamp: chrono::Utc::now().to_rfc3339(),
                }),
                skill_proposal: None,
                status: "pending".to_string(),
                created_at: chrono::Utc::now().to_rfc3339(),
            },
        );

        let sample_secret = format!("{}-{}", "sk", "1234567890abcdef1234567890abcdef");
        let resp = decide_oversight(
            OversightPrincipal::global("operator-alice"),
            Path(entry_id.clone()),
            State(state.clone()),
            Json(OversightDecision {
                decision: "approved".to_string(),
                answer: Some(format!("Proceed with key {}", sample_secret)),
            }),
        )
        .await
        .expect("decide_oversight should succeed");

        let body = response_json(resp).await;
        assert_eq!(body["status"], "ok");
        assert_eq!(body["decision"], "approved");
        assert_eq!(body["decided_by"], "operator-alice");
        assert_eq!(body["delivered"], true);
        assert!(body["answer"].as_str().unwrap().contains("[REDACTED]"));
        assert!(!body["answer"].as_str().unwrap().contains(&sample_secret));

        // Resolver received `true`
        assert!(rx.await.unwrap());

        // DB row updated with operator attribution
        let row = sqlx::query("SELECT status, decision, decided_by FROM oversight_log WHERE id = ?")
            .bind(&entry_id)
            .fetch_one(&state.resources.pool)
            .await
            .unwrap();
        assert_eq!(row.try_get::<String, _>("status").unwrap(), "resolved");
        assert_eq!(row.try_get::<String, _>("decision").unwrap(), "approved");
        assert_eq!(
            row.try_get::<String, _>("decided_by").unwrap(),
            "operator-alice"
        );

        // Audit trail contains `oversight_decision` attributed to `operator-alice`
        let audit_row = sqlx::query(
            "SELECT user_id, action, params FROM audit_trail WHERE action = 'oversight_decision' LIMIT 1",
        )
        .fetch_one(&state.resources.pool)
        .await
        .unwrap();
        assert_eq!(
            audit_row.try_get::<Option<String>, _>("user_id").unwrap().as_deref(),
            Some("operator-alice")
        );

        // Second decision on the same ID returns 409 Conflict
        let second = decide_oversight(
            OversightPrincipal::global("operator-alice"),
            Path(entry_id),
            State(state),
            Json(OversightDecision {
                decision: "rejected".to_string(),
                answer: None,
            }),
        )
        .await;
        assert!(matches!(second, Err(AppError::Conflict(_))));
    }

    #[tokio::test]
    async fn test_decide_oversight_db_failure_preserves_queue_and_does_not_unblock() {
        let state = Arc::new(AppState::new_minimal_mock().await);
        let entry_id = "entry-db-fail".to_string();
        let (tx, mut rx) = tokio::sync::oneshot::channel();

        sqlx::query(
            "INSERT INTO oversight_log (id, agent_id, entry_type, skill, params, status) \
             VALUES (?, 'agent-1', 'tool_call', 'bash', '{}', 'pending')",
        )
        .bind(&entry_id)
        .execute(&state.resources.pool)
        .await
        .unwrap();

        state.comms.oversight_resolvers.insert(entry_id.clone(), tx);
        state.comms.oversight_queue.insert(
            entry_id.clone(),
            OversightEntry {
                id: entry_id.clone(),
                mission_id: None,
                tool_call: None,
                skill_proposal: None,
                status: "pending".to_string(),
                created_at: chrono::Utc::now().to_rfc3339(),
            },
        );

        // Inject DB failure on oversight_log update
        sqlx::query(
            "CREATE TRIGGER abort_oversight_update BEFORE UPDATE ON oversight_log \
             BEGIN SELECT RAISE(ABORT, 'injected oversight_log failure'); END",
        )
        .execute(&state.resources.pool)
        .await
        .unwrap();

        let res = decide_oversight(
            OversightPrincipal::global("operator-1"),
            Path(entry_id.clone()),
            State(state.clone()),
            Json(OversightDecision {
                decision: "approved".to_string(),
                answer: None,
            }),
        )
        .await;

        assert!(res.is_err(), "DB update failure must not return 200 OK");
        assert!(
            state.comms.oversight_queue.contains_key(&entry_id),
            "Entry must remain in queue when DB persistence fails"
        );
        assert!(
            rx.try_recv().is_err(),
            "Waiting agent must not be unblocked when DB persistence fails"
        );
    }

    #[tokio::test]
    async fn test_decide_oversight_mission_scoping_authorization() {
        let state = Arc::new(AppState::new_minimal_mock().await);
        let entry_id = "entry-scoped-1".to_string();

        state.comms.oversight_queue.insert(
            entry_id.clone(),
            OversightEntry {
                id: entry_id.clone(),
                mission_id: Some("mission-alpha".to_string()),
                tool_call: None,
                skill_proposal: None,
                status: "pending".to_string(),
                created_at: chrono::Utc::now().to_rfc3339(),
            },
        );

        // Principal scoped to mission-beta cannot resolve mission-alpha's entry
        let forbidden = decide_oversight(
            OversightPrincipal::scoped("op-beta", ["mission-beta"]),
            Path(entry_id.clone()),
            State(state.clone()),
            Json(OversightDecision {
                decision: "approved".to_string(),
                answer: None,
            }),
        )
        .await;
        assert!(matches!(forbidden, Err(AppError::Forbidden(_))));
        assert!(state.comms.oversight_queue.contains_key(&entry_id));

        // Principal scoped to mission-alpha succeeds
        let allowed = decide_oversight(
            OversightPrincipal::scoped("op-alpha", ["mission-alpha"]),
            Path(entry_id),
            State(state),
            Json(OversightDecision {
                decision: "approved".to_string(),
                answer: None,
            }),
        )
        .await;
        assert!(allowed.is_ok());
    }

    #[tokio::test]
    async fn test_update_policy_global_upsert_deduplicates_null_agent_id() {
        let state = Arc::new(AppState::new_minimal_mock().await);
        let principal = OversightPrincipal::global("sec-admin");

        update_policy(
            principal.clone(),
            State(state.clone()),
            Json(UpdatePolicyPayload {
                tool_name: "custom_deploy_tool".to_string(),
                agent_id: None,
                mode: PolicyMode::Allow,
            }),
        )
        .await
        .unwrap();

        update_policy(
            principal,
            State(state.clone()),
            Json(UpdatePolicyPayload {
                tool_name: "custom_deploy_tool".to_string(),
                agent_id: None,
                mode: PolicyMode::Deny,
            }),
        )
        .await
        .unwrap();

        let (count, mode): (i64, String) = sqlx::query_as(
            "SELECT COUNT(*), COALESCE(MAX(mode), '') FROM permission_policies WHERE tool_name = 'custom_deploy_tool'",
        )
        .fetch_one(&state.resources.pool)
        .await
        .unwrap();
        assert_eq!(count, 1, "Repeated global policy updates must not insert duplicate NULL rows");
        assert_eq!(mode, "deny");

        let policies_json = response_json(get_policies(State(state)).await.unwrap()).await;
        let matching: Vec<_> = policies_json
            .as_array()
            .unwrap()
            .iter()
            .filter(|p| p["tool_name"] == "custom_deploy_tool")
            .collect();
        assert_eq!(matching.len(), 1);
        assert!(matching[0].get("agent_id").is_some());
    }

    #[tokio::test]
    async fn test_get_integrity_status_and_audit_trail_verification() {
        let state = Arc::new(AppState::new_minimal_mock().await);

        // 1. Empty chain -> EMPTY (not SECURE, score None)
        let empty_json =
            response_json(get_integrity_status(State(state.clone())).await.unwrap()).await;
        assert_eq!(empty_json["status"], "EMPTY");
        assert!(empty_json["integrity_score"].is_null());

        // 2. Record two valid entries -> UNSIGNED_CHAIN (or VERIFIED), is_verified == true
        let e1 = state
            .security
            .audit_trail
            .record("agent-1", None, Some("op-1"), "step_one", "{}")
            .await
            .unwrap();
        let _e2 = state
            .security
            .audit_trail
            .record("agent-1", None, Some("op-1"), "step_two", "{}")
            .await
            .unwrap();

        let valid_integrity =
            response_json(get_integrity_status(State(state.clone())).await.unwrap()).await;
        assert_eq!(valid_integrity["status"], "UNSIGNED_CHAIN");
        assert_eq!(valid_integrity["verified_count"], 2);
        assert_eq!(valid_integrity["total_count"], 2);
        assert!(valid_integrity["chain_head"].is_string());

        let trail_ok = response_json(
            get_audit_trail(
                State(state.clone()),
                Query(PaginationParams {
                    page: 1,
                    per_page: 10,
                }),
            )
            .await
            .unwrap(),
        )
        .await;
        let rows_ok = trail_ok["data"].as_array().unwrap();
        assert_eq!(rows_ok.len(), 2);
        assert!(rows_ok.iter().all(|r| r["is_verified"] == true));
        assert!(rows_ok.iter().all(|r| r["prev_hash"].is_string() && r["current_hash"].is_string()));

        // 3. Tamper with entry 1 -> get_audit_trail reports is_verified == false & get_integrity_status reports TAMPERED
        sqlx::query("UPDATE audit_trail SET action = 'forged_action' WHERE id = ?")
            .bind(&e1.id)
            .execute(&state.resources.pool)
            .await
            .unwrap();

        let trail_tampered = response_json(
            get_audit_trail(
                State(state.clone()),
                Query(PaginationParams {
                    page: 1,
                    per_page: 10,
                }),
            )
            .await
            .unwrap(),
        )
        .await;
        let tampered_entry = trail_tampered["data"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == e1.id)
            .unwrap();
        assert_eq!(tampered_entry["is_verified"], false);
        assert_eq!(tampered_entry["status"], "tampered");

        let tampered_integrity =
            response_json(get_integrity_status(State(state.clone())).await.unwrap()).await;
        assert_eq!(tampered_integrity["status"], "TAMPERED");

        // 4. Verification error (table dropped) -> UNAVAILABLE (not TAMPERED)
        sqlx::query("DROP TABLE audit_trail")
            .execute(&state.resources.pool)
            .await
            .unwrap();
        let unavail_integrity =
            response_json(get_integrity_status(State(state)).await.unwrap()).await;
        assert_eq!(unavail_integrity["status"], "UNAVAILABLE");
        assert!(unavail_integrity["integrity_score"].is_null());
        assert!(unavail_integrity["error"].is_string());
    }

    #[tokio::test]
    async fn test_mission_quota_round_trip_and_null_reset_period_handling() {
        let state = Arc::new(AppState::new_minimal_mock().await);
        let principal = OversightPrincipal::global("op-quota");

        update_mission_quota(
            principal,
            Path("cluster-alpha".to_string()),
            State(state.clone()),
            Json(UpdateQuotaPayload {
                budget_usd: 42.5,
                reset_period: Some(ResetPeriod::Monthly),
            }),
        )
        .await
        .unwrap();

        let quotas_json = response_json(get_mission_quotas(State(state)).await.unwrap()).await;
        let quotas = quotas_json.as_array().unwrap();
        let found = quotas
            .iter()
            .find(|q| q["entity_id"] == "cluster-alpha")
            .expect("Updated mission quota must be returned by GET");
        assert_eq!(found["budget_usd"], 42.5);
        assert_eq!(found["reset_period"], "monthly");

        // Verify NULL reset_period in SQLite does not panic
        assert_eq!(BudgetGuard::parse_reset_period(None), ResetPeriod::Never);
    }

    #[tokio::test]
    async fn test_ledger_redaction_malformed_json_and_negative_duration_anomaly() {
        let state = Arc::new(AppState::new_minimal_mock().await);
        let sample_secret = format!("{}-{}", "sk", "1234567890abcdef1234567890abcdef");
        let decided_at = chrono::Utc::now();
        let predating_audit_ts = decided_at - chrono::Duration::seconds(5);

        // Row 1: secret in params + audit entry predating decided_at (timestamp anomaly)
        sqlx::query(
            "INSERT INTO oversight_log (id, mission_id, agent_id, entry_type, skill, params, status, decision, decided_at, decided_by, created_at, payload) \
             VALUES ('led-1', NULL, 'agent-1', 'tool_call', 'deploy', ?1, 'resolved', 'approved', ?2, 'op-1', ?3, ?1)",
        )
        .bind(format!(r#"{{"api_key":"{}"}}"#, sample_secret))
        .bind(decided_at)
        .bind(decided_at - chrono::Duration::seconds(10))
        .execute(&state.resources.pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO audit_trail (id, agent_id, mission_id, user_id, action, params, prev_hash, current_hash, timestamp) \
             VALUES ('aud-1', 'agent-1', NULL, 'op-1', '[SUCCESS] deploy', 'deployed ok', ?1, ?2, ?3)",
        )
        .bind("0".repeat(64))
        .bind("1".repeat(64))
        .bind(predating_audit_ts)
        .execute(&state.resources.pool)
        .await
        .unwrap();

        // Row 2: malformed JSON in params and NULL skill
        sqlx::query(
            "INSERT INTO oversight_log (id, mission_id, agent_id, entry_type, skill, params, status, decision, decided_at, decided_by, created_at, payload) \
             VALUES ('led-2', NULL, 'agent-1', 'tool_call', NULL, '{malformed_json', 'resolved', 'rejected', ?1, 'op-1', ?1, '')",
        )
        .bind(decided_at)
        .execute(&state.resources.pool)
        .await
        .unwrap();

        let ledger_json = response_json(
            get_ledger(
                State(state),
                Query(PaginationParams {
                    page: 1,
                    per_page: 25,
                }),
            )
            .await
            .unwrap(),
        )
        .await;
        let items = ledger_json["data"].as_array().unwrap();

        let item1 = items.iter().find(|i| i["id"] == "led-1").unwrap();
        assert_eq!(item1["tool_call"]["params"]["api_key"], "[REDACTED]");
        assert_eq!(item1["payload"]["api_key"], "[REDACTED]");
        assert_eq!(item1["result"]["timestamp_anomaly"], true);
        assert!(item1["result"]["duration_ms"].as_i64().unwrap() < 0);

        let item2 = items.iter().find(|i| i["id"] == "led-2").unwrap();
        assert!(
            item2["tool_call"]["params"]["_parse_error"].is_string(),
            "Malformed JSON must emit _parse_error rather than silent null"
        );
    }

    #[tokio::test]
    async fn test_partial_update_settings_preserves_auto_approve_safe_skills() {
        let state = Arc::new(AppState::new_minimal_mock().await);
        state
            .governance
            .auto_approve_safe_skills
            .store(true, Ordering::SeqCst);

        let resp = update_settings(
            OversightPrincipal::global("admin-1"),
            State(state.clone()),
            Json(OversightSettingsPayload {
                auto_approve_safe_skills: None,
                privacy_mode: None,
                max_agents: Some(25),
                max_clusters: None,
                max_swarm_depth: None,
                max_task_length: None,
                default_budget_usd: None,
            }),
        )
        .await
        .unwrap();

        let body = response_json(resp).await;
        assert_eq!(body["auto_approve_safe_skills"], true);
        assert_eq!(body["max_agents"], 25);
    }
}

// Metadata: [oversight]

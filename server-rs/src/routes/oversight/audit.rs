//! @docs ARCHITECTURE:Networking
//!
//! ### AI Assist Note
//! **Audit & Integrity Subsystem**: Queries historical oversight decisions, Merkle hash-chain ledgers,
//! digital signature status, and cryptographic tamper detection metrics.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Cryptographic chain tamper detected, database ledger read error, or signature mismatch.
//! - **Telemetry Link**: Search for `[audit]` in tracing logs.
//! - **Trace Scope**: `server-rs::routes::oversight::audit`

use axum::{
    extract::{Query, State},
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;
use sqlx::Row;
use std::sync::Arc;

use crate::error::AppError;
use crate::routes::oversight::principal::{
    redact_json_value, INTEGRITY_VERIFY_WINDOW, UNSIGNED_CHAIN_SCORE_MULTIPLIER,
};
use crate::routes::pagination::{PaginatedResponse, PaginationParams};
use crate::security::audit::AuditEntry;
use crate::state::AppState;

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

pub fn parse_and_redact_ledger_json(
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
pub fn map_oversight_ledger_row(row: &sqlx::sqlite::SqliteRow) -> Result<serde_json::Value, AppError> {
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

// Metadata: [audit]



//! @docs ARCHITECTURE:Networking
//!
//! ### AI Assist Note
//! **Oversight Queue Subsystem**: Orchestrates pending approvals, reconciles in-memory queue
//! with durable database records, and executes the multi-stage decision saga with Merkle audit trail recording.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Decision race conflict, agent unblocking failure, or unauthorized decision payload.
//! - **Telemetry Link**: Search for `[queue]` in tracing logs.
//! - **Trace Scope**: `server-rs::routes::oversight::queue`

use axum::{
    extract::{Path, Query, State},
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use sqlx::Row;
use std::collections::HashSet;
use std::sync::Arc;

use crate::agent::types::{OversightDecision, OversightEntry, ToolCallAudit};
use crate::error::AppError;
use crate::routes::oversight::audit::parse_and_redact_ledger_json;
use crate::routes::oversight::principal::{
    parse_oversight_decision, validate_identifier, OversightPrincipal,
};
use crate::routes::pagination::{PaginatedResponse, PaginationParams};
use crate::state::AppState;

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

    let mut sql = "SELECT id, mission_id, agent_id, skill, params, status, created_at FROM oversight_log WHERE status = 'pending'".to_string();
    if query.mission_id.is_some() {
        sql.push_str(" AND mission_id = ?");
    }
    if query.agent_id.is_some() {
        sql.push_str(" AND agent_id = ?");
    }
    if query.skill.is_some() {
        sql.push_str(" AND skill = ?");
    }
    sql.push_str(" ORDER BY created_at DESC LIMIT 500");

    let mut q = sqlx::query(&sql);
    if let Some(ref m) = query.mission_id {
        q = q.bind(m);
    }
    if let Some(ref a) = query.agent_id {
        q = q.bind(a);
    }
    if let Some(ref s) = query.skill {
        q = q.bind(s);
    }

    let db_rows = q.fetch_all(&state.resources.pool).await.map_err(AppError::Sqlx)?;
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
        seen_ids.insert(id);
        entries.push(reconstructed);
    }

    entries.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| a.id.cmp(&b.id)));

    let pagination_params = query.to_pagination_params();
    Ok(Json(PaginatedResponse::from_vec(
        entries,
        &pagination_params,
        "/v1/oversight/pending",
    )))
}

/// Finds an existing pending oversight entry without mutating the database.
async fn find_pending_entry_context(
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
        (Some(entry), None) => Ok(entry),
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

/// Seeds a durable pending row in `oversight_log` if not yet committed (e.g. enqueued only in memory).
/// Uses `ON CONFLICT(id) DO NOTHING` to eliminate concurrency races.
async fn seed_pending_entry_if_needed(
    state: &AppState,
    entry: &OversightEntry,
) -> Result<(), AppError> {
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
         VALUES (?, ?, ?, 'tool_call', ?, ?, 'pending', ?) \
         ON CONFLICT(id) DO NOTHING",
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

    Ok(())
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

    // 1. Resolve entry context without mutating DB or removing from queue yet
    let entry = find_pending_entry_context(&state, &validated_entry_id).await?;
    let canonical_id = entry.id.clone();

    // 2. Authorize principal against the entry's mission scope BEFORE database mutation (§2.5)
    let actor = principal
        .authorize_mission(entry.mission_id.as_deref())?
        .to_string();

    // 3. Ensure durable row exists in oversight_log (idempotent ON CONFLICT DO NOTHING)
    seed_pending_entry_if_needed(&state, &entry).await?;

    tracing::info!(
        entry_id = %canonical_id,
        decision = %normalized_decision,
        operator = %actor,
        "[Oversight] Decision received"
    );

    let raw_answer = payload
        .answer
        .as_ref()
        .map(|ans| ans.trim().to_string())
        .filter(|a| !a.is_empty());
    let sanitized_answer = raw_answer
        .as_ref()
        .map(|ans| crate::utils::security::redact_secrets(ans));

    // 4. Authoritative DB update with `AND status = 'pending'` OCC guard
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

    // 5. Record in tamper-evident Merkle audit trail BEFORE unblocking the agent
    let params = serde_json::to_string(&json!({
        "entry_id": canonical_id,
        "approved": approved,
        "decided_by": actor,
        "answer": sanitized_answer
    }))
    .unwrap_or_default();

    if let Err(e) = state
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
    {
        tracing::error!(
            entry_id = %canonical_id,
            operator = %actor,
            error = %e,
            "Failed to record oversight decision in audit trail; reverting DB row to pending to prevent stranding (§2.2)"
        );
        // Rollback status to 'pending' so retries succeed and agent is not stranded
        let _ = sqlx::query(
            "UPDATE oversight_log SET status = 'pending', decision = NULL, decided_at = NULL, decided_by = NULL WHERE id = ?",
        )
        .bind(&canonical_id)
        .execute(&state.resources.pool)
        .await;

        return Err(AppError::InternalServerError(
            "Failed to record oversight audit entry".to_string(),
        ));
    }

    // 6. Store operator guidance and remove from in-memory pending queue
    if let Some(ref ans) = raw_answer {
        state
            .comms
            .oversight_answers
            .insert(canonical_id.clone(), ans.clone());
    }
    state.comms.oversight_queue.remove(&canonical_id);

    // 7. Resolve the waiting promise to unblock the agent
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

    // 8. Broadcast and emit UI telemetry event (uses sanitized_answer)
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

// Metadata: [queue]



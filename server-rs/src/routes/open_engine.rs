//! Open Engine P0: Status Ledger, Receipt System, Task Claim Lock
//!
//! @docs ARCHITECTURE:OpenEngine
//!
//! ### AI Assist Note
//! Implements the three P0 improvements from the Open Engine framework:
//!
//! 1. **Status Ledger** — per-agent living operational status (GET/PUT)
//! 2. **Receipt System** — standardized task state tokens (POST /receipts)
//! 3. **Task Claim Lock** — atomic claimed_by compare-and-swap (POST /claim)
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: 409 on double-claim race; 404 if agent/task not found
//! - **Telemetry Link**: Search `[Ledger]`, `[Receipt]`, `[Claim]` in logs

use crate::error::AppError;
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// ─── Receipt Types ────────────────────────────────────────────────────────────

/// Standardized receipt vocabulary from the Open Engine coordination framework.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptType {
    /// Task started — claim lock established
    Claimed,
    /// Task complete, no human review required
    Done,
    /// Waiting for input on the same task thread (inline blocker)
    Blocked,
    /// Waiting for out-of-band human decision (permissions, auth)
    HumanHold,
    /// Inline blocker cleared — resuming
    Unblocked,
    /// Work resuming after any pause
    Resumed,
    /// Unrecoverable failure (includes last_safe_step + retry_count)
    Failed,
    /// Complete but requires human QA / review
    Review,
    /// First optional skill approval granted → subscription active
    SkillSubscribed,
    /// Subscribed optional skill received same-scope update
    SkillUpdated,
    /// Delegated task state changed, follow-up recorded
    FollowUp,
}

impl ReceiptType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ReceiptType::Claimed          => "claimed",
            ReceiptType::Done             => "done",
            ReceiptType::Blocked          => "blocked",
            ReceiptType::HumanHold        => "human_hold",
            ReceiptType::Unblocked        => "unblocked",
            ReceiptType::Resumed          => "resumed",
            ReceiptType::Failed           => "failed",
            ReceiptType::Review           => "review",
            ReceiptType::SkillSubscribed  => "skill_subscribed",
            ReceiptType::SkillUpdated     => "skill_updated",
            ReceiptType::FollowUp         => "follow_up",
        }
    }
}

// ─── Request / Response Types ─────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostReceiptRequest {
    pub receipt_type: ReceiptType,
    /// For BLOCKED receipts: the specific question that must be answered
    pub blocking_question: Option<String>,
    /// For BLOCKED receipts: inline (answer on task) or human_hold (out-of-band)
    pub block_type: Option<String>,
    /// For FAILED receipts: last step completed successfully before failure
    pub last_safe_step: Option<String>,
    /// Optional notes attached to this receipt
    pub notes: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReceiptResponse {
    pub task_id: String,
    pub receipt_type: String,
    pub recorded_at: i64,
    pub message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusLedgerResponse {
    pub agent_id: String,
    pub agent_code: String,
    pub runtime: Option<String>,
    pub automation_state: String,
    pub last_heartbeat: Option<i64>,
    pub last_queue_result: String,
    pub last_task_id: Option<String>,
    pub last_successful_run: Option<i64>,
    pub context_version: i64,
    pub context_packet: serde_json::Value,
    pub subscribed_skills: serde_json::Value,
    pub notes: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateLedgerRequest {
    pub last_queue_result: Option<String>,
    pub last_task_id: Option<String>,
    pub automation_state: Option<String>,
    pub notes: Option<String>,
    pub context_version: Option<i64>,
}

// ─── POST /v1/agents/:id/tasks/:task_id/claim ─────────────────────────────────

/// Atomically claims a task for this agent.
/// Returns 409 Conflict if another agent has already claimed it.
pub async fn claim_task(
    State(state): State<Arc<AppState>>,
    Path((agent_id, task_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let now = chrono::Utc::now().timestamp();

    // Atomic compare-and-swap: only update if claimed_by IS NULL
    let rows_affected = sqlx::query(
        "UPDATE agent_tasks
         SET claimed_by = ?1, claimed_at = ?2, current_receipt = 'claimed',
             status = 'working', updated_at = ?2
         WHERE id = ?3 AND agent_id = ?4 AND claimed_by IS NULL"
    )
    .bind(&agent_id)
    .bind(now)
    .bind(&task_id)
    .bind(&agent_id)
    .execute(&state.resources.pool)
    .await
    .map_err(AppError::Sqlx)?
    .rows_affected();

    if rows_affected == 0 {
        return Err(AppError::Conflict(
            "Task already claimed by another agent or does not belong to this agent".to_string()
        ));
    }

    // Append AGENT CLAIMED to receipt_history
    append_receipt_history(&state, &task_id, "claimed", None).await?;

    // Update status ledger
    update_ledger_result(&state, &agent_id, &format!("claimed {}", task_id), Some(task_id.clone())).await?;

    tracing::info!("🔒 [Claim] Agent {} claimed task {}", agent_id, task_id);

    Ok((StatusCode::OK, Json(serde_json::json!({
        "status": "claimed",
        "agentId": agent_id,
        "taskId": task_id,
        "claimedAt": now,
        "receipt": "AGENT CLAIMED"
    }))))
}

// ─── POST /v1/agents/:id/tasks/:task_id/receipts ──────────────────────────────

/// Posts a receipt (state transition token) to a task.
/// Updates current_receipt and appends to receipt_history.
pub async fn post_receipt(
    State(state): State<Arc<AppState>>,
    Path((agent_id, task_id)): Path<(String, String)>,
    Json(body): Json<PostReceiptRequest>,
) -> Result<impl IntoResponse, AppError> {
    let now = chrono::Utc::now().timestamp();
    let receipt_str = body.receipt_type.as_str();

    // Determine new task status from receipt type
    let new_status = match body.receipt_type {
        ReceiptType::Claimed          => "working",
        ReceiptType::Done             => "done",
        ReceiptType::Blocked          => "needs_input",
        ReceiptType::HumanHold        => "needs_input",
        ReceiptType::Unblocked        => "working",
        ReceiptType::Resumed          => "working",
        ReceiptType::Failed           => "failed",
        ReceiptType::Review           => "review",
        ReceiptType::SkillSubscribed  => "working",
        ReceiptType::SkillUpdated     => "working",
        ReceiptType::FollowUp         => "working",
    };

    let block_type = body.block_type.as_deref().unwrap_or("inline");

    sqlx::query(
        "UPDATE agent_tasks
         SET current_receipt    = ?1,
             status             = ?2,
             block_type         = CASE WHEN ?3 IN ('inline','human_hold') THEN ?3 ELSE block_type END,
             blocking_question  = COALESCE(?4, blocking_question),
             last_safe_step     = COALESCE(?5, last_safe_step),
             retry_count        = CASE WHEN ?1 = 'failed' THEN retry_count + 1 ELSE retry_count END,
             updated_at         = ?6
         WHERE id = ?7 AND agent_id = ?8"
    )
    .bind(receipt_str)
    .bind(new_status)
    .bind(block_type)
    .bind(body.blocking_question.as_deref())
    .bind(body.last_safe_step.as_deref())
    .bind(now)
    .bind(&task_id)
    .bind(&agent_id)
    .execute(&state.resources.pool)
    .await
    .map_err(AppError::Sqlx)?;

    // Append to receipt_history JSON array
    append_receipt_history(
        &state, &task_id, receipt_str,
        body.notes.as_deref()
    ).await?;

    // Emit WebSocket event for real-time dashboard update
    state.emit_event(serde_json::json!({
        "type": "agent:task_receipt",
        "agentId": agent_id,
        "taskId": task_id,
        "receipt": receipt_str,
        "status": new_status,
        "timestamp": now
    }));

    // Update status ledger with new result
    let ledger_result = match body.receipt_type {
        ReceiptType::Done            => format!("completed {}", task_id),
        ReceiptType::Failed          => format!("failed {}", task_id),
        ReceiptType::Blocked         => format!("blocked {}", task_id),
        ReceiptType::HumanHold       => format!("holding {}", task_id),
        ReceiptType::Resumed         => format!("resumed {}", task_id),
        _                            => format!("observed {}", task_id),
    };
    update_ledger_result(&state, &agent_id, &ledger_result, Some(task_id.clone())).await?;

    tracing::info!("📝 [Receipt] Agent {} task {} → AGENT {}", agent_id, task_id, receipt_str.to_uppercase());

    Ok((StatusCode::CREATED, Json(ReceiptResponse {
        task_id,
        receipt_type: receipt_str.to_string(),
        recorded_at: now,
        message: format!("AGENT {}", receipt_str.to_uppercase()),
    })))
}

// ─── GET /v1/agents/:id/status-ledger ─────────────────────────────────────────

/// Returns the living status document for this agent.
/// Created on first access with sensible defaults.
pub async fn get_status_ledger(
    State(state): State<Arc<AppState>>,
    Path(agent_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    use sqlx::Row;

    // Upsert: create ledger row if it doesn't exist yet
    let agent_code = format!("tadpole-{}", &agent_id[..agent_id.len().min(8)]);
    sqlx::query(
        "INSERT OR IGNORE INTO agent_status_ledger
         (agent_id, agent_code, last_queue_result, automation_state)
         VALUES (?1, ?2, 'none', 'manual')"
    )
    .bind(&agent_id)
    .bind(&agent_code)
    .execute(&state.resources.pool)
    .await
    .map_err(AppError::Sqlx)?;

    let row = sqlx::query(
        "SELECT agent_id, agent_code, runtime, automation_state,
                last_heartbeat, last_queue_result, last_task_id,
                last_successful_run, context_version, context_packet,
                subscribed_skills, notes
         FROM agent_status_ledger WHERE agent_id = ?1"
    )
    .bind(&agent_id)
    .fetch_optional(&state.resources.pool)
    .await
    .map_err(AppError::Sqlx)?
    .ok_or_else(|| AppError::NotFound(format!("Status ledger for agent {} not found", agent_id)))?;

    let context_packet: serde_json::Value = row.try_get::<String, _>("context_packet")
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(serde_json::json!({}));

    let subscribed_skills: serde_json::Value = row.try_get::<String, _>("subscribed_skills")
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(serde_json::json!([]));

    Ok(Json(StatusLedgerResponse {
        agent_id:          row.get("agent_id"),
        agent_code:        row.get("agent_code"),
        runtime:           row.try_get("runtime").ok(),
        automation_state:  row.get("automation_state"),
        last_heartbeat:    row.try_get("last_heartbeat").ok(),
        last_queue_result: row.get("last_queue_result"),
        last_task_id:      row.try_get("last_task_id").ok(),
        last_successful_run: row.try_get("last_successful_run").ok(),
        context_version:   row.try_get("context_version").unwrap_or(1),
        context_packet,
        subscribed_skills,
        notes:             row.try_get("notes").ok(),
    }))
}

// ─── PUT /v1/agents/:id/status-ledger ─────────────────────────────────────────

/// Updates the status ledger for this agent (heartbeat / queue result update).
pub async fn update_status_ledger(
    State(state): State<Arc<AppState>>,
    Path(agent_id): Path<String>,
    Json(body): Json<UpdateLedgerRequest>,
) -> Result<impl IntoResponse, AppError> {
    let now = chrono::Utc::now().timestamp();

    sqlx::query(
        "INSERT INTO agent_status_ledger (agent_id, agent_code, last_queue_result, last_heartbeat, automation_state)
         VALUES (?1, ?2, COALESCE(?3, 'none'), ?4, COALESCE(?5, 'manual'))
         ON CONFLICT(agent_id) DO UPDATE SET
             last_queue_result   = COALESCE(?3, last_queue_result),
             last_task_id        = COALESCE(?6, last_task_id),
             last_heartbeat      = ?4,
             automation_state    = COALESCE(?5, automation_state),
             notes               = COALESCE(?7, notes),
             context_version     = COALESCE(?8, context_version)"
    )
    .bind(&agent_id)
    .bind(format!("tadpole-{}", &agent_id[..agent_id.len().min(8)]))
    .bind(body.last_queue_result.as_deref())
    .bind(now)
    .bind(body.automation_state.as_deref())
    .bind(body.last_task_id.as_deref())
    .bind(body.notes.as_deref())
    .bind(body.context_version)
    .execute(&state.resources.pool)
    .await
    .map_err(AppError::Sqlx)?;

    tracing::debug!("📊 [Ledger] Agent {} heartbeat updated", agent_id);

    Ok((StatusCode::OK, Json(serde_json::json!({
        "status": "ok",
        "agentId": agent_id,
        "heartbeat": now
    }))))
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Appends a receipt entry to the JSON receipt_history array on a task.
async fn append_receipt_history(
    state: &Arc<AppState>,
    task_id: &str,
    receipt: &str,
    notes: Option<&str>,
) -> Result<(), AppError> {
    use sqlx::Row;

    let row = sqlx::query("SELECT receipt_history FROM agent_tasks WHERE id = ?1")
        .bind(task_id)
        .fetch_optional(&state.resources.pool)
        .await
        .map_err(AppError::Sqlx)?;

    let mut history: Vec<serde_json::Value> = row
        .and_then(|r| r.try_get::<String, _>("receipt_history").ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();

    history.push(serde_json::json!({
        "receipt": receipt,
        "timestamp": chrono::Utc::now().timestamp(),
        "notes": notes
    }));

    let history_json = serde_json::to_string(&history)
        .unwrap_or_else(|_| "[]".to_string());

    sqlx::query("UPDATE agent_tasks SET receipt_history = ?1 WHERE id = ?2")
        .bind(&history_json)
        .bind(task_id)
        .execute(&state.resources.pool)
        .await
        .map_err(AppError::Sqlx)?;

    Ok(())
}

/// Updates the status ledger last_queue_result field.
async fn update_ledger_result(
    state: &Arc<AppState>,
    agent_id: &str,
    result: &str,
    task_id: Option<String>,
) -> Result<(), AppError> {
    let now = chrono::Utc::now().timestamp();

    sqlx::query(
        "INSERT INTO agent_status_ledger (agent_id, agent_code, last_queue_result, last_task_id, last_heartbeat)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(agent_id) DO UPDATE SET
             last_queue_result = ?3,
             last_task_id      = COALESCE(?4, last_task_id),
             last_heartbeat    = ?5"
    )
    .bind(agent_id)
    .bind(format!("tadpole-{}", &agent_id[..agent_id.len().min(8)]))
    .bind(result)
    .bind(task_id.as_deref())
    .bind(now)
    .execute(&state.resources.pool)
    .await
    .map_err(AppError::Sqlx)?;

    Ok(())
}

//! Agentic Engine P0: Status Ledger, Receipt System, Task Claim Lock
//!
//! @docs ARCHITECTURE:AgenticEngine
//!
//! ### AI Assist Note
//! Implements the three P0 improvements from the Agentic Engine framework:
//!
//! 1. **Status Ledger** — per-agent living operational status (GET/PUT)
//! 2. **Receipt System** — standardized task state tokens (POST /receipts)
//! 3. **Task Claim Lock** — atomic claimed_by compare-and-swap (POST /claim)
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: 409 on double-claim race; 404 if agent/task not found
//! - **Telemetry Link**: Search `[agentic_engine]`, `[Receipt]`, `[Claim]` in logs

use crate::error::AppError;
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use specta::Type;
use sqlx::{Row, SqlitePool};
use std::sync::Arc;

// ─── Receipt Types ────────────────────────────────────────────────────────────

/// Standardized receipt vocabulary from the Agentic Engine coordination framework.
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
            ReceiptType::Claimed => "claimed",
            ReceiptType::Done => "done",
            ReceiptType::Blocked => "blocked",
            ReceiptType::HumanHold => "human_hold",
            ReceiptType::Unblocked => "unblocked",
            ReceiptType::Resumed => "resumed",
            ReceiptType::Failed => "failed",
            ReceiptType::Review => "review",
            ReceiptType::SkillSubscribed => "skill_subscribed",
            ReceiptType::SkillUpdated => "skill_updated",
            ReceiptType::FollowUp => "follow_up",
        }
    }

    pub fn task_status(&self) -> &'static str {
        match self {
            ReceiptType::Claimed
            | ReceiptType::Unblocked
            | ReceiptType::Resumed
            | ReceiptType::SkillSubscribed
            | ReceiptType::SkillUpdated
            | ReceiptType::FollowUp => "working",
            ReceiptType::Done => "done",
            ReceiptType::Blocked | ReceiptType::HumanHold => "needs_input",
            ReceiptType::Failed => "failed",
            ReceiptType::Review => "review",
        }
    }

    pub fn format_ledger_result(&self, task_id: &str) -> String {
        let verb = match self {
            ReceiptType::Done => "completed",
            ReceiptType::Failed => "failed",
            ReceiptType::Blocked => "blocked",
            ReceiptType::HumanHold => "holding",
            ReceiptType::Resumed => "resumed",
            _ => "observed",
        };
        format!("{} {}", verb, task_id)
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

#[derive(Serialize, Type, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ClaimResponse {
    pub status: String,
    pub agent_id: String,
    pub task_id: String,
    #[specta(type = f64)]
    pub claimed_at: i64,
    pub receipt: String,
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
         WHERE id = ?3 AND agent_id = ?4 AND claimed_by IS NULL",
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
            "Task already claimed by another agent or does not belong to this agent".to_string(),
        ));
    }

    // Append AGENT CLAIMED to receipt_history
    append_receipt_history(&state, &task_id, "claimed", None).await?;

    // Update status ledger
    update_ledger_result(
        &state,
        &agent_id,
        &format!("claimed {}", task_id),
        Some(task_id.clone()),
    )
    .await?;

    tracing::info!("🔒 [Claim] Agent {} claimed task {}", agent_id, task_id);

    Ok((
        StatusCode::OK,
        Json(ClaimResponse {
            status: "claimed".to_string(),
            agent_id,
            task_id,
            claimed_at: now,
            receipt: "AGENT CLAIMED".to_string(),
        }),
    ))
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
    let new_status = body.receipt_type.task_status();
    let block_type = body.block_type.as_deref().unwrap_or("inline");

    let res = sqlx::query(
        "UPDATE agent_tasks
         SET current_receipt    = ?1,
             status             = ?2,
             block_type         = CASE WHEN ?3 IN ('inline','human_hold') THEN ?3 ELSE block_type END,
             blocking_question  = COALESCE(?4, blocking_question),
             last_safe_step     = COALESCE(?5, last_safe_step),
             retry_count        = CASE WHEN ?1 = 'failed' THEN retry_count + 1 ELSE retry_count END,
             updated_at         = ?6
         WHERE id = ?7 AND agent_id = ?8",
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

    if res.rows_affected() == 0 {
        return Err(AppError::NotFound(format!(
            "Task {} for agent {} not found",
            task_id, agent_id
        )));
    }

    // Append to receipt_history JSON array
    append_receipt_history(&state, &task_id, receipt_str, body.notes.as_deref()).await?;

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
    let ledger_result = body.receipt_type.format_ledger_result(&task_id);
    update_ledger_result(&state, &agent_id, &ledger_result, Some(task_id.clone())).await?;

    tracing::info!(
        "📝 [Receipt] Agent {} task {} → AGENT {}",
        agent_id,
        task_id,
        receipt_str.to_uppercase()
    );

    Ok((
        StatusCode::CREATED,
        Json(ReceiptResponse {
            task_id,
            receipt_type: receipt_str.to_string(),
            recorded_at: now,
            message: format!("AGENT {}", receipt_str.to_uppercase()),
        }),
    ))
}

// ─── GET /v1/agents/:id/status-ledger ─────────────────────────────────────────

/// Returns the living status document for this agent.
/// Created on first access with sensible defaults.
pub async fn get_status_ledger(
    State(state): State<Arc<AppState>>,
    Path(agent_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    ensure_status_ledger(&state.resources.pool, &agent_id).await?;

    let row = sqlx::query(
        "SELECT agent_id, agent_code, runtime, automation_state,
                last_heartbeat, last_queue_result, last_task_id,
                last_successful_run, context_version, context_packet,
                subscribed_skills, notes
         FROM agent_status_ledger WHERE agent_id = ?1",
    )
    .bind(&agent_id)
    .fetch_optional(&state.resources.pool)
    .await
    .map_err(AppError::Sqlx)?
    .ok_or_else(|| AppError::NotFound(format!("Status ledger for agent {} not found", agent_id)))?;

    let context_packet: serde_json::Value = row
        .try_get::<String, _>("context_packet")
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| serde_json::json!({}));

    let subscribed_skills: serde_json::Value = row
        .try_get::<String, _>("subscribed_skills")
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| serde_json::json!([]));

    Ok(Json(StatusLedgerResponse {
        agent_id: row.get("agent_id"),
        agent_code: row.get("agent_code"),
        runtime: row.try_get("runtime").ok(),
        automation_state: row.get("automation_state"),
        last_heartbeat: row.try_get("last_heartbeat").ok(),
        last_queue_result: row.get("last_queue_result"),
        last_task_id: row.try_get("last_task_id").ok(),
        last_successful_run: row.try_get("last_successful_run").ok(),
        context_version: row.try_get("context_version").unwrap_or(1),
        context_packet,
        subscribed_skills,
        notes: row.try_get("notes").ok(),
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
             context_version     = COALESCE(?8, context_version)",
    )
    .bind(&agent_id)
    .bind(generate_agent_code(&agent_id))
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

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "ok",
            "agentId": agent_id,
            "heartbeat": now
        })),
    ))
}

// ─── Shared Helpers ───────────────────────────────────────────────────────────

/// Generates a standardized agent code for neural identification.
fn generate_agent_code(agent_id: &str) -> String {
    let prefix: String = agent_id.chars().take(8).collect();
    format!("tadpole-{}", prefix)
}

/// Ensures an `agent_status_ledger` row exists for `agent_id` with default values.
async fn ensure_status_ledger(pool: &SqlitePool, agent_id: &str) -> Result<(), AppError> {
    let agent_code = generate_agent_code(agent_id);
    sqlx::query(
        "INSERT OR IGNORE INTO agent_status_ledger
         (agent_id, agent_code, last_queue_result, automation_state)
         VALUES (?1, ?2, 'none', 'manual')",
    )
    .bind(agent_id)
    .bind(&agent_code)
    .execute(pool)
    .await
    .map_err(AppError::Sqlx)?;
    Ok(())
}

/// Appends a receipt entry to the JSON receipt_history array on a task.
async fn append_receipt_history(
    state: &Arc<AppState>,
    task_id: &str,
    receipt: &str,
    notes: Option<&str>,
) -> Result<(), AppError> {
    let entry = serde_json::json!({
        "receipt": receipt,
        "timestamp": chrono::Utc::now().timestamp(),
        "notes": notes
    });
    let entry_str = serde_json::to_string(&entry).unwrap_or_else(|_| "{}".to_string());

    sqlx::query(
        "UPDATE agent_tasks 
         SET receipt_history = json_insert(COALESCE(receipt_history, '[]'), '$[#]', json(?1)) 
         WHERE id = ?2",
    )
    .bind(&entry_str)
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
             last_heartbeat    = ?5",
    )
    .bind(agent_id)
    .bind(generate_agent_code(agent_id))
    .bind(result)
    .bind(task_id.as_deref())
    .bind(now)
    .execute(&state.resources.pool)
    .await
    .map_err(AppError::Sqlx)?;

    Ok(())
}

// ─── Phase 2 Endpoints ────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextPacketResponse {
    pub agent_id: String,
    pub context_version: i64,
    pub context_packet: serde_json::Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateContextPacketRequest {
    pub context_version: i64,
    pub context_packet: serde_json::Value,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillSubscriptionResponse {
    pub id: String,
    pub agent_id: String,
    pub skill_id: String,
    pub approved_at: Option<i64>,
    pub scope_hash: String,
    pub subscription_status: String,
    pub installed_at: Option<i64>,
    pub last_updated_at: Option<i64>,
    pub notes: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscribeSkillRequest {
    pub notes: Option<String>,
}

/// Helper to compute stable SHA-256 scope hash for a skill manifest.
pub fn compute_skill_scope_hash(manifest: &crate::agent::skill_manifest::SkillManifest) -> String {
    let scope_data = serde_json::json!({
        "permissions": manifest.permissions,
        "toolset_group": manifest.toolset_group,
        "requires_oversight": manifest.requires_oversight,
        "parameters": manifest.parameters,
    });
    let serialized = serde_json::to_string(&scope_data).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(serialized.as_bytes());
    hex::encode(hasher.finalize())
}

/// Resolves a skill across manifest registry, builtin tools, and dynamic script registry,
/// returning `(scope_hash, version)`.
fn resolve_skill_scope_and_version(
    state: &AppState,
    skill_id: &str,
) -> Result<(String, String), AppError> {
    if let Some(manifest) = state.registry.skill_registry.get(skill_id) {
        Ok((compute_skill_scope_hash(&manifest), manifest.version))
    } else if crate::agent::runner::tools::registry::BUILTIN_TOOLS.contains(&skill_id) {
        let mut hasher = Sha256::new();
        hasher.update(b"builtin:");
        hasher.update(skill_id.as_bytes());
        Ok((hex::encode(hasher.finalize()), "1.0.0".to_string()))
    } else if state.registry.skills.snapshot().skills.contains_key(skill_id) {
        let mut hasher = Sha256::new();
        hasher.update(b"script:");
        hasher.update(skill_id.as_bytes());
        Ok((hex::encode(hasher.finalize()), "1.0.0".to_string()))
    } else {
        Err(AppError::NotFound(format!(
            "Skill '{}' not found in registry",
            skill_id
        )))
    }
}

/// GET /v1/agents/:id/context-packet
pub async fn get_context_packet(
    State(state): State<Arc<AppState>>,
    Path(agent_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    ensure_status_ledger(&state.resources.pool, &agent_id).await?;

    let row = sqlx::query(
        "SELECT context_version, context_packet FROM agent_status_ledger WHERE agent_id = ?1",
    )
    .bind(&agent_id)
    .fetch_optional(&state.resources.pool)
    .await
    .map_err(AppError::Sqlx)?
    .ok_or_else(|| AppError::NotFound(format!("Status ledger for agent {} not found", agent_id)))?;

    let context_version = row.try_get::<i64, _>("context_version").unwrap_or(1);
    let context_packet_str: String = row
        .try_get("context_packet")
        .unwrap_or_else(|_| "{}".to_string());
    let context_packet: serde_json::Value =
        serde_json::from_str(&context_packet_str).unwrap_or_else(|_| serde_json::json!({}));

    Ok(Json(ContextPacketResponse {
        agent_id,
        context_version,
        context_packet,
    }))
}

/// PUT /v1/agents/:id/context-packet
pub async fn update_context_packet(
    State(state): State<Arc<AppState>>,
    Path(agent_id): Path<String>,
    Json(body): Json<UpdateContextPacketRequest>,
) -> Result<impl IntoResponse, AppError> {
    let context_packet_str =
        serde_json::to_string(&body.context_packet).unwrap_or_else(|_| "{}".to_string());

    let res = sqlx::query(
        "UPDATE agent_status_ledger
         SET context_version = ?1, context_packet = ?2
         WHERE agent_id = ?3",
    )
    .bind(body.context_version)
    .bind(&context_packet_str)
    .bind(&agent_id)
    .execute(&state.resources.pool)
    .await
    .map_err(AppError::Sqlx)?;

    if res.rows_affected() == 0 {
        return Err(AppError::NotFound(format!(
            "Agent status ledger for agent {} not found",
            agent_id
        )));
    }

    state.emit_event(serde_json::json!({
        "type": "agent:context_packet_updated",
        "agentId": agent_id,
        "contextVersion": body.context_version,
        "contextPacket": body.context_packet,
    }));

    Ok((StatusCode::OK, Json(serde_json::json!({ "status": "ok" }))))
}

/// GET /v1/agents/:id/skills/subscribed
pub async fn get_subscribed_skills(
    State(state): State<Arc<AppState>>,
    Path(agent_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let rows = sqlx::query(
        "SELECT id, agent_id, skill_id, approved_at, scope_hash, subscription_status,
                installed_at, last_updated_at, notes
         FROM skill_subscriptions WHERE agent_id = ?1",
    )
    .bind(&agent_id)
    .fetch_all(&state.resources.pool)
    .await
    .map_err(AppError::Sqlx)?;

    let mut subs = Vec::with_capacity(rows.len());
    for row in rows {
        subs.push(SkillSubscriptionResponse {
            id: row.get("id"),
            agent_id: row.get("agent_id"),
            skill_id: row.get("skill_id"),
            approved_at: row.try_get("approved_at").ok(),
            scope_hash: row.get("scope_hash"),
            subscription_status: row.get("subscription_status"),
            installed_at: row.try_get("installed_at").ok(),
            last_updated_at: row.try_get("last_updated_at").ok(),
            notes: row.try_get("notes").ok(),
        });
    }

    Ok(Json(subs))
}

/// POST /v1/agents/:id/skills/:skill_id/subscribe
pub async fn subscribe_skill(
    State(state): State<Arc<AppState>>,
    Path((agent_id, skill_id)): Path<(String, String)>,
    Json(body): Json<SubscribeSkillRequest>,
) -> Result<impl IntoResponse, AppError> {
    let now = chrono::Utc::now().timestamp();
    let (current_scope_hash, _) = resolve_skill_scope_and_version(&state, &skill_id)?;

    let existing = sqlx::query(
        "SELECT id, subscription_status, scope_hash FROM skill_subscriptions WHERE agent_id = ?1 AND skill_id = ?2",
    )
    .bind(&agent_id)
    .bind(&skill_id)
    .fetch_optional(&state.resources.pool)
    .await
    .map_err(AppError::Sqlx)?;

    let (sub_id, new_status) = if let Some(row) = existing {
        let id: String = row.get("id");
        let status: String = row.get("subscription_status");
        let old_hash: String = row.get("scope_hash");

        let next_status = if old_hash == current_scope_hash {
            status
        } else if status == "approved" {
            "pending_reapproval".to_string()
        } else {
            "pending".to_string()
        };

        sqlx::query(
            "UPDATE skill_subscriptions
             SET scope_hash = ?1, subscription_status = ?2, last_updated_at = ?3, notes = ?4
             WHERE id = ?5",
        )
        .bind(&current_scope_hash)
        .bind(&next_status)
        .bind(now)
        .bind(body.notes.as_deref())
        .bind(&id)
        .execute(&state.resources.pool)
        .await
        .map_err(AppError::Sqlx)?;

        (id, next_status)
    } else {
        let id = format!("sub-{}", uuid::Uuid::new_v4());
        let status = "pending".to_string();

        sqlx::query(
            "INSERT INTO skill_subscriptions
             (id, agent_id, skill_id, scope_hash, subscription_status, installed_at, last_updated_at, notes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, ?7)",
        )
        .bind(&id)
        .bind(&agent_id)
        .bind(&skill_id)
        .bind(&current_scope_hash)
        .bind(&status)
        .bind(now)
        .bind(body.notes.as_deref())
        .execute(&state.resources.pool)
        .await
        .map_err(AppError::Sqlx)?;

        (id, status)
    };

    state.emit_event(serde_json::json!({
        "type": "agent:skill_subscription_updated",
        "agentId": agent_id,
        "skillId": skill_id,
        "subscriptionStatus": new_status,
        "scopeHash": current_scope_hash,
    }));

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "ok",
            "subscriptionId": sub_id,
            "subscriptionStatus": new_status,
        })),
    ))
}

/// POST /v1/agents/:id/skills/:skill_id/approve
pub async fn approve_skill(
    State(state): State<Arc<AppState>>,
    Path((agent_id, skill_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let now = chrono::Utc::now().timestamp();
    let (current_scope_hash, skill_version) = resolve_skill_scope_and_version(&state, &skill_id)?;

    let rows_affected = sqlx::query(
        "UPDATE skill_subscriptions
         SET subscription_status = 'approved', approved_at = ?1, scope_hash = ?2, last_updated_at = ?1
         WHERE agent_id = ?3 AND skill_id = ?4",
    )
    .bind(now)
    .bind(&current_scope_hash)
    .bind(&agent_id)
    .bind(&skill_id)
    .execute(&state.resources.pool)
    .await
    .map_err(AppError::Sqlx)?
    .rows_affected();

    if rows_affected == 0 {
        return Err(AppError::NotFound(format!(
            "Subscription not found for agent {} and skill {}",
            agent_id, skill_id
        )));
    }

    let row = sqlx::query("SELECT subscribed_skills FROM agent_status_ledger WHERE agent_id = ?1")
        .bind(&agent_id)
        .fetch_optional(&state.resources.pool)
        .await
        .map_err(AppError::Sqlx)?;

    let mut subscribed_skills: Vec<serde_json::Value> = row
        .and_then(|r| r.try_get::<String, _>("subscribed_skills").ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();

    subscribed_skills.retain(|item| {
        item.get("skill_id")
            .and_then(|v| v.as_str())
            .map(|s| s != skill_id)
            .unwrap_or(true)
    });

    subscribed_skills.push(serde_json::json!({
        "skill_id": skill_id,
        "version": skill_version,
        "approved_at": now
    }));

    let subscribed_skills_str =
        serde_json::to_string(&subscribed_skills).unwrap_or_else(|_| "[]".to_string());

    sqlx::query(
        "UPDATE agent_status_ledger
         SET subscribed_skills = ?1
         WHERE agent_id = ?2",
    )
    .bind(&subscribed_skills_str)
    .bind(&agent_id)
    .execute(&state.resources.pool)
    .await
    .map_err(AppError::Sqlx)?;

    state.emit_event(serde_json::json!({
        "type": "agent:skill_approved",
        "agentId": agent_id,
        "skillId": skill_id,
        "approvedAt": now,
        "version": skill_version
    }));

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "approved",
            "agentId": agent_id,
            "skillId": skill_id,
            "approvedAt": now,
        })),
    ))
}

// ─── Maintenance Report Types & Evaluators ───────────────────────────────────

#[derive(Serialize, Type, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AgentMaintenanceReport {
    pub agent_id: String,
    pub overall_score: f64,
    pub status: String,
    pub capability_drift: MaintenanceDimension,
    pub memory_health: MaintenanceDimension,
    pub rate_limits: MaintenanceDimension,
    pub latency: MaintenanceDimension,
    pub error_rate: MaintenanceDimension,
    pub budget_headroom: MaintenanceDimension,
    pub dependency_health: MaintenanceDimension,
    #[specta(type = f64)]
    pub evaluated_at: i64,
}

#[derive(Serialize, Type, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MaintenanceDimension {
    pub score: f64,
    pub status: String, // "optimal" | "warning" | "critical"
    pub details: String,
}

impl MaintenanceDimension {
    fn new(score: f64, status: &str, details: impl Into<String>) -> Self {
        Self {
            score,
            status: status.to_string(),
            details: details.into(),
        }
    }
}

fn evaluate_capability_drift(pending_reapproval_count: i64) -> MaintenanceDimension {
    if pending_reapproval_count > 0 {
        MaintenanceDimension::new(
            0.7,
            "warning",
            format!(
                "{} skill subscription(s) pending re-approval due to scope hash change.",
                pending_reapproval_count
            ),
        )
    } else {
        MaintenanceDimension::new(
            1.0,
            "optimal",
            "No capability drift detected. All approved skill scope hashes are aligned.",
        )
    }
}

fn evaluate_memory_health(wm_len: usize) -> MaintenanceDimension {
    if wm_len > 2000 {
        MaintenanceDimension::new(
            0.8,
            "warning",
            format!(
                "Working memory load is elevated ({} characters). Consider memory compaction.",
                wm_len
            ),
        )
    } else {
        MaintenanceDimension::new(
            1.0,
            "optimal",
            format!(
                "Working memory size is within nominal limits ({} characters).",
                wm_len
            ),
        )
    }
}

fn evaluate_rate_limits(rate_limit_hits: i64) -> MaintenanceDimension {
    if rate_limit_hits > 0 {
        MaintenanceDimension::new(
            0.6,
            "warning",
            format!(
                "Rate limiting occurrences detected in {} recent tasks.",
                rate_limit_hits
            ),
        )
    } else {
        MaintenanceDimension::new(
            1.0,
            "optimal",
            "No rate limit violations or HTTP 429 throttles detected.",
        )
    }
}

fn evaluate_latency(avg_latency: Option<f64>) -> MaintenanceDimension {
    match avg_latency {
        Some(lat) if lat > 180.0 => MaintenanceDimension::new(
            0.5,
            "critical",
            format!(
                "Critical average execution latency: {:.1}s (exceeds 180s threshold).",
                lat
            ),
        ),
        Some(lat) if lat > 60.0 => MaintenanceDimension::new(
            0.8,
            "warning",
            format!(
                "Elevated average execution latency: {:.1}s (exceeds 60s threshold).",
                lat
            ),
        ),
        Some(lat) => MaintenanceDimension::new(
            1.0,
            "optimal",
            format!("Nominal average execution latency: {:.1}s.", lat),
        ),
        None => MaintenanceDimension::new(
            1.0,
            "optimal",
            "No completed tasks available to measure latency.",
        ),
    }
}

fn evaluate_error_rate(task_stats: Option<(i64, i64)>) -> MaintenanceDimension {
    match task_stats {
        Some((total, failed)) if total > 0 => {
            let rate = failed as f64 / total as f64;
            if rate > 0.3 {
                MaintenanceDimension::new(
                    0.5,
                    "critical",
                    format!(
                        "High task error rate: {:.1}% ({}/{} failed).",
                        rate * 100.0,
                        failed,
                        total
                    ),
                )
            } else if rate > 0.1 {
                MaintenanceDimension::new(
                    0.8,
                    "warning",
                    format!(
                        "Moderate task error rate: {:.1}% ({}/{} failed).",
                        rate * 100.0,
                        failed,
                        total
                    ),
                )
            } else {
                MaintenanceDimension::new(
                    1.0,
                    "optimal",
                    format!(
                        "Nominal error rate: {:.1}% ({}/{} failed).",
                        rate * 100.0,
                        failed,
                        total
                    ),
                )
            }
        }
        _ => MaintenanceDimension::new(
            1.0,
            "optimal",
            "No recent tasks available to evaluate error rate.",
        ),
    }
}

fn evaluate_budget_headroom(budget: f64, cost: f64) -> MaintenanceDimension {
    if budget <= 0.0 {
        MaintenanceDimension::new(
            1.0,
            "optimal",
            "Unlimited budget limit. Full headroom available.",
        )
    } else if cost >= budget {
        MaintenanceDimension::new(
            0.0,
            "critical",
            format!(
                "Budget fully exhausted (${:.2} spent of ${:.2} budget). Agent locked.",
                cost, budget
            ),
        )
    } else {
        let headroom_ratio = (budget - cost) / budget;
        if headroom_ratio < 0.1 {
            MaintenanceDimension::new(
                0.5,
                "critical",
                format!(
                    "Critical budget headroom: {:.1}% remaining (${:.2} spent of ${:.2} budget).",
                    headroom_ratio * 100.0,
                    cost,
                    budget
                ),
            )
        } else if headroom_ratio < 0.25 {
            MaintenanceDimension::new(
                0.8,
                "warning",
                format!(
                    "Low budget headroom: {:.1}% remaining (${:.2} spent of ${:.2} budget).",
                    headroom_ratio * 100.0,
                    cost,
                    budget
                ),
            )
        } else {
            MaintenanceDimension::new(
                1.0,
                "optimal",
                format!(
                    "Nominal budget headroom: {:.1}% remaining (${:.2} spent of ${:.2} budget).",
                    headroom_ratio * 100.0,
                    cost,
                    budget
                ),
            )
        }
    }
}

fn evaluate_dependency_health(declined_count: i64, pending_count: i64) -> MaintenanceDimension {
    if declined_count > 0 {
        MaintenanceDimension::new(
            0.4,
            "critical",
            format!(
                "Critical: {} requested skill subscriptions were declined.",
                declined_count
            ),
        )
    } else if pending_count > 0 {
        MaintenanceDimension::new(
            0.8,
            "warning",
            format!(
                "Warning: {} skill subscription(s) are pending approval.",
                pending_count
            ),
        )
    } else {
        MaintenanceDimension::new(
            1.0,
            "optimal",
            "All skill dependencies approved and aligned.",
        )
    }
}

/// GET /v1/agents/:id/maintenance-report
///
/// Evaluates the agent node across 7 dimensions (capability drift, memory health,
/// rate limits, latency, error rate, budget headroom, dependency health).
///
/// @docs API_REFERENCE:GetAgentMaintenanceReport
#[tracing::instrument(skip(state), name = "governance::get_maintenance_report")]
pub async fn get_maintenance_report(
    State(state): State<Arc<AppState>>,
    Path(agent_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let now = chrono::Utc::now().timestamp();

    // Extract snapshot metrics from DashMap and drop the shard guard BEFORE any `.await` point
    // to prevent TOCTOU panics and cross-await DashMap shard deadlocks.
    let (wm_len, budget_usd, cost_usd) = {
        let agent_entry = state
            .registry
            .agents
            .get(&agent_id)
            .ok_or_else(|| AppError::NotFound(format!("Agent {} not found", agent_id)))?;
        let agent = agent_entry.value();
        let wm_len = serde_json::to_string(&agent.state.working_memory)
            .unwrap_or_default()
            .len();
        (wm_len, agent.economics.budget_usd, agent.economics.cost_usd)
    };

    // Query all three counts from skill_subscriptions in a single trip to the DB
    let (pending_reapproval_count, declined_count, pending_count): (i64, i64, i64) =
        sqlx::query_as(
            "SELECT 
                COALESCE(SUM(CASE WHEN subscription_status = 'pending_reapproval' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN subscription_status = 'declined' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN subscription_status = 'pending' THEN 1 ELSE 0 END), 0)
             FROM skill_subscriptions 
             WHERE agent_id = ?1",
        )
        .bind(&agent_id)
        .fetch_one(&state.resources.pool)
        .await
        .map_err(AppError::Sqlx)?;

    let rate_limit_hits: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM agent_tasks 
         WHERE agent_id = ?1 AND (receipt_history LIKE '%rate_limit%' OR receipt_history LIKE '%429%')",
    )
    .bind(&agent_id)
    .fetch_one(&state.resources.pool)
    .await
    .map_err(AppError::Sqlx)?;

    let avg_latency: Option<f64> = sqlx::query_scalar(
        "SELECT AVG(updated_at - claimed_at) FROM (
            SELECT claimed_at, updated_at FROM agent_tasks 
            WHERE agent_id = ?1 AND status = 'done' AND claimed_at IS NOT NULL
            ORDER BY updated_at DESC
            LIMIT 50
         )",
    )
    .bind(&agent_id)
    .fetch_one(&state.resources.pool)
    .await
    .map_err(AppError::Sqlx)?;

    let task_stats: Option<(i64, i64)> = sqlx::query_as(
        "SELECT COUNT(*), COALESCE(SUM(CASE WHEN current_receipt = 'failed' THEN 1 ELSE 0 END), 0) 
         FROM (
            SELECT current_receipt FROM agent_tasks 
            WHERE agent_id = ?1
            ORDER BY updated_at DESC
            LIMIT 50
         )",
    )
    .bind(&agent_id)
    .fetch_optional(&state.resources.pool)
    .await
    .map_err(AppError::Sqlx)?;

    let capability_drift = evaluate_capability_drift(pending_reapproval_count);
    let memory_health = evaluate_memory_health(wm_len);
    let rate_limits = evaluate_rate_limits(rate_limit_hits);
    let latency = evaluate_latency(avg_latency);
    let error_rate = evaluate_error_rate(task_stats);
    let budget_headroom = evaluate_budget_headroom(budget_usd, cost_usd);
    let dependency_health = evaluate_dependency_health(declined_count, pending_count);

    let overall_score = (capability_drift.score
        + memory_health.score
        + rate_limits.score
        + latency.score
        + error_rate.score
        + budget_headroom.score
        + dependency_health.score)
        / 7.0;

    let overall_status = if overall_score >= 0.95 {
        "healthy".to_string()
    } else if overall_score >= 0.75 {
        "warning".to_string()
    } else {
        "critical".to_string()
    };

    Ok(Json(AgentMaintenanceReport {
        agent_id,
        overall_score,
        status: overall_status,
        capability_drift,
        memory_health,
        rate_limits,
        latency,
        error_rate,
        budget_headroom,
        dependency_health,
        evaluated_at: now,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_maintenance_dimension_evaluators_happy_failure_and_edge() {
        // (a) Happy Path: Optimal thresholds across dimensions
        assert_eq!(evaluate_capability_drift(0).status, "optimal");
        assert_eq!(evaluate_memory_health(512).status, "optimal");
        assert_eq!(evaluate_rate_limits(0).status, "optimal");
        assert_eq!(evaluate_latency(Some(12.5)).status, "optimal");
        assert_eq!(evaluate_error_rate(Some((50, 2))).status, "optimal");
        assert_eq!(evaluate_budget_headroom(10.0, 2.0).status, "optimal");
        assert_eq!(evaluate_dependency_health(0, 0).status, "optimal");

        // (b) Failure Path: Critical and warning degradation thresholds
        assert_eq!(evaluate_capability_drift(2).status, "warning");
        assert_eq!(evaluate_memory_health(2500).status, "warning");
        assert_eq!(evaluate_rate_limits(3).status, "warning");
        assert_eq!(evaluate_latency(Some(240.0)).status, "critical");
        assert_eq!(evaluate_error_rate(Some((10, 5))).status, "critical");
        assert_eq!(evaluate_budget_headroom(5.0, 5.0).status, "critical");
        assert_eq!(evaluate_dependency_health(1, 0).status, "critical");

        // (c) Edge Case Path: Zero tasks, unlimited (0.0) budget, exact boundary values
        assert_eq!(evaluate_latency(None).status, "optimal");
        assert_eq!(evaluate_error_rate(Some((0, 0))).status, "optimal");
        assert_eq!(evaluate_budget_headroom(0.0, 100.0).status, "optimal");
        assert_eq!(evaluate_memory_health(2000).status, "optimal");
        assert_eq!(evaluate_latency(Some(60.0)).status, "optimal");
        assert_eq!(evaluate_latency(Some(180.0)).status, "warning");
    }
}

// Metadata: [agentic_engine]

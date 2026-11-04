//! @docs ARCHITECTURE:AgenticEngine
//!
//! ### AI Assist Note
//! Agent context-packet and skill-subscription endpoints.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Missing agents, version conflicts, or skill registry errors.
//! - **Telemetry Link**: Search [agentic_engine] in tracing logs.

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
use sqlx::Row;
use std::sync::Arc;

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
    super::receipts::ensure_status_ledger(&state.resources.pool, &agent_id).await?;

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

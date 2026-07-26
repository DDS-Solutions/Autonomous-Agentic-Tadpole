//! @docs ARCHITECTURE:Persistence
//!
//! ### AI Assist Note
//! **Core technical resource for the Tadpole OS Sovereign infrastructure.**
//! This module implements high-fidelity logic for the Sovereign Reality layer.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Runtime logic error, state desynchronization, or resource exhaustion.
//! - **Telemetry Link**: Search `[metadata]` in tracing logs.

use sqlx::{Row, SqlitePool};
use crate::error::AppError;
use super::super::types::{KnowledgeEntry, DEFAULT_TTL_DAYS};

pub const SECONDS_PER_DAY: i64 = 86_400;

/// Computes a SHA-256 hex hash of the given text for dedup and P2P idempotency.
pub fn sha256_hash(text: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    let result = hasher.finalize();
    hex::encode(result)
}

/// Computes the TTL unix timestamp for a new entry.
/// Q3 decision: agent default = 90d, human-confirmed = NULL (never).
pub fn compute_ttl(human_confirmed: bool, ttl_days: Option<i64>, now_unix: i64) -> Option<i64> {
    match (human_confirmed, ttl_days) {
        (true, _) => None,                                             // human-confirmed → never expires
        (false, Some(d)) => Some(now_unix + d * SECONDS_PER_DAY),     // caller-supplied
        (false, None) => Some(now_unix + DEFAULT_TTL_DAYS * SECONDS_PER_DAY), // agent default: 90d
    }
}

/// Decodes a SQLite row into a KnowledgeEntry struct.
pub fn entry_from_row(row: sqlx::sqlite::SqliteRow) -> Result<KnowledgeEntry, sqlx::Error> {
    Ok(KnowledgeEntry {
        id: row.try_get("id")?,
        text: row.try_get("text")?,
        topic: row.try_get("topic")?,
        cluster_id: row.try_get("cluster_id")?,
        source_node_id: row.try_get("source_node_id")?,
        source_agent_id: row.try_get("source_agent_id")?,
        content_hash: row.try_get("content_hash")?,
        confidence: row.try_get::<f64, _>("confidence")? as f32,
        human_confirmed: row.try_get::<i64, _>("human_confirmed")? != 0,
        ttl: row.try_get("ttl")?,
        created_at: row.try_get("created_at")?,
        access_count: row.try_get("access_count")?,
        concept_type: row.try_get("concept_type")?,
        title: row.try_get("title")?,
        description: row.try_get("description")?,
        resource_uri: row.try_get("resource_uri")?,
        tags: row.try_get("tags")?,
        constraints_json: row.try_get("constraints_json").ok().flatten(),
        provenance_chain: row.try_get("provenance_chain").ok().flatten(),
    })
}

/// Look up a single entry by ID. Bumps access count and access timestamp on hit.
pub async fn get_by_id(pool: &SqlitePool, id: &str) -> Result<Option<KnowledgeEntry>, AppError> {
    let row = sqlx::query(
        r#"SELECT id, text, content_hash, topic, cluster_id, source_node_id,
                  source_agent_id, confidence, ttl, human_confirmed,
                  created_at, access_count,
                  concept_type, title, description, resource_uri, tags,
                  constraints_json, provenance_chain
           FROM knowledge_store_meta
           WHERE id = ?"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|e| AppError::InternalServerError(format!("[IKS] get_by_id failed: {}", e)))?;

    let Some(row) = row else {
        return Ok(None);
    };

    let entry = entry_from_row(row).map_err(|e| {
        AppError::InternalServerError(format!("[IKS] row decode failed for id={}: {}", id, e))
    })?;

    // Asynchronously bump access count (fire-and-forget, non-blocking)
    let pool_clone = pool.clone();
    let id_string = id.to_string();
    tokio::spawn(async move {
        let _ = sqlx::query(
            "UPDATE knowledge_store_meta SET access_count = access_count + 1 WHERE id = ?",
        )
        .bind(&id_string)
        .execute(&pool_clone)
        .await;
    });

    Ok(Some(entry))
}

/// List entries by topic, cluster, or concept_type with pagination.
pub async fn list(
    pool: &SqlitePool,
    topic: Option<&str>,
    cluster_id: Option<&str>,
    concept_type: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<KnowledgeEntry>, AppError> {
    let rows = sqlx::query(
        r#"SELECT id, text, content_hash, topic, cluster_id, source_node_id,
                  source_agent_id, confidence, ttl, human_confirmed,
                  created_at, access_count,
                  concept_type, title, description, resource_uri, tags,
                  constraints_json, provenance_chain
           FROM knowledge_store_meta
           WHERE (? IS NULL OR topic = ?)
             AND (? IS NULL OR cluster_id = ? OR cluster_id IS NULL)
             AND (? IS NULL OR concept_type = ?)
             AND (ttl IS NULL OR ttl > unixepoch())
           ORDER BY confidence DESC
           LIMIT ? OFFSET ?"#,
    )
    .bind(topic)
    .bind(topic)
    .bind(cluster_id)
    .bind(cluster_id)
    .bind(concept_type)
    .bind(concept_type)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(|e| AppError::InternalServerError(format!("[IKS] list failed: {}", e)))?;

    rows.into_iter()
        .map(entry_from_row)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::InternalServerError(format!("[IKS] list decode failed: {}", e)))
}

/// Fetch entries created or updated since `since_unix`.
pub async fn get_entries_since(pool: &SqlitePool, since_unix: i64) -> Result<Vec<KnowledgeEntry>, AppError> {
    let rows = sqlx::query(
        r#"SELECT id, text, content_hash, topic, cluster_id, source_node_id,
                  source_agent_id, confidence, ttl, human_confirmed,
                  created_at, access_count,
                  concept_type, title, description, resource_uri, tags,
                  constraints_json, provenance_chain
           FROM knowledge_store_meta
           WHERE created_at >= ? OR updated_at >= ?"#,
    )
    .bind(since_unix)
    .bind(since_unix)
    .fetch_all(pool)
    .await
    .map_err(|e| AppError::InternalServerError(format!("[IKS] get_entries_since failed: {}", e)))?;

    rows.into_iter()
        .map(entry_from_row)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::InternalServerError(format!("[IKS] sync row decode failed: {}", e)))
}

/// Evicts entries whose TTL timestamp is in the past, EXCEPT human-confirmed entries.
pub async fn evict_expired(pool: &SqlitePool) -> Result<u64, AppError> {
    let now = chrono::Utc::now().timestamp();
    let res = sqlx::query(
        r#"DELETE FROM knowledge_store_meta
           WHERE human_confirmed = 0
             AND ttl IS NOT NULL
             AND ttl < ?"#,
    )
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| AppError::InternalServerError(format!("[IKS] TTL eviction failed: {}", e)))?;

    let count = res.rows_affected();
    if count > 0 {
        tracing::info!(evicted = count, "[IKS] TTL eviction complete");
    }
    Ok(count)
}

/// Confidence decay: reduce confidence based on actual time elapsed since last update.
pub async fn decay_confidence(pool: &SqlitePool) -> Result<(), AppError> {
    sqlx::query(
        r#"UPDATE knowledge_store_meta
           SET confidence = MAX(0.0, confidence - (0.01 * CAST((unixepoch() - updated_at) / 86400.0 AS REAL))),
               updated_at = unixepoch()
           WHERE human_confirmed = 0
             AND updated_at < unixepoch() - 86400"#,
    )
    .execute(pool)
    .await
    .map_err(|e| AppError::InternalServerError(format!("[IKS] Confidence decay failed: {}", e)))?;

    tracing::info!("[IKS] Confidence decay pass complete");
    Ok(())
}

/// Confirm or un-confirm an entry. Human confirmation freezes confidence at 1.0 and removes TTL.
pub async fn update_confirm_status(pool: &SqlitePool, id: &str, confirmed: bool) -> Result<(), AppError> {
    let now_unix = chrono::Utc::now().timestamp();

    if confirmed {
        sqlx::query(
            r#"UPDATE knowledge_store_meta
               SET human_confirmed = 1, confidence = 1.0, ttl = NULL, updated_at = ?
               WHERE id = ?"#,
        )
        .bind(now_unix)
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| AppError::InternalServerError(format!("[IKS] Confirm update failed: {}", e)))?;
    } else {
        let new_ttl = now_unix + DEFAULT_TTL_DAYS * SECONDS_PER_DAY;
        sqlx::query(
            r#"UPDATE knowledge_store_meta
               SET human_confirmed = 0, ttl = ?, updated_at = ?
               WHERE id = ?"#,
        )
        .bind(new_ttl)
        .bind(now_unix)
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| AppError::InternalServerError(format!("[IKS] Unconfirm update failed: {}", e)))?;
    }

    tracing::info!(id = %id, confirmed = confirmed, "[IKS] Confirm status updated");
    Ok(())
}

// Telemetry Tag duplicate reference: [metadata]

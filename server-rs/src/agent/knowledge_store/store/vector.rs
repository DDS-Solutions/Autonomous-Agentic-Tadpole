//! @docs ARCHITECTURE:Retrieval
//!
//! ### AI Assist Note
//! **Core technical resource for the Tadpole OS Sovereign infrastructure.**
//! This module implements high-fidelity logic for the Sovereign Reality layer.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Runtime logic error, state desynchronization, or resource exhaustion.
//! - **Telemetry Link**: Search `[vector]` in tracing logs.

#[cfg(feature = "vector-memory")]
use std::sync::Arc;
use sqlx::SqlitePool;
use crate::error::AppError;
use super::super::types::KnowledgeEntry;
use super::metadata;

/// Sanitizes an input identifier string before inserting into filter predicates.
pub fn sanitize_predicate_id(id: &str) -> String {
    id.chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .collect()
}

/// Finds similar knowledge entries (peers) based on vector distance, excluding the node itself.
pub async fn get_peers(
    pool: &SqlitePool,
    #[cfg(feature = "vector-memory")] lance_cell: &tokio::sync::OnceCell<Arc<crate::agent::memory::VectorMemory>>,
    google_api_key: Option<&str>,
    id: &str,
    limit: usize,
    http_client: reqwest::Client,
) -> Result<Vec<KnowledgeEntry>, AppError> {
    let entry = metadata::get_by_id(pool, id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("[IKS] Entry {} not found", id)))?;

    #[cfg(feature = "vector-memory")]
    {
        let api_key = google_api_key.ok_or_else(|| {
            AppError::InternalServerError(
                "[IKS] GOOGLE_API_KEY required for embedding. Set it in .env.".to_string(),
            )
        })?;
        let query_vector = crate::agent::memory::get_gemini_embedding(&http_client, api_key, &entry.text)
            .await?;

        let lance = lance_cell
            .get_or_try_init(|| async {
                let v = crate::agent::memory::VectorMemory::connect(
                    "data/iks/knowledge_store",
                    "knowledge_store",
                )
                .await?;
                Ok(Arc::new(v))
            })
            .await?;

        let _ = lance.ensure_table().await?;

        // Retrieve top peers (limit + 1 to account for self-exclusion)
        let safe_id = sanitize_predicate_id(id);
        let predicate = format!("id != '{}' AND confidence >= 0.0 AND (ttl IS NULL OR ttl > {})", safe_id, chrono::Utc::now().timestamp());
        let hits = lance
            .search_knowledge_filtered(query_vector, limit + 1, &predicate)
            .await?;

        if hits.is_empty() {
            return Ok(vec![]);
        }

        let hit_ids: Vec<String> = hits.iter().map(|h| h.id.clone()).collect();
        let placeholders = hit_ids.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
        let hydrate_sql = format!(
            r#"SELECT id, text, content_hash, topic, cluster_id, source_node_id,
                      source_agent_id, confidence, ttl, human_confirmed,
                      created_at, access_count,
                      concept_type, title, description, resource_uri, tags,
                      constraints_json, provenance_chain
               FROM knowledge_store_meta
               WHERE id IN ({})"#,
            placeholders
        );
        let mut q = sqlx::query(&hydrate_sql);
        for hit_id in &hit_ids {
            q = q.bind(hit_id);
        }
        let rows = q.fetch_all(pool).await.map_err(|e| {
            AppError::InternalServerError(format!("[IKS] peer hydration failed: {}", e))
        })?;

        let mut entry_map: std::collections::HashMap<String, KnowledgeEntry> = rows
            .into_iter()
            .map(metadata::entry_from_row)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| {
                AppError::InternalServerError(format!("[IKS] peer row decode failed: {}", e))
            })?
            .into_iter()
            .map(|e| (e.id.clone(), e))
            .collect();

        // Preserve nearest semantic distance ordering returned by LanceDB vector search
        let mut results = Vec::new();
        for hit_id in &hit_ids {
            if let Some(entry) = entry_map.remove(hit_id) {
                results.push(entry);
            }
        }

        // Limit results
        results.truncate(limit);
        Ok(results)
    }

    #[cfg(not(feature = "vector-memory"))]
    {
        let _ = (google_api_key, http_client);
        tracing::debug!(id = %id, "[IKS] get_peers called without vector-memory feature");
        Ok(vec![])
    }
}

// Telemetry Tag duplicate reference: [vector]

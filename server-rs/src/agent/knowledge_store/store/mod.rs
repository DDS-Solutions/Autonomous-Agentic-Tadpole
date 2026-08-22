//! @docs ARCHITECTURE:Persistence
//!
//! ### AI Assist Note
//! **Core technical resource for the Tadpole OS Sovereign infrastructure.**
//! This module implements high-fidelity logic for the Sovereign Reality layer.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Runtime logic error, state desynchronization, or resource exhaustion.
//! - **Telemetry Link**: Search `[mod]` in tracing logs.

pub mod metadata;
pub mod vector;
pub mod graph;

#[cfg(feature = "vector-memory")]
use std::sync::Arc;
use sqlx::{Row, SqlitePool};
use crate::error::AppError;
use super::types::{
    KnowledgeEntry, AddKnowledgeRequest, KnowledgeEdge, AddKnowledgeEdgeRequest,
    KnowledgeSynthesizeRequest, KnowledgeSynthesisResponse,
};

pub struct KnowledgeStore {
    pub(crate) pool: SqlitePool,
    pub(crate) privacy_mode: bool,
    pub(crate) google_api_key: Option<String>,
    #[cfg(feature = "vector-memory")]
    pub(crate) lance: tokio::sync::OnceCell<Arc<crate::agent::memory::VectorMemory>>,
}

impl KnowledgeStore {
    /// Creates a new KnowledgeStore backed by the given pool.
    /// Config environment variables are cached during initialization to prevent syscall bottlenecks.
    pub fn new(pool: SqlitePool) -> Self {
        let privacy_mode = std::env::var("PRIVACY_MODE")
            .map(|v| v.to_lowercase() == "true")
            .unwrap_or(false);
        let google_api_key = std::env::var("GOOGLE_API_KEY").ok();

        Self {
            pool,
            privacy_mode,
            google_api_key,
            #[cfg(feature = "vector-memory")]
            lance: tokio::sync::OnceCell::new(),
        }
    }

    /// Computes a SHA-256 hex hash of the given text.
    pub fn sha256_hash(text: &str) -> String {
        metadata::sha256_hash(text)
    }

    /// Computes the TTL unix timestamp for a new entry.
    pub fn compute_ttl(human_confirmed: bool, ttl_days: Option<i64>, now_unix: i64) -> Option<i64> {
        metadata::compute_ttl(human_confirmed, ttl_days, now_unix)
    }

    /// Lazily initializes and returns the LanceDB vector store.
    #[cfg(feature = "vector-memory")]
    pub(super) async fn get_lance(&self) -> Result<Arc<crate::agent::memory::VectorMemory>, AppError> {
        let lance = self
            .lance
            .get_or_try_init(|| async {
                let v = crate::agent::memory::VectorMemory::connect(
                    "data/iks/knowledge_store",
                    "knowledge_store",
                )
                .await?;
                Ok(Arc::new(v))
            })
            .await?;
        Ok(lance.clone())
    }

    /// Write a new knowledge entry. Coordinates dedup check -> vector embedding -> SQLite row insert -> LanceDB vector insert.
    pub async fn add_entry(
        &self,
        req: AddKnowledgeRequest,
        http_client: reqwest::Client,
    ) -> Result<KnowledgeEntry, AppError> {
        // ── Privacy guard ──────────────────────────────────────────────────
        if self.privacy_mode {
            tracing::warn!(
                topic = %req.topic,
                "[IKS] PRIVACY_MODE active — skipping knowledge store write"
            );
            return Err(AppError::InternalServerError(
                "[IKS] Writes require cloud embedding (PRIVACY_MODE=true)".to_string(),
            ));
        }

        // ── Dedup check ────────────────────────────────────────────────────
        let content_hash = metadata::sha256_hash(&req.text);
        let existing =
            sqlx::query("SELECT id FROM knowledge_store_meta WHERE content_hash = ? LIMIT 1")
                .bind(&content_hash)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| {
                    AppError::InternalServerError(format!("[IKS] Dedup check failed: {}", e))
                })?;

        if let Some(row) = existing {
            let existing_id: String = row.try_get("id").map_err(|e| {
                AppError::InternalServerError(format!("[IKS] Dedup row decode failed: {}", e))
            })?;
            tracing::debug!(id = %existing_id, "[IKS] Dedup hit — returning existing entry");
            return self
                .get_by_id(&existing_id)
                .await?
                .ok_or_else(|| AppError::NotFound("[IKS] Dedup entry vanished".to_string()));
        }

        // ── Compute embedding ──────────────────────────────────────────────
        #[cfg(feature = "vector-memory")]
        let vector = {
            let api_key = self.google_api_key.as_deref().ok_or_else(|| {
                AppError::InternalServerError(
                    "[IKS] GOOGLE_API_KEY required for embedding. Set it in .env.".to_string(),
                )
            })?;
            crate::agent::memory::get_gemini_embedding(&http_client, api_key, &req.text).await?
        };

        // ── Prepare metadata ───────────────────────────────────────────────
        let id = uuid::Uuid::new_v4().to_string();
        let now_unix = chrono::Utc::now().timestamp();
        let human_confirmed = req.human_confirmed.unwrap_or(false);
        let confidence = if human_confirmed {
            1.0_f32
        } else {
            req.confidence.unwrap_or(1.0).clamp(0.0, 1.0)
        };
        let ttl = metadata::compute_ttl(human_confirmed, req.ttl_days, now_unix);
        let human_confirmed_int: i64 = if human_confirmed { 1 } else { 0 };
        let topic = req.topic.to_lowercase();
        let concept_type = req.concept_type.unwrap_or_else(|| "general".to_string()).to_lowercase();

        // ── Insert SQLite metadata row ─────────────────────────────────────
        let insert_res = sqlx::query(
            r#"INSERT INTO knowledge_store_meta
               (id, text, content_hash, topic, cluster_id, source_node_id, source_agent_id,
                confidence, ttl, human_confirmed, created_at, updated_at,
                concept_type, title, description, resource_uri, tags,
                constraints_json, provenance_chain)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(&id)
        .bind(&req.text)
        .bind(&content_hash)
        .bind(&topic)
        .bind(&req.cluster_id)
        .bind(&req.source_node_id)
        .bind(&req.source_agent_id)
        .bind(confidence)
        .bind(ttl)
        .bind(human_confirmed_int)
        .bind(now_unix)
        .bind(now_unix)
        .bind(&concept_type)
        .bind(&req.title)
        .bind(&req.description)
        .bind(&req.resource_uri)
        .bind(&req.tags)
        .bind(&req.constraints_json)
        .bind(&req.provenance_chain)
        .execute(&self.pool)
        .await;

        if let Err(ref db_err) = insert_res {
            let err_str = db_err.to_string();
            if err_str.contains("UNIQUE constraint failed") || err_str.contains("content_hash") {
                tracing::info!(hash = %content_hash, "[IKS] Dedup collision on insert — retrieving existing entry");
                if let Ok(Some(row)) = sqlx::query("SELECT id FROM knowledge_store_meta WHERE content_hash = ? LIMIT 1")
                    .bind(&content_hash)
                    .fetch_optional(&self.pool)
                    .await
                {
                    let existing_id: String = row.try_get("id").unwrap_or_default();
                    if let Ok(Some(entry)) = self.get_by_id(&existing_id).await {
                        return Ok(entry);
                    }
                }
            }
            return Err(AppError::InternalServerError(format!("[IKS] SQLite insert failed: {}", db_err)));
        }

        // ── Insert LanceDB vector row ──────────────────────────────────────
        #[cfg(feature = "vector-memory")]
        {
            let lance = self.get_lance().await?;
            lance.ensure_table().await?;
            lance
                .add_memory(&id, &req.text, &topic, vector)
                .await
                .map_err(|e| {
                    AppError::InternalServerError(format!("[IKS] LanceDB insert failed: {}", e))
                })?;
        }
        #[cfg(not(feature = "vector-memory"))]
        let _ = http_client;

        tracing::info!(
            id = %id,
            topic = %topic,
            concept_type = %concept_type,
            "[IKS] Entry written successfully"
        );

        Ok(KnowledgeEntry {
            id,
            text: req.text,
            topic,
            cluster_id: req.cluster_id,
            source_node_id: req.source_node_id,
            source_agent_id: req.source_agent_id,
            content_hash,
            confidence,
            human_confirmed,
            ttl,
            created_at: now_unix,
            access_count: 0,
            concept_type,
            title: req.title,
            description: req.description,
            resource_uri: req.resource_uri,
            tags: req.tags,
            constraints_json: req.constraints_json,
            provenance_chain: req.provenance_chain,
        })
    }

    /// Decodes a SQLite row into a KnowledgeEntry struct.
    pub fn entry_from_row(row: sqlx::sqlite::SqliteRow) -> Result<KnowledgeEntry, sqlx::Error> {
        metadata::entry_from_row(row)
    }

    /// Look up a single entry by ID.
    pub async fn get_by_id(&self, id: &str) -> Result<Option<KnowledgeEntry>, AppError> {
        metadata::get_by_id(&self.pool, id).await
    }

    /// List entries by topic, cluster, or concept_type with pagination.
    pub async fn list(
        &self,
        topic: Option<&str>,
        cluster_id: Option<&str>,
        concept_type: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<KnowledgeEntry>, AppError> {
        metadata::list(&self.pool, topic, cluster_id, concept_type, limit, offset).await
    }

    /// Fetch entries created or updated since `since_unix`.
    pub async fn get_entries_since(&self, since_unix: i64) -> Result<Vec<KnowledgeEntry>, AppError> {
        metadata::get_entries_since(&self.pool, since_unix).await
    }

    /// Delete an entry by ID. Removes from LanceDB first, then SQLite.
    pub async fn delete(&self, id: &str) -> Result<(), AppError> {
        #[cfg(feature = "vector-memory")]
        {
            if let Ok(lance) = self.get_lance().await {
                if let Err(e) = lance.delete_memories(vec![id.to_string()]).await {
                    tracing::warn!(id = %id, error = %e, "[IKS] LanceDB delete warning before SQLite metadata removal");
                }
            }
        }

        sqlx::query("DELETE FROM knowledge_store_meta WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| {
                AppError::InternalServerError(format!("[IKS] delete SQLite failed: {}", e))
            })?;

        tracing::info!(id = %id, "[IKS] Entry deleted");
        Ok(())
    }

    /// Alias for delete for route handler compatibility.
    pub async fn remove(&self, id: &str, _force: bool) -> Result<(), AppError> {
        self.delete(id).await
    }

    /// Evicts expired entries.
    pub async fn evict_expired(&self) -> Result<u64, AppError> {
        metadata::evict_expired(&self.pool).await
    }

    /// Confidence decay pass.
    pub async fn decay_confidence(&self) -> Result<(), AppError> {
        metadata::decay_confidence(&self.pool).await
    }

    /// Update confirm status.
    pub async fn update_confirm_status(&self, id: &str, confirmed: bool) -> Result<(), AppError> {
        metadata::update_confirm_status(&self.pool, id, confirmed).await
    }

    /// Confirm entry helper.
    pub async fn confirm(&self, id: &str) -> Result<(), AppError> {
        self.update_confirm_status(id, true).await
    }

    /// Finds similar knowledge entries (peers) based on vector distance.
    pub async fn get_peers(
        &self,
        id: &str,
        limit: usize,
        http_client: reqwest::Client,
    ) -> Result<Vec<KnowledgeEntry>, AppError> {
        #[cfg(feature = "vector-memory")]
        {
            vector::get_peers(
                &self.pool,
                &self.lance,
                self.google_api_key.as_deref(),
                id,
                limit,
                http_client,
            )
            .await
        }
        #[cfg(not(feature = "vector-memory"))]
        {
            vector::get_peers(
                &self.pool,
                self.google_api_key.as_deref(),
                id,
                limit,
                http_client,
            )
            .await
        }
    }

    /// Add a graph edge.
    pub async fn add_edge(&self, req: AddKnowledgeEdgeRequest) -> Result<KnowledgeEdge, AppError> {
        graph::add_edge(&self.pool, req).await
    }

    /// List graph edges with optional filters.
    pub async fn list_edges(
        &self,
        source_id: Option<&str>,
        target_id: Option<&str>,
    ) -> Result<Vec<KnowledgeEdge>, AppError> {
        graph::list_edges(&self.pool, source_id, target_id).await
    }

    /// Synthesize knowledge entries into a composite concept node.
    pub async fn synthesize(
        &self,
        req: KnowledgeSynthesizeRequest,
        http_client: reqwest::Client,
    ) -> Result<KnowledgeSynthesisResponse, AppError> {
        graph::synthesize(&self.pool, req, |add_req| self.add_entry(add_req, http_client)).await
    }
}

// Telemetry Tag duplicate reference: [store]

// Metadata: [mod]

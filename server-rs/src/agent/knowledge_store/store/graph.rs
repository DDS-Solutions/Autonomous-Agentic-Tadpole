//! @docs ARCHITECTURE:Intelligence
//!
//! ### AI Assist Note
//! **Core technical resource for the Tadpole OS Sovereign infrastructure.**
//! This module implements high-fidelity logic for the Sovereign Reality layer.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Runtime logic error, state desynchronization, or resource exhaustion.
//! - **Telemetry Link**: Search `[graph]` in tracing logs.

use sqlx::{Row, SqlitePool};
use crate::error::AppError;
use super::super::types::{
    KnowledgeEdge, AddKnowledgeEdgeRequest, KnowledgeSynthesizeRequest,
    KnowledgeSynthesisResponse, AddKnowledgeRequest,
};
use super::metadata;

/// Add a typed directed relational edge between two knowledge nodes.
pub async fn add_edge(pool: &SqlitePool, req: AddKnowledgeEdgeRequest) -> Result<KnowledgeEdge, AppError> {
    let id = uuid::Uuid::new_v4().to_string();
    let weight = req.weight.unwrap_or(1.0).clamp(0.0, 1.0);
    let now_unix = chrono::Utc::now().timestamp();

    sqlx::query(
        r#"INSERT INTO knowledge_edges (id, source_id, target_id, relation_type, weight, created_at)
           VALUES (?, ?, ?, ?, ?, ?)"#,
    )
    .bind(&id)
    .bind(&req.source_id)
    .bind(&req.target_id)
    .bind(&req.relation_type)
    .bind(weight)
    .bind(now_unix)
    .execute(pool)
    .await
    .map_err(|e| AppError::InternalServerError(format!("[IKS] add_edge SQLite failed: {}", e)))?;

    Ok(KnowledgeEdge {
        id,
        source_id: req.source_id,
        target_id: req.target_id,
        relation_type: req.relation_type,
        weight,
        created_at: now_unix,
    })
}

/// List knowledge graph edges by source or target node ID filters.
pub async fn list_edges(
    pool: &SqlitePool,
    source_id: Option<&str>,
    target_id: Option<&str>,
) -> Result<Vec<KnowledgeEdge>, AppError> {
    let rows = sqlx::query(
        r#"SELECT id, source_id, target_id, relation_type, weight, created_at
           FROM knowledge_edges
           WHERE (? IS NULL OR source_id = ?)
             AND (? IS NULL OR target_id = ?)
           ORDER BY created_at DESC"#,
    )
    .bind(source_id)
    .bind(source_id)
    .bind(target_id)
    .bind(target_id)
    .fetch_all(pool)
    .await
    .map_err(|e| AppError::InternalServerError(format!("[IKS] list_edges failed: {}", e)))?;

    let mut edges = Vec::new();
    for row in rows {
        edges.push(KnowledgeEdge {
            id: row.try_get("id")?,
            source_id: row.try_get("source_id")?,
            target_id: row.try_get("target_id")?,
            relation_type: row.try_get("relation_type")?,
            weight: row.try_get::<f64, _>("weight")? as f32,
            created_at: row.try_get("created_at")?,
        });
    }
    Ok(edges)
}

/// Synthesize cross-agent knowledge entries into a higher-order concept node (OKF v0.3).
pub async fn synthesize<F, Fut>(
    pool: &SqlitePool,
    req: KnowledgeSynthesizeRequest,
    add_entry_fn: F,
) -> Result<KnowledgeSynthesisResponse, AppError>
where
    F: FnOnce(AddKnowledgeRequest) -> Fut,
    Fut: std::future::Future<Output = Result<super::super::types::KnowledgeEntry, AppError>>,
{
    if req.source_ids.is_empty() {
        return Err(AppError::BadRequest("[IKS] Synthesis requires at least one source_id".to_string()));
    }

    let mut composite_text = String::new();
    for sid in &req.source_ids {
        if let Some(entry) = metadata::get_by_id(pool, sid).await? {
            composite_text.push_str(&format!("--- Node {}\n{}\n\n", entry.id, entry.text));
        }
    }

    let concept_type = req.concept_type.unwrap_or_else(|| "composite_concept".to_string());
    let provenance = serde_json::json!({
        "source_ids": req.source_ids,
        "engine": "ollama",
        "synthesized_at": chrono::Utc::now().timestamp()
    }).to_string();

    let add_req = AddKnowledgeRequest {
        text: composite_text,
        topic: req.topic,
        cluster_id: None,
        source_node_id: None,
        source_agent_id: Some("synth_engine_ollama".to_string()),
        confidence: Some(1.0),
        ttl_days: None,
        human_confirmed: Some(false),
        concept_type: Some(concept_type),
        title: Some(req.title),
        description: Some("Auto-synthesized cross-agent knowledge entry".to_string()),
        resource_uri: None,
        tags: Some("synthesized,okf_v03".to_string()),
        constraints_json: None,
        provenance_chain: Some(provenance),
    };

    let synthesized_entry = add_entry_fn(add_req).await?;
    let mut edges_created = 0;
    let now_unix = chrono::Utc::now().timestamp();

    let mut tx = pool.begin().await.map_err(|e| {
        AppError::InternalServerError(format!("[IKS] Failed to begin transaction for synthesis: {}", e))
    })?;

    for sid in &req.source_ids {
        let edge_id = uuid::Uuid::new_v4().to_string();
        let res = sqlx::query(
            r#"INSERT INTO knowledge_edges (id, source_id, target_id, relation_type, weight, created_at)
               VALUES (?, ?, ?, 'derives_from', 0.9, ?)"#,
        )
        .bind(&edge_id)
        .bind(&synthesized_entry.id)
        .bind(sid)
        .bind(now_unix)
        .execute(&mut *tx)
        .await;

        if res.is_ok() {
            edges_created += 1;
        }
    }

    tx.commit().await.map_err(|e| {
        AppError::InternalServerError(format!("[IKS] Failed to commit synthesis transaction: {}", e))
    })?;

    Ok(KnowledgeSynthesisResponse {
        synthesized_entry,
        edges_created,
        contradiction_warning: None,
    })
}

// Telemetry Tag duplicate reference: [graph]

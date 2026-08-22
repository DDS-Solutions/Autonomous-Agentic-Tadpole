//! @docs ARCHITECTURE:Gateways
//!
//! ### AI Assist Note
//! **Intelligence Layer Routes — Code Graph & Blast Radius Analysis**
//! This module implements high-fidelity logic for the Sovereign Reality layer.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Runtime logic error, state desynchronization, or resource exhaustion.
//! - **Telemetry Link**: Search `[intelligence]` in tracing logs.

use crate::error::AppError;
use crate::intelligence::graph::SymbolNode;
use crate::intelligence::service::{
    GraphRebuildResponse, GraphResponse, IntelligenceService, ResolveResponse,
};
use crate::state::AppState;
use axum::{
    extract::{Query, State},
    Json,
};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Deserialize)]
pub struct BlastRadiusQuery {
    pub name: String,
    pub path: String,
    pub limit: Option<usize>,
}

#[derive(Deserialize)]
pub struct CodeGraphQuery {
    pub path_prefix: Option<String>,
    pub max_nodes: Option<usize>,
}

#[derive(Deserialize)]
pub struct RebuildQuery {
    pub dry_run: Option<bool>,
}

#[derive(Deserialize)]
pub struct ResolveQuery {
    pub name: String,
    pub path: String,
    pub budget: Option<usize>,
}

/// GET /v1/intelligence/graph
///
/// Returns the full high-fidelity symbol graph for visualization.
pub async fn get_code_graph(
    State(state): State<Arc<AppState>>,
    Query(query): Query<CodeGraphQuery>,
) -> Result<Json<GraphResponse>, AppError> {
    let service = IntelligenceService::new(state);
    let res = service.list_graph(query.path_prefix.clone(), query.max_nodes).await?;
    Ok(Json(res))
}

/// GET /v1/intelligence/blast-radius
///
/// Calculates the downstream impact of changing a specific symbol.
pub async fn get_blast_radius(
    State(state): State<Arc<AppState>>,
    Query(query): Query<BlastRadiusQuery>,
) -> Result<Json<Vec<SymbolNode>>, AppError> {
    let service = IntelligenceService::new(state);
    let res = service.blast_radius(&query.name, &query.path, query.limit).await?;
    Ok(Json(res))
}

/// POST /v1/intelligence/graph/rebuild
///
/// Rebuilds the symbol-level knowledge graph from the workspace files.
pub async fn rebuild_code_graph(
    State(state): State<Arc<AppState>>,
    Query(query): Query<RebuildQuery>,
) -> Result<Json<GraphRebuildResponse>, AppError> {
    let service = IntelligenceService::new(state);
    let dry_run = query.dry_run.unwrap_or(false);
    let res = service.rebuild_graph(dry_run).await?;
    Ok(Json(res))
}

/// GET /v1/intelligence/resolve
///
/// Resolves dependent symbols for a given symbol within a token budget constraint.
pub async fn resolve_code_context(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ResolveQuery>,
) -> Result<Json<ResolveResponse>, AppError> {
    let service = IntelligenceService::new(state);
    let budget = query.budget.unwrap_or(4000);
    let res = service.resolve_context(&query.name, &query.path, budget).await?;
    Ok(Json(res))
}

// Metadata: [intelligence]

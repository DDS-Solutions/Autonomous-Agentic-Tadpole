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
use crate::intelligence::graph::{CodeSymbolGraph, SymbolNode};
use crate::state::AppState;
use axum::{
    extract::{Query, State},
    Json,
};
use serde::Deserialize;
use std::sync::Arc;

use parking_lot::RwLock;

async fn get_built_symbol_graph(
    state: &Arc<AppState>,
) -> Result<Arc<RwLock<CodeSymbolGraph>>, AppError> {
    Ok(state.resources.get_symbol_graph().await)
}

#[derive(Deserialize)]
pub struct BlastRadiusQuery {
    pub name: String,
    pub path: String,
}

#[derive(Deserialize)]
pub struct CodeGraphQuery {
    pub path_prefix: Option<String>,
}

/// GET /v1/intelligence/graph
///
/// Returns the full high-fidelity symbol graph for visualization.
pub async fn get_code_graph(
    State(state): State<Arc<AppState>>,
    Query(query): Query<CodeGraphQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let graph_lock = get_built_symbol_graph(&state).await?;
    let lock_clone = Arc::clone(&graph_lock);
    let path_prefix = query.path_prefix.clone();

    let (nodes, edges, anomalies) = tokio::task::spawn_blocking(move || {
        let guard = lock_clone.read();

        let mut nodes = Vec::new();
        let mut edges = Vec::new();

        // 1. Filter nodes based on optional path prefix
        for idx in guard.graph.node_indices() {
            if let Some(node) = guard.graph.node_weight(idx) {
                let matches = if let Some(ref prefix) = path_prefix {
                    let real_path = guard.obfuscated_to_real_path.get(&node.path)
                        .map(|p| p.as_str())
                        .unwrap_or(&node.path);
                    real_path.starts_with(prefix) || node.path.starts_with(prefix)
                } else {
                    true
                };

                if matches {
                    nodes.push(node.clone());
                }
            }
        }

        // 2. Filter edges: only include if both source and target nodes exist in the filtered node set
        let filtered_keys: std::collections::HashSet<String> = nodes
            .iter()
            .map(|n| format!("{}:{}", n.path, n.name))
            .collect();

        use petgraph::visit::EdgeRef;
        for edge in guard.graph.edge_references() {
            let source = &guard.graph[edge.source()];
            let target = &guard.graph[edge.target()];
            let src_key = format!("{}:{}", source.path, source.name);
            let tgt_key = format!("{}:{}", target.path, target.name);

            if filtered_keys.contains(&src_key) && filtered_keys.contains(&tgt_key) {
                edges.push(serde_json::json!({
                    "source": src_key,
                    "target": tgt_key,
                }));
            }
        }

        let anomalies = guard.find_anomalies();
        let filtered_anomalies = if let Some(ref prefix) = path_prefix {
            anomalies
                .into_iter()
                .filter(|a| a.contains(prefix))
                .collect()
        } else {
            anomalies
        };

        (nodes, edges, filtered_anomalies)
    })
    .await
    .map_err(|e| {
        AppError::InternalServerError(format!("Graph processing thread panicked: {}", e))
    })?;

    Ok(Json(serde_json::json!({
        "nodes": nodes,
        "links": edges,
        "anomalies": anomalies,
    })))
}

/// GET /v1/intelligence/blast-radius
///
/// Calculates the downstream impact of changing a specific symbol.
pub async fn get_blast_radius(
    State(state): State<Arc<AppState>>,
    Query(query): Query<BlastRadiusQuery>,
) -> Result<Json<Vec<SymbolNode>>, AppError> {
    // 🛡️ [Path Traversal Hardening] Verify input resides within workspace boundary
    let workspace_root = &state.resources.base_dir;

    let graph_lock = get_built_symbol_graph(&state).await?;
    let lock_clone = Arc::clone(&graph_lock);
    let query_path = query.path.clone();
    let query_name = query.name.clone();
    let workspace_root_clone = workspace_root.clone();

    let affected = tokio::task::spawn_blocking(move || {
        let guard = lock_clone.read();

        // Reverse-resolve the physical raw path from the obfuscated path sent by the frontend client (O(1) lookup!)
        let raw_path = guard.obfuscated_to_real_path.get(&query_path)
            .ok_or_else(|| AppError::Forbidden("Invalid path boundary: potential path traversal detected".to_string()))?;

        // 🛡️ [Path Traversal Hardening] Verify resolved path resides within workspace boundary
        if crate::utils::security::validate_path(&workspace_root_clone, raw_path).is_err() {
            return Err(AppError::Forbidden("Invalid path boundary: potential path traversal detected".to_string()));
        }

        let affected = guard.calculate_blast_radius(&query_name, raw_path);
        Ok::<_, AppError>(affected)
    })
    .await
    .map_err(|e| {
        AppError::InternalServerError(format!("Blast radius processing thread panicked: {}", e))
    })??;

    Ok(Json(affected))
}

/// POST /v1/intelligence/graph/rebuild
///
/// Rebuilds the symbol-level knowledge graph from the workspace files.
pub async fn rebuild_code_graph(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let graph_lock = state.resources.get_symbol_graph().await;
    let salt = state.resources.obfuscation_salt.clone();
    let lock_clone = Arc::clone(&graph_lock);

    // 1. Copy config, repository, and root inside a brief read lock
    let (root, config, repository) = {
        let guard = lock_clone.read();
        (guard.root.clone(), guard.config.clone(), guard.repository.clone())
    };

    // 2. Perform the CPU-intensive rebuild on a temporary new graph instance
    // (no reader locks are held during this step!)
    tokio::task::spawn_blocking(move || -> Result<(), AppError> {
        let mut temp_graph = CodeSymbolGraph {
            graph: petgraph::graph::DiGraph::new(),
            index: std::collections::HashMap::new(),
            obfuscated_to_real_path: std::collections::HashMap::new(),
            repository,
            config,
            root,
        };
        temp_graph.build(&salt)?;

        // 3. Swap the built graph into the live lock using a brief write lock
        {
            let mut guard = lock_clone.write();
            *guard = temp_graph;
        }
        Ok(())
    })
    .await
    .map_err(|e| AppError::InternalServerError(format!("Graph rebuild thread panicked: {}", e)))??;

    Ok(Json(serde_json::json!({
        "status": "success",
        "message": "Symbol-level knowledge graph rebuilt successfully."
    })))
}

#[derive(Deserialize)]
pub struct ResolveQuery {
    pub name: String,
    pub path: String,
    pub budget: Option<usize>,
}

/// GET /v1/intelligence/resolve
///
/// Resolves dependent symbols for a given symbol within a token budget constraint.
pub async fn resolve_code_context(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ResolveQuery>,
) -> Result<Json<Vec<SymbolNode>>, AppError> {
    // 🛡️ [Path Traversal Hardening] Verify input resides within workspace boundary
    let workspace_root = &state.resources.base_dir;

    let graph_lock = get_built_symbol_graph(&state).await?;
    let lock_clone = Arc::clone(&graph_lock);
    let query_path = query.path.clone();
    let query_name = query.name.clone();
    let budget = query.budget.unwrap_or(4000);
    let workspace_root_clone = workspace_root.clone();

    let resolved = tokio::task::spawn_blocking(move || {
        let guard = lock_clone.read();

        // Reverse-resolve the physical raw path from the obfuscated path using O(1) index
        let raw_path = guard.obfuscated_to_real_path.get(&query_path)
            .ok_or_else(|| AppError::Forbidden("Invalid path boundary: potential path traversal detected".to_string()))?;

        // 🛡️ [Path Traversal Hardening] Verify resolved path resides within workspace boundary
        if crate::utils::security::validate_path(&workspace_root_clone, raw_path).is_err() {
            return Err(AppError::Forbidden("Invalid path boundary: potential path traversal detected".to_string()));
        }

        let resolved = guard.resolve_context(&query_name, raw_path, budget);
        Ok::<_, AppError>(resolved)
    })
    .await
    .map_err(|e| AppError::InternalServerError(format!("Context resolution thread panicked: {}", e)))??;

    Ok(Json(resolved))
}

// Metadata: [intelligence]

/*
### AI Assist Note
**🛡️ Tadpole OS: Intelligence**
Core system module providing specialized functionality for the agent swarm.

### 🔍 Debugging & Observability
- **Failure Path**: Unexpected execution drift or type compatibility issues.
- **Telemetry Link**: Traced via active system logging channels.
*/

//! Intelligence Layer Routes — Code Graph & Blast Radius Analysis
//!
//! @docs ARCHITECTURE:Intelligence
//!
//! ### AI Assist Note
//! **Intelligence Router**: Provides RESTful access to the system's 
//! semantic knowledge graph. Enables the frontend to visualize code 
//! interdependencies and perform real-time impact analysis (MOD-03).

use axum::{
    extract::{Query, State},
    Json,
};
use std::sync::Arc;
use serde::Deserialize;
use crate::state::AppState;
use crate::error::AppError;
use crate::intelligence::graph::{SymbolNode, obfuscate_path};

#[derive(Deserialize)]
pub struct BlastRadiusQuery {
    pub name: String,
    pub path: String,
}

/// [GET] /v1/intelligence/graph
/// Returns the full high-fidelity symbol graph for visualization.
pub async fn get_code_graph(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let graph_lock = state.resources.get_symbol_graph().await;
    let salt = state.resources.obfuscation_salt.clone();
    let lock_clone = Arc::clone(&graph_lock);

    let (nodes, edges, anomalies) = tokio::task::spawn_blocking(move || {
        let guard = lock_clone.read();
        
        let mut nodes = Vec::new();
        let mut edges = Vec::new();

        for idx in guard.graph.node_indices() {
            if let Some(node) = guard.graph.node_weight(idx) {
                let mut node_clone = node.clone();
                node_clone.path = obfuscate_path(&node_clone.path, &salt);
                nodes.push(node_clone);
            }
        }

        use petgraph::visit::EdgeRef;
        for edge in guard.graph.edge_references() {
            let source = &guard.graph[edge.source()];
            let target = &guard.graph[edge.target()];
            edges.push(serde_json::json!({
                "source": format!("{}:{}", obfuscate_path(&source.path, &salt), source.name),
                "target": format!("{}:{}", obfuscate_path(&target.path, &salt), target.name),
            }));
        }

        let mut anomalies = Vec::new();
        for anomaly in guard.find_anomalies() {
            if let Some(pos) = anomaly.rfind(" in ") {
                let prefix = &anomaly[..pos + 4];
                let raw_path = &anomaly[pos + 4..];
                let obf = obfuscate_path(raw_path, &salt);
                anomalies.push(format!("{}{}", prefix, obf));
            } else {
                anomalies.push(anomaly);
            }
        }

        (nodes, edges, anomalies)
    })
    .await
    .map_err(|e| AppError::InternalServerError(format!("Graph processing thread panicked: {}", e)))?;

    Ok(Json(serde_json::json!({
        "nodes": nodes,
        "links": edges,
        "anomalies": anomalies,
    })))
}

/// [GET] /v1/intelligence/blast-radius
/// Calculates the downstream impact of changing a specific symbol.
pub async fn get_blast_radius(
    State(state): State<Arc<AppState>>,
    Query(query): Query<BlastRadiusQuery>,
) -> Result<Json<Vec<SymbolNode>>, AppError> {
    // 🛡️ [Path Traversal Hardening] Verify input resides within workspace boundary
    let workspace_root = &state.resources.base_dir;

    let graph_lock = state.resources.get_symbol_graph().await;
    let salt = state.resources.obfuscation_salt.clone();
    let lock_clone = Arc::clone(&graph_lock);
    let query_path = query.path.clone();
    let query_name = query.name.clone();
    let workspace_root_clone = workspace_root.clone();

    let obfuscated_affected = tokio::task::spawn_blocking(move || {
        let guard = lock_clone.read();
        
        // Reverse-resolve the physical raw path from the obfuscated path using O(1) index
        let raw_path = guard.reverse_obfuscation_index.get(&query_path)
            .ok_or_else(|| AppError::Forbidden("Invalid path boundary: potential path traversal detected".to_string()))?;

        // 🛡️ [Path Traversal Hardening] Verify resolved path resides within workspace boundary
        if crate::utils::security::validate_path(&workspace_root_clone, raw_path).is_err() {
            return Err(AppError::Forbidden("Invalid path boundary: potential path traversal detected".to_string()));
        }

        let affected = guard.calculate_blast_radius(&query_name, raw_path);
        
        // Obfuscate target paths returned in the final impact list
        let mut obfuscated_affected = Vec::new();
        for node in affected {
            let mut node_clone = node.clone();
            node_clone.path = obfuscate_path(&node_clone.path, &salt);
            obfuscated_affected.push(node_clone);
        }
        Ok::<_, AppError>(obfuscated_affected)
    })
    .await
    .map_err(|e| AppError::InternalServerError(format!("Blast radius processing thread panicked: {}", e)))??;

    Ok(Json(obfuscated_affected))
}

/// [POST] /v1/intelligence/graph/rebuild
/// Rebuilds the symbol-level knowledge graph from the workspace files.
pub async fn rebuild_code_graph(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let graph_lock = state.resources.get_symbol_graph().await;
    let salt = state.resources.obfuscation_salt.clone();
    let lock_clone = Arc::clone(&graph_lock);
    let root = state.resources.base_dir.clone();

    // 1. Scan and parse workspace files outside the lock
    let parsed_files = tokio::task::spawn_blocking(move || {
        crate::intelligence::graph::CodeSymbolGraph::scan_workspace(&root)
    })
    .await
    .map_err(|e| AppError::InternalServerError(format!("Workspace scanning thread panicked: {}", e)))?;

    // 2. Rebuild the graph in-memory under the write lock
    tokio::task::spawn_blocking(move || {
        let mut guard = lock_clone.write();
        guard.build_from_parsed(parsed_files, &salt);
    })
    .await
    .map_err(|e| AppError::InternalServerError(format!("Graph rebuild thread panicked: {}", e)))?;

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

/// [GET] /v1/intelligence/resolve
/// Resolves dependent symbols for a given symbol within a token budget constraint.
pub async fn resolve_code_context(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ResolveQuery>,
) -> Result<Json<Vec<SymbolNode>>, AppError> {
    // 🛡️ [Path Traversal Hardening] Verify input resides within workspace boundary
    let workspace_root = &state.resources.base_dir;

    let graph_lock = state.resources.get_symbol_graph().await;
    let salt = state.resources.obfuscation_salt.clone();
    let lock_clone = Arc::clone(&graph_lock);
    let query_path = query.path.clone();
    let query_name = query.name.clone();
    let budget = query.budget.unwrap_or(4000);
    let workspace_root_clone = workspace_root.clone();

    let obfuscated_resolved = tokio::task::spawn_blocking(move || {
        let guard = lock_clone.read();
        
        // Reverse-resolve the physical raw path from the obfuscated path using O(1) index
        let raw_path = guard.reverse_obfuscation_index.get(&query_path)
            .ok_or_else(|| AppError::Forbidden("Invalid path boundary: potential path traversal detected".to_string()))?;

        // 🛡️ [Path Traversal Hardening] Verify resolved path resides within workspace boundary
        if crate::utils::security::validate_path(&workspace_root_clone, raw_path).is_err() {
            return Err(AppError::Forbidden("Invalid path boundary: potential path traversal detected".to_string()));
        }

        let resolved = guard.resolve_context(&query_name, raw_path, budget);
        
        // Obfuscate target paths returned in the final impact list
        let mut obfuscated_resolved = Vec::new();
        for node in resolved {
            let mut node_clone = node.clone();
            node_clone.path = obfuscate_path(&node_clone.path, &salt);
            obfuscated_resolved.push(node_clone);
        }
        Ok::<_, AppError>(obfuscated_resolved)
    })
    .await
    .map_err(|e| AppError::InternalServerError(format!("Context resolution thread panicked: {}", e)))??;

    Ok(Json(obfuscated_resolved))
}

// Metadata: [intelligence]

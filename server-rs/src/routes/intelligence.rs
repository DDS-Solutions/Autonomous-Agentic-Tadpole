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
use crate::intelligence::graph::SymbolNode;
use std::path::Path;
use sha2::{Sha256, Digest};

#[derive(Deserialize)]
pub struct BlastRadiusQuery {
    pub name: String,
    pub path: String,
}

/// Helper to obfuscate physical file path structures deterministically
/// while preserving UX force-graph clustering and file basenames.
fn obfuscate_path(path_str: &str, salt: &str) -> String {
    let path = Path::new(path_str);
    let file_name = path.file_name().and_then(|f| f.to_str()).unwrap_or("unknown");
    let parent = path.parent().unwrap_or(Path::new("")).to_string_lossy();
    
    if parent.is_empty() {
        file_name.to_string()
    } else {
        let mut hasher = Sha256::new();
        hasher.update(salt.as_bytes());
        hasher.update(parent.as_bytes());
        let result = hasher.finalize();
        let hash_val = hex::encode(result);
        format!("{}/{}", &hash_val[..16], file_name)
    }
}

/// [GET] /v1/intelligence/graph
/// Returns the full high-fidelity symbol graph for visualization.
pub async fn get_code_graph(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let graph_lock = state.resources.get_symbol_graph().await;
    
    // 🕒 [Optimistic Double-Checked Locking] Acquire read lock first to minimize contention
    let is_empty = {
        let guard = graph_lock.read();
        guard.index.is_empty()
    };

    if is_empty {
        let lock_clone = Arc::clone(&graph_lock);
        // 🕒 [Thread-Starvation Guard] Offload CPU/disk-bound compilation to blocking pool
        tokio::task::spawn_blocking(move || {
            let mut graph = lock_clone.write();
            if graph.index.is_empty() {
                graph.build();
            }
        })
        .await
        .map_err(|e| AppError::InternalServerError(format!("Graph build thread panicked: {}", e)))?;
    }

    let salt = state.resources.obfuscation_salt.clone();
    let lock_clone = Arc::clone(&graph_lock);

    let (nodes, edges) = tokio::task::spawn_blocking(move || {
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

        (nodes, edges)
    })
    .await
    .map_err(|e| AppError::InternalServerError(format!("Graph processing thread panicked: {}", e)))?;

    Ok(Json(serde_json::json!({
        "nodes": nodes,
        "links": edges,
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
    
    // Convert Windows backward slashes to forward slashes for unified traversal protection
    let normalized_path = query.path.replace('\\', "/");
    let combined = workspace_root.join(&normalized_path);
    
    let is_safe = if let Ok(canonical) = combined.canonicalize() {
        canonical.starts_with(workspace_root)
    } else {
        // Fallback boundary validation for raw query values
        !normalized_path.contains("..") && !normalized_path.starts_with('/') && !normalized_path.contains(':')
    };

    if !is_safe {
        return Err(AppError::BadRequest("Invalid path boundary: potential path traversal detected".to_string()));
    }

    let graph_lock = state.resources.get_symbol_graph().await;
    let salt = state.resources.obfuscation_salt.clone();
    let lock_clone = Arc::clone(&graph_lock);
    let query_path = query.path.clone();
    let query_name = query.name.clone();

    let obfuscated_affected = tokio::task::spawn_blocking(move || {
        let guard = lock_clone.read();
        
        // Reverse-resolve the physical raw path from the obfuscated path sent by the frontend client
        let mut raw_path = query_path.clone();
        for node in guard.graph.node_weights() {
            if obfuscate_path(&node.path, &salt) == query_path {
                raw_path = node.path.clone();
                break;
            }
        }

        let affected = guard.calculate_blast_radius(&query_name, &raw_path);
        
        // Obfuscate target paths returned in the final impact list
        let mut obfuscated_affected = Vec::new();
        for node in affected {
            let mut node_clone = node.clone();
            node_clone.path = obfuscate_path(&node_clone.path, &salt);
            obfuscated_affected.push(node_clone);
        }
        obfuscated_affected
    })
    .await
    .map_err(|e| AppError::InternalServerError(format!("Blast radius processing thread panicked: {}", e)))?;

    Ok(Json(obfuscated_affected))
}

// Metadata: [intelligence]

// Metadata: [intelligence]

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
    
    // Ensure graph is built
    {
        let mut graph = graph_lock.write();
        if graph.index.is_empty() {
            graph.build();
        }
    }

    let guard = graph_lock.read();
    
    // Export nodes and edges for the frontend force-graph
    let mut nodes = Vec::new();
    let mut edges = Vec::new();

    for idx in guard.graph.node_indices() {
        if let Some(node) = guard.graph.node_weight(idx) {
            nodes.push(node.clone());
        }
    }

    use petgraph::visit::EdgeRef;
    for edge in guard.graph.edge_references() {
        let source = &guard.graph[edge.source()];
        let target = &guard.graph[edge.target()];
        edges.push(serde_json::json!({
            "source": format!("{}:{}", source.path, source.name),
            "target": format!("{}:{}", target.path, target.name),
        }));
    }

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
    let graph_lock = state.resources.get_symbol_graph().await;
    let guard = graph_lock.read();
    
    let affected = guard.calculate_blast_radius(&query.name, &query.path);
    Ok(Json(affected))
}

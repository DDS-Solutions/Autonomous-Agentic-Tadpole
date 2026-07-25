//! @docs ARCHITECTURE:Core
//!
//! ### AI Assist Note
//! **Intelligence Service**: Exposes business logic for codebase querying,
//! including BFS dependency resolution and blast-radius tracking.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Timeout on large queries, missing paths in obfuscation map.
//! - **Telemetry Link**: Search `[intelligence]` in tracing logs.
//!

use crate::error::AppError;
use crate::intelligence::graph::{CodeSymbolGraph, SymbolNode};
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::time::timeout;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeGraphLink {
    pub source: String,
    pub target: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphResponse {
    pub nodes: Vec<SymbolNode>,
    pub links: Vec<CodeGraphLink>,
    pub anomalies: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RebuildSummary {
    pub node_count: usize,
    pub edge_count: usize,
    pub file_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphRebuildResponse {
    pub status: String,
    pub dry_run: bool,
    pub summary: Option<RebuildSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolveResponse {
    pub symbols: Vec<SymbolNode>,
    pub budget: usize,
    pub accumulated_tokens: usize,
    pub truncation_estimate: bool,
}

pub struct IntelligenceService {
    state: Arc<AppState>,
}

impl IntelligenceService {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }

    #[tracing::instrument(skip(self), fields(user_id, request_id))]
    pub async fn list_graph(&self, path_prefix: Option<String>, max_nodes: Option<usize>) -> Result<GraphResponse, AppError> {
        let graph_swap = self.state.resources.get_symbol_graph().await;
        let swap_clone = Arc::clone(&graph_swap);

        let (nodes, links, anomalies) = tokio::task::spawn_blocking(move || {
            let guard = swap_clone.load();

            let mut nodes = Vec::new();
            let mut links = Vec::new();

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

            // Apply max_nodes clamp if provided
            let limit = max_nodes.unwrap_or(20_000).min(20_000);
            if nodes.len() > limit {
                nodes.truncate(limit);
            }

            // 2. Filter edges: only include if both source and target nodes exist in the filtered node set
            let filtered_keys: std::collections::HashSet<String> = nodes
                .iter()
                .map(|n| format!("{}:{}", n.path, n.name))
                .collect();

            use petgraph::visit::EdgeRef;
            for edge in guard.graph.edge_references() {
                let source = match guard.graph.node_weight(edge.source()) {
                    Some(s) => s,
                    None => continue,
                };
                let target = match guard.graph.node_weight(edge.target()) {
                    Some(t) => t,
                    None => continue,
                };
                let src_key = format!("{}:{}", source.path, source.name);
                let tgt_key = format!("{}:{}", target.path, target.name);

                if filtered_keys.contains(&src_key) && filtered_keys.contains(&tgt_key) {
                    links.push(CodeGraphLink {
                        source: src_key,
                        target: tgt_key,
                    });
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

            (nodes, links, filtered_anomalies)
        })
        .await
        .map_err(|e| {
            AppError::InternalServerError(format!("Graph processing thread panicked: {}", e))
        })?;

        Ok(GraphResponse {
            nodes,
            links,
            anomalies,
        })
    }

    #[tracing::instrument(skip(self), fields(user_id, request_id))]
    pub async fn rebuild_graph(&self, dry_run: bool) -> Result<GraphRebuildResponse, AppError> {
        let graph_swap = self.state.resources.get_symbol_graph().await;
        let salt = self.state.resources.obfuscation_salt.clone();
        let swap_clone = Arc::clone(&graph_swap);

        // 1. Copy config, repository, and root inside a brief load
        let (root, config, repository) = {
            let guard = swap_clone.load();
            (guard.root.clone(), guard.config.clone(), guard.repository.clone())
        };

        if dry_run {
            // For dry run, we build a temporary graph but do not swap it.
            let summary = tokio::task::spawn_blocking(move || -> Result<RebuildSummary, AppError> {
                let mut temp_graph = CodeSymbolGraph {
                    graph: petgraph::graph::DiGraph::new(),
                    index: std::collections::HashMap::new(),
                    obfuscated_to_real_path: std::collections::HashMap::new(),
                    repository,
                    config,
                    root,
                };
                let _success = temp_graph.build(&salt)?;
                
                Ok(RebuildSummary {
                    node_count: temp_graph.graph.node_count(),
                    edge_count: temp_graph.graph.edge_count(),
                    file_count: temp_graph.index.len(),
                })
            })
            .await
            .map_err(|e| AppError::InternalServerError(format!("Graph dry-rebuild thread panicked: {}", e)))??;

            return Ok(GraphRebuildResponse {
                status: "success".to_string(),
                dry_run: true,
                summary: Some(summary),
            });
        }

        // 2. Perform the CPU-intensive rebuild on a temporary new graph instance
        let (summary, success) = tokio::task::spawn_blocking(move || -> Result<(RebuildSummary, bool), AppError> {
            let mut temp_graph = CodeSymbolGraph {
                graph: petgraph::graph::DiGraph::new(),
                index: std::collections::HashMap::new(),
                obfuscated_to_real_path: std::collections::HashMap::new(),
                repository,
                config,
                root,
            };
            let success = temp_graph.build(&salt)?;

            let node_count = temp_graph.graph.node_count();
            let edge_count = temp_graph.graph.edge_count();
            let file_count = temp_graph.index.len();

            // 3. Swap the built graph into the live lock only if success is true (soft cap constraint)
            if success {
                swap_clone.store(Arc::new(temp_graph));
            }
            
            Ok((RebuildSummary {
                node_count,
                edge_count,
                file_count,
            }, success))
        })
        .await
        .map_err(|e| AppError::InternalServerError(format!("Graph rebuild thread panicked: {}", e)))??;

        Ok(GraphRebuildResponse {
            status: if success { "success".to_string() } else { "warning".to_string() },
            dry_run: false,
            summary: Some(summary),
        })
    }

    #[tracing::instrument(skip(self), fields(user_id, request_id))]
    pub async fn blast_radius(&self, name: &str, path: &str) -> Result<Vec<SymbolNode>, AppError> {
        let workspace_root = self.state.resources.base_dir.clone();
        let graph_swap = self.state.resources.get_symbol_graph().await;
        let swap_clone = Arc::clone(&graph_swap);
        let query_name = name.to_string();
        let query_path = path.to_string();

        let task_future = tokio::task::spawn_blocking(move || {
            // Verify input path boundary unconditionally first
            if crate::utils::security::validate_path(&workspace_root, &query_path).is_err() {
                return Err(AppError::Forbidden("Invalid path boundary: potential path traversal detected".to_string()));
            }

            tracing::debug!("[intelligence] Calculating blast radius for symbol: {}", query_name);
            let guard = swap_clone.load();

            // Reverse-resolve the physical raw path from the obfuscated path
            let raw_path = guard.obfuscated_to_real_path.get(&query_path)
                .ok_or_else(|| AppError::IntelPathUnknown(format!("Path lookup failed for: {}", query_path)))?;

            // Verify resolved path resides within workspace boundary
            if crate::utils::security::validate_path(&workspace_root, raw_path).is_err() {
                return Err(AppError::Forbidden("Invalid path boundary: potential path traversal detected".to_string()));
            }

            let affected = guard.calculate_blast_radius(&query_name, raw_path);
            Ok::<_, AppError>(affected)
        });

        let affected = match timeout(Duration::from_secs(5), task_future).await {
            Ok(res) => res.map_err(|e| AppError::InternalServerError(format!("Blast radius processing thread panicked: {}", e)))??,
            Err(_) => {
                return Err(AppError::DomainError {
                    code: "BR_TIMEOUT".to_string(),
                    detail: "Blast radius computation timed out after 5 seconds".to_string(),
                    help_link: Some("https://tadpole.os/errors/br-timeout".to_string()),
                });
            }
        };

        Ok(affected)
    }

    #[tracing::instrument(skip(self), fields(user_id, request_id))]
    pub async fn resolve_context(&self, name: &str, path: &str, budget: usize) -> Result<ResolveResponse, AppError> {
        let workspace_root = self.state.resources.base_dir.clone();
        let graph_swap = self.state.resources.get_symbol_graph().await;
        let swap_clone = Arc::clone(&graph_swap);
        let query_name = name.to_string();
        let query_path = path.to_string();

        let bpe_arc = self.state.resources.get_tokenizer_bpe().await;
        let truncation_estimate = bpe_arc.is_none();

        let task_future = tokio::task::spawn_blocking(move || {
            // Verify input path boundary unconditionally first
            if crate::utils::security::validate_path(&workspace_root, &query_path).is_err() {
                return Err(AppError::Forbidden("Invalid path boundary: potential path traversal detected".to_string()));
            }

            tracing::debug!("[intelligence] Resolving context for symbol: {}", query_name);
            let guard = swap_clone.load();

            // Reverse-resolve the physical raw path from the obfuscated path
            let raw_path = guard.obfuscated_to_real_path.get(&query_path)
                .ok_or_else(|| AppError::IntelPathUnknown(format!("Path lookup failed for: {}", query_path)))?;

            // Verify resolved path resides within workspace boundary
            if crate::utils::security::validate_path(&workspace_root, raw_path).is_err() {
                return Err(AppError::Forbidden("Invalid path boundary: potential path traversal detected".to_string()));
            }

            let bpe_ref = bpe_arc.as_deref();
            let symbols = guard.resolve_context(&query_name, raw_path, budget, bpe_ref);
            
            // Calculate accumulated tokens
            let accumulated_tokens = symbols.iter().map(|s| s.tokens).sum();

            Ok::<_, AppError>((symbols, accumulated_tokens))
        });

        let (symbols, accumulated_tokens) = match timeout(Duration::from_secs(5), task_future).await {
            Ok(res) => res.map_err(|e| AppError::InternalServerError(format!("Context resolution thread panicked: {}", e)))??,
            Err(_) => {
                return Err(AppError::DomainError {
                    code: "BR_TIMEOUT".to_string(),
                    detail: "Context resolution computation timed out after 5 seconds".to_string(),
                    help_link: Some("https://tadpole.os/errors/br-timeout".to_string()),
                });
            }
        };

        Ok(ResolveResponse {
            symbols,
            budget,
            accumulated_tokens,
            truncation_estimate,
        })
    }
}

// Metadata: [service]

// Metadata: [service]

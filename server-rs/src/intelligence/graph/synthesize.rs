//! @docs ARCHITECTURE:Core
//!
//! ### AI Assist Note
//! **synthesize**: Core technical resource for the Tadpole OS infrastructure.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Unhandled errors, lock contention, or connection staling.
//! - **Telemetry Link**: Search `[synthesize]` in tracing logs.
//! - **Trace Scope**: `server-rs::intelligence::graph::synthesize`

//! Graph synthesis service for the `CodeSymbolGraph`.
//!
//! Takes parsed AST data and integrates it into the live petgraph `DiGraph`,
//! including node creation, edge resolution, and DoS-cap enforcement.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::intelligence::graph::{
    constants::{MAX_EDGES, MAX_NODES},
    engine::CodeSymbolGraph,
    error::GraphError,
    key::index_key,
    path::{obfuscate_path, to_unix_path},
    types::{SymbolEdge, SymbolNode},
};

/// Service trait to synthesize the petgraph from cached/parsed inputs.
pub trait GraphSynthesizer: Send + Sync {
    fn synthesize(
        &self,
        graph: &mut CodeSymbolGraph,
        salt: &str,
        to_delete: &[PathBuf],
        updates: Vec<(PathBuf, String, Option<(Vec<crate::utils::parser::Symbol>, Vec<crate::utils::parser::Reference>, std::time::SystemTime, u64)>)>,
    ) -> Result<(), GraphError>;
}

/// Default implementation of [`GraphSynthesizer`].
pub struct GraphSynthesisEngine;

impl GraphSynthesizer for GraphSynthesisEngine {
    fn synthesize(
        &self,
        graph: &mut CodeSymbolGraph,
        salt: &str,
        to_delete: &[PathBuf],
        updates: Vec<(PathBuf, String, Option<(Vec<crate::utils::parser::Symbol>, Vec<crate::utils::parser::Reference>, std::time::SystemTime, u64)>)>,
    ) -> Result<(), GraphError> {
        // 1. Remove deleted files from caches
        for path in to_delete {
            let rel_path = to_unix_path(path.strip_prefix(&graph.root).unwrap_or(path));
            graph.repository.file_metadata.remove(path);
            graph.repository.parse_cache.remove(&rel_path);
        }

        // 2. Apply parsed updates sequentially to avoid concurrent mutation issues
        for (path, rel_path, opt_data) in updates {
            if let Some((symbols, refs, mtime, size)) = opt_data {
                graph.repository.parse_cache.insert(rel_path, (symbols, refs));
                graph.repository.file_metadata.insert(path, (mtime, size));
            } else {
                graph.repository.parse_cache.remove(&rel_path);
                graph.repository.file_metadata.remove(&path);
            }
        }

        // 3. Clear existing graph structures
        graph.graph.clear();
        graph.index.clear();
        graph.obfuscated_to_real_path.clear();

        // 4. Add nodes and compile Inverted Name Index (with DoS protection)
        let mut name_to_indices: HashMap<String, Vec<petgraph::graph::NodeIndex>> = HashMap::new();
        let bpe = tiktoken_rs::cl100k_base().ok();
        let count_tokens = |bpe_opt: Option<&tiktoken_rs::CoreBPE>, text: &str| -> usize {
            bpe_opt
                .map(|bpe| bpe.encode_with_special_tokens(text).len())
                .unwrap_or_else(|| text.len() / 4)
        };

        for (rel_path, (symbols, _)) in &graph.repository.parse_cache {
            let obf_path = obfuscate_path(rel_path, salt)?;
            graph.obfuscated_to_real_path
                .insert(obf_path.clone(), rel_path.to_string());
            for sym in symbols {
                let key = index_key(rel_path, &sym.name);
                let token_count = count_tokens(bpe.as_ref(), &sym.signature)
                    + count_tokens(bpe.as_ref(), &sym.name)
                    + count_tokens(bpe.as_ref(), rel_path)
                    + count_tokens(bpe.as_ref(), &sym.kind);

                let node = SymbolNode {
                    name: sym.name.clone(),
                    path: obf_path.clone(),
                    kind: sym.kind.clone(),
                    signature: sym.signature.clone(),
                    start_line: (sym.range.start_line + 1) as u32,
                    end_line: (sym.range.end_line + 1) as u32,
                    tokens: token_count,
                };
                if graph.graph.node_count() >= MAX_NODES {
                    return Err(GraphError::Internal(format!(
                        "Maximum node count limit exceeded: {} nodes, max allowed is {}",
                        graph.graph.node_count(),
                        MAX_NODES
                    )));
                }
                let idx = graph.graph.add_node(node);
                graph.index.insert(key, idx);

                let entry = name_to_indices.entry(sym.name.clone()).or_default();
                if entry.len() < 1000 {
                    entry.push(idx);
                } else {
                    tracing::warn!(
                        "⚠️ [graph] Soft limit (1,000) exceeded for symbol '{}' (path: {}). Disabling indexing for this duplicate to prevent memory exhaustion.",
                        sym.name,
                        rel_path
                    );
                }
            }
        }

        tracing::info!("🔍 [graph] Indexed {} symbols.", graph.index.len());

        // 5. Extract references and add edges
        let mut added_edges = std::collections::HashSet::new();
        for (rel_path, (symbols, refs)) in &graph.repository.parse_cache {
            if symbols.is_empty() {
                continue;
            }
            for r in refs {
                let mut tightest_src: Option<(&crate::utils::parser::Symbol, usize)> = None;

                let search_start = match symbols
                    .binary_search_by(|sym| sym.range.start_byte.cmp(&r.range.start_byte))
                {
                    Ok(idx) => idx,
                    Err(idx) => idx.saturating_sub(1),
                };

                for i in (0..=search_start).rev() {
                    let src_sym = &symbols[i];
                    if src_sym.range.start_byte > r.range.start_byte {
                        continue;
                    }
                    if r.range.start_byte >= src_sym.range.start_byte
                        && r.range.end_byte <= src_sym.range.end_byte
                    {
                        let span_size = src_sym.range.end_byte - src_sym.range.start_byte;
                        match tightest_src {
                            None => {
                                tightest_src = Some((src_sym, span_size));
                            }
                            Some((_, current_min_span)) => {
                                if span_size < current_min_span {
                                    tightest_src = Some((src_sym, span_size));
                                }
                            }
                        }
                    }
                }

                if let Some((src_sym, _)) = tightest_src {
                    let src_key = index_key(rel_path, &src_sym.name);
                    if let Some(&src_idx) = graph.index.get(&src_key) {
                        if let Some(target_indices) = name_to_indices.get(&r.name) {
                            for &target_idx in target_indices {
                                if src_idx != target_idx
                                    && added_edges.insert((src_idx, target_idx))
                                {
                                    if graph.graph.edge_count() >= MAX_EDGES {
                                        return Err(GraphError::Internal(format!(
                                            "Maximum edge count limit exceeded: {} edges, max allowed is {}",
                                            graph.graph.edge_count(),
                                            MAX_EDGES
                                        )));
                                    }
                                    graph.graph.add_edge(
                                        src_idx,
                                        target_idx,
                                        SymbolEdge {
                                            kind: "ref".to_string(),
                                        },
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }

        tracing::info!(
            "✅ [graph] Knowledge graph build complete (Nodes: {}, Edges: {}).",
            graph.graph.node_count(),
            graph.graph.edge_count()
        );

        Ok(())
    }
}

// Metadata: [synthesize]

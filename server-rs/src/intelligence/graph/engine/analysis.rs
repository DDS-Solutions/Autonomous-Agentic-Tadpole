//! @docs ARCHITECTURE:Core
//!
//! ### AI Assist Note
//! **Graph Analysis & Query Engine**: Exposes intelligence queries on the
//! symbol graph including anomaly detection, blast radius BFS calculations,
//! and token-budgeted context resolution.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Large graph cycle traversal, token budget truncation.
//! - **Telemetry Link**: Search `[analysis]` in tracing logs.

use std::collections::{HashSet, VecDeque};
use petgraph::visit::EdgeRef;
use crate::intelligence::graph::{
    key::index_key,
    types::SymbolNode,
};
use super::CodeSymbolGraph;

impl CodeSymbolGraph {
    /// Audits the graph for structural anomalies (dead code — symbols with 0
    /// incoming references, excluding configured exclusion patterns).
    pub fn find_anomalies(&self) -> Vec<String> {
        tracing::debug!("[analysis] Auditing graph anomalies...");
        let mut anomalies = Vec::new();

        for idx in self.graph.node_indices() {
            if let Some(node) = self.graph.node_weight(idx) {
                let real_path = self
                    .obfuscated_to_real_path
                    .get(&node.path)
                    .map(|p| p.as_str())
                    .unwrap_or(&node.path);

                if self.config.is_path_excluded(real_path) {
                    continue;
                }

                if self.config.is_symbol_excluded(&node.name, &node.kind) {
                    continue;
                }

                let incoming = self
                    .graph
                    .edges_directed(idx, petgraph::Direction::Incoming)
                    .count();
                if incoming == 0 {
                    anomalies.push(format!(
                        "Unused symbol (0 incoming references): {} in {}",
                        node.name, node.path
                    ));
                }
            }
        }

        anomalies
    }

    /// Resolves a path that could be real (raw) or obfuscated, with
    /// forward-slash normalisation.
    pub fn resolve_path(&self, path: &str) -> String {
        let normalized = path.replace('\\', "/");
        if let Some(real) = self.obfuscated_to_real_path.get(&normalized) {
            return real.clone();
        }
        normalized
    }

    /// Calculates the "Blast Radius" for a given symbol: the set of symbols
    /// that directly or transitively depend on it (via BFS on incoming edges,
    /// capped at depth 50 and bounded by max_nodes to resist adversarial graph topologies).
    pub fn calculate_blast_radius(&self, symbol_name: &str, path: &str) -> Vec<SymbolNode> {
        self.calculate_blast_radius_bounded(symbol_name, path, None).0
    }

    /// Calculates the "Blast Radius" with optional node limit and returns
    /// `(affected_symbols, was_truncated)`. Default node limit is 500.
    pub fn calculate_blast_radius_bounded(
        &self,
        symbol_name: &str,
        path: &str,
        max_nodes: Option<usize>,
    ) -> (Vec<SymbolNode>, bool) {
        let real_path = self.resolve_path(path);
        let limit = max_nodes.unwrap_or(500).clamp(1, crate::intelligence::graph::constants::MAX_NODES);
        let mut affected = Vec::new();
        let mut truncated = false;

        let start_indices: Vec<petgraph::graph::NodeIndex> = if symbol_name == "*" || symbol_name.is_empty() {
            let prefix = format!("{real_path}\0");
            let mut indices: Vec<_> = self
                .index
                .iter()
                .filter_map(|(k, &idx)| if k.starts_with(&prefix) { Some(idx) } else { None })
                .collect();
            if indices.is_empty() {
                // Fallback: check node weights if index used relative/normalized path differences
                for idx in self.graph.node_indices() {
                    if let Some(node) = self.graph.node_weight(idx) {
                        let node_real = self.resolve_path(&node.path);
                        if node_real == real_path || node.path == real_path {
                            indices.push(idx);
                        }
                    }
                }
            }
            indices
        } else {
            let key = index_key(&real_path, symbol_name);
            self.index.get(&key).copied().into_iter().collect()
        };

        if !start_indices.is_empty() {
            let mut visited = HashSet::new();
            let mut queue = VecDeque::new();
            let mut affected_indices = Vec::new();

            for &start_idx in &start_indices {
                if visited.insert(start_idx) {
                    if affected_indices.len() >= limit {
                        truncated = true;
                        break;
                    }
                    affected_indices.push(start_idx);
                    queue.push_back((start_idx, 0));
                }
            }

            'traverse: while let Some((current_idx, depth)) = queue.pop_front() {
                for edge in self
                    .graph
                    .edges_directed(current_idx, petgraph::Direction::Incoming)
                {
                    let neighbor_idx = edge.source();
                    if !visited.contains(&neighbor_idx) {
                        if depth >= 50 {
                            truncated = true;
                            continue;
                        }
                        if affected_indices.len() >= limit {
                            truncated = true;
                            break 'traverse;
                        }
                        visited.insert(neighbor_idx);
                        affected_indices.push(neighbor_idx);
                        queue.push_back((neighbor_idx, depth + 1));
                    }
                }
            }

            // Single contiguous clone with node_weight bounds protection
            for idx in affected_indices {
                if let Some(node) = self.graph.node_weight(idx) {
                    affected.push(node.clone());
                }
            }
        }

        (affected, truncated)
    }

    /// Resolves dependent symbols for a given target within a token `budget`.
    ///
    /// Prioritizes the target symbol first, then walks backward through incoming
    /// edges (callers) via BFS until the budget is exhausted or depth 50 is reached.
    /// Counts metadata and the truncation suffix as well as the signature. A
    /// budget too small for the target's metadata returns an empty result.
    pub fn resolve_context(
        &self,
        symbol_name: &str,
        path: &str,
        budget: usize,
        bpe: Option<&tiktoken_rs::CoreBPE>,
    ) -> Vec<SymbolNode> {
        let real_path = self.resolve_path(path);
        let key = index_key(&real_path, symbol_name);
        let mut results = Vec::new();
        let mut accumulated_tokens = 0;
        let count_tokens = |text: &str| -> usize {
            bpe.map(|tokenizer| tokenizer.encode_with_special_tokens(text).len())
                .unwrap_or_else(|| text.len().div_ceil(4))
        };
        let metadata_tokens = |node: &SymbolNode| -> usize {
            count_tokens(&node.name)
                + count_tokens(&self.resolve_path(&node.path))
                + count_tokens(&node.kind)
        };

        if let Some(&start_idx) = self.index.get(&key) {
            let start_node = match self.graph.node_weight(start_idx) {
                Some(n) => n,
                None => return results,
            };
            let overhead = metadata_tokens(start_node);
            if overhead > budget {
                return results;
            }
            let mut start_clone = start_node.clone();
            let signature_budget = budget - overhead;
            if count_tokens(&start_clone.signature) > signature_budget {
                let suffix = if count_tokens("...") <= signature_budget { "..." } else { "" };
                let content_budget = signature_budget.saturating_sub(count_tokens(suffix));
                if let Some(tokenizer) = bpe {
                    let tokens = tokenizer.encode_with_special_tokens(&start_clone.signature);
                    let mut end = content_budget.min(tokens.len());
                    // A token boundary can split a UTF-8 code point; back up until valid.
                    loop {
                        if let Ok(text) = tokenizer.decode(&tokens[..end]) {
                            start_clone.signature = text;
                            break;
                        }
                        if end == 0 {
                            start_clone.signature.clear();
                            break;
                        }
                        end -= 1;
                    }
                } else {
                    let byte_budget = signature_budget.saturating_mul(4).saturating_sub(suffix.len());
                    let end = start_clone.signature.floor_char_boundary(byte_budget.min(start_clone.signature.len()));
                    start_clone.signature.truncate(end);
                }
                // Retokenize the combined text: merging a suffix can change BPE boundaries.
                while count_tokens(&format!("{}{suffix}", start_clone.signature)) > signature_budget {
                    if start_clone.signature.pop().is_none() {
                        break;
                    }
                }
                start_clone.signature.push_str(suffix);
            }
            start_clone.tokens = overhead + count_tokens(&start_clone.signature);
            accumulated_tokens += start_clone.tokens;

            results.push(start_clone);

            // BFS for callers (if remaining budget allows)
            if accumulated_tokens < budget {
                let mut visited = HashSet::new();
                let mut queue = VecDeque::new();
                queue.push_back((start_idx, 0));
                visited.insert(start_idx);

                while let Some((current_idx, depth)) = queue.pop_front() {
                    if depth >= 50 {
                        continue;
                    }
                    for edge in self
                        .graph
                        .edges_directed(current_idx, petgraph::Direction::Incoming)
                    {
                        let neighbor_idx = edge.source();
                        if visited.insert(neighbor_idx) {
                            let node = match self.graph.node_weight(neighbor_idx) {
                                Some(n) => n,
                                None => continue,
                            };
                            let node_tokens = metadata_tokens(node) + count_tokens(&node.signature);

                            if node_tokens <= budget.saturating_sub(accumulated_tokens) {
                                let mut node = node.clone();
                                node.tokens = node_tokens;
                                results.push(node);
                                accumulated_tokens += node_tokens;
                                queue.push_back((neighbor_idx, depth + 1));
                            }
                        }
                    }
                }
            }
        }

        results
    }
}

// Metadata: [analysis]

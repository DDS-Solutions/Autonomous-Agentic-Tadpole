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
        let key = index_key(&real_path, symbol_name);
        let mut affected = Vec::new();
        let limit = max_nodes.unwrap_or(500).clamp(1, 5_000);
        let mut truncated = false;

        if let Some(&start_idx) = self.index.get(&key) {
            let mut visited = HashSet::new();
            let mut queue = VecDeque::new();
            queue.push_back((start_idx, 0));
            visited.insert(start_idx);

            let mut affected_indices = vec![start_idx];
            while let Some((current_idx, depth)) = queue.pop_front() {
                if affected_indices.len() >= limit {
                    truncated = true;
                    break;
                }
                if depth >= 50 {
                    continue; // Shield against malicious/adversarial large depth chains
                }
                for edge in self
                    .graph
                    .edges_directed(current_idx, petgraph::Direction::Incoming)
                {
                    let neighbor_idx = edge.source();
                    if visited.insert(neighbor_idx) {
                        affected_indices.push(neighbor_idx);
                        if affected_indices.len() >= limit {
                            truncated = true;
                            break;
                        }
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
    /// The target node's signature is truncated (with `"..."` suffix) if it alone
    /// exceeds the budget, preserving room for top callers where possible.
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

        if let Some(&start_idx) = self.index.get(&key) {
            let start_node = match self.graph.node_weight(start_idx) {
                Some(n) => n,
                None => return results,
            };
            let start_tokens = start_node.tokens;

            let mut start_clone = start_node.clone();

            let effective_start_tokens = if start_tokens > budget {
                // Truncate signature to fit budget using BPE tokenizer if available
                if let Some(tokenizer) = bpe {
                    let tokens = tokenizer.encode_with_special_tokens(&start_clone.signature);
                    let target_budget = budget.saturating_sub(5).max(1);
                    if tokens.len() > target_budget {
                        if let Ok(truncated_text) = tokenizer.decode(&tokens[..target_budget]) {
                            start_clone.signature = format!("{}...", truncated_text);
                            start_clone.tokens = target_budget;
                        }
                    }
                } else {
                    let target_budget = budget.saturating_sub(5).max(1);
                    let budget_chars = target_budget * 4;
                    if start_clone.signature.len() > budget_chars {
                        // SEC: Safe truncation at char boundary to prevent panics on multi-byte UTF-8
                        let safe_bound = start_clone.signature.floor_char_boundary(budget_chars);
                        start_clone.signature =
                            format!("{}...", &start_clone.signature[..safe_bound]);
                        start_clone.tokens = target_budget;
                    }
                }
                start_clone.tokens.min(budget)
            } else {
                start_tokens
            };

            results.push(start_clone);
            accumulated_tokens += effective_start_tokens;

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
                    let mut budget_exceeded = false;
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
                            let node_tokens = node.tokens;

                            if accumulated_tokens + node_tokens <= budget {
                                results.push(node.clone());
                                accumulated_tokens += node_tokens;
                                queue.push_back((neighbor_idx, depth + 1));
                            } else {
                                budget_exceeded = true;
                                break;
                            }
                        }
                    }
                    if budget_exceeded {
                        break;
                    }
                }
            }
        }

        results
    }
}

// Metadata: [analysis]

//! Symbol-Level Knowledge Graph — Codebase Topology
//!
//! @docs ARCHITECTURE:Intelligence
//!
//! ### AI Assist Note
//! **Knowledge Graph**: Builds a directed graph of code symbols 
//! (functions, structs, traits) and their interdependencies. 
//! Enables **Blast Radius Analysis**: helps agents understand the 
//! impact of changing a specific symbol by tracing outgoing edges. 
//! Note: Current implementation uses a **Global Namespace Heuristic** 
//! for cross-file resolution; complex shadowing or macro-heavy 
//! code may require more advanced scope-aware parsing (GRAPH-02).

use std::collections::HashMap;
use std::path::{PathBuf};
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::EdgeRef;
use crate::utils::parser::{SymbolExtractor};
use serde::{Deserialize, Serialize};
use specta::Type;
use walkdir::WalkDir;

/// A node in the knowledge graph representing a code symbol.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct SymbolNode {
    pub name: String,
    pub path: String,
    pub kind: String,
    pub signature: String,
}

/// An edge in the knowledge graph representing a dependency.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct SymbolEdge {
    pub kind: String,
}

/// The core Knowledge Graph engine.
pub struct CodeSymbolGraph {
    pub graph: DiGraph<SymbolNode, SymbolEdge>,
    pub index: HashMap<String, NodeIndex>, // key: "path::name"
    root: PathBuf,
}

impl CodeSymbolGraph {
    /// Creates a new, empty knowledge graph.
    pub fn new(root: PathBuf) -> Self {
        Self {
            graph: DiGraph::new(),
            index: HashMap::new(),
            root,
        }
    }

    /// Scans the workspace and populates the graph with symbols and references.
    pub fn build(&mut self) {
        let mut extractor = SymbolExtractor::new();
        let mut all_symbols = Vec::new();
        
        tracing::info!("🔍 [Graph] Building symbol-level knowledge graph for {}...", self.root.display());

        // 1. Extract all symbols (Nodes)
        for entry in WalkDir::new(&self.root)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
        {
            let path = entry.path();
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if ext != "rs" && ext != "ts" && ext != "tsx" {
                continue;
            }

            // Skip infrastructure dirs
            let path_str = path.to_string_lossy();
            if path_str.contains("target") || path_str.contains("node_modules") || path_str.contains(".git") {
                continue;
            }

            if let Ok(content) = std::fs::read_to_string(path) {
                let rel_path = path.strip_prefix(&self.root).unwrap_or(path).to_string_lossy().to_string().replace('\\', "/");
                let symbols = extractor.extract_symbols(path, &content);
                for sym in symbols {
                    all_symbols.push((rel_path.clone(), sym));
                }
            }
        }

        // 2. Add nodes to graph
        for (path, sym) in &all_symbols {
            let key = format!("{}::{}", path, sym.name);
            let node = SymbolNode {
                name: sym.name.clone(),
                path: path.clone(),
                kind: sym.kind.clone(),
                signature: sym.signature.clone(),
            };
            let idx = self.graph.add_node(node);
            self.index.insert(key, idx);
        }

        tracing::info!("✅ [Graph] Indexed {} symbols.", all_symbols.len());

        // 3. Extract references and add edges (Dependencies)
        for entry in WalkDir::new(&self.root)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
        {
            let path = entry.path();
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if ext != "rs" && ext != "ts" && ext != "tsx" {
                continue;
            }

            let path_str = path.to_string_lossy();
            if path_str.contains("target") || path_str.contains("node_modules") {
                continue;
            }

            if let Ok(content) = std::fs::read_to_string(path) {
                let rel_path = path.strip_prefix(&self.root).unwrap_or(path).to_string_lossy().to_string().replace('\\', "/");
                let refs = extractor.extract_references(path, &content);
                
                let file_symbols: Vec<_> = all_symbols.iter()
                    .filter(|(p, _)| p == &rel_path)
                    .collect();

                for r in refs {
                    // Try to find the target symbol (heuristic: global name match)
                    for (target_key, &target_idx) in &self.index {
                        let target_name = target_key.split("::").last().unwrap_or("");
                        if target_name == r.name {
                            // Find which source symbol in THIS file contains this reference
                            for (_, src_sym) in &file_symbols {
                                if r.range.start_byte >= src_sym.range.start_byte && r.range.end_byte <= src_sym.range.end_byte {
                                    let src_key = format!("{}::{}", rel_path, src_sym.name);
                                    if let Some(&src_idx) = self.index.get(&src_key) {
                                        if src_idx != target_idx {
                                            self.graph.add_edge(src_idx, target_idx, SymbolEdge { kind: "ref".to_string() });
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        
        tracing::info!("✅ [Graph] Knowledge graph build complete (Nodes: {}, Edges: {}).", self.graph.node_count(), self.graph.edge_count());
    }

    /// Calculates the "Blast Radius" for a given symbol.
    /// Returns a list of symbols that directly or indirectly depend on it.
    pub fn calculate_blast_radius(&self, symbol_name: &str, path: &str) -> Vec<SymbolNode> {
        let key = format!("{}::{}", path, symbol_name);
        let mut affected = Vec::new();
        
        if let Some(&start_idx) = self.index.get(&key) {
            // BFS/DFS to find all symbols that reference this one
            // Note: edges are (source -> target), so we need to traverse in REVERSE (target -> source)
            let mut visited = std::collections::HashSet::new();
            let mut stack = vec![start_idx];
            visited.insert(start_idx);

            while let Some(current_idx) = stack.pop() {
                // Find all neighbors that point to current_idx
                for edge in self.graph.edges_directed(current_idx, petgraph::Direction::Incoming) {
                    let neighbor_idx = edge.source();
                    if visited.insert(neighbor_idx) {
                        affected.push(self.graph[neighbor_idx].clone());
                        stack.push(neighbor_idx);
                    }
                }
            }
        }
        
        affected
    }
}

// Metadata: [graph]

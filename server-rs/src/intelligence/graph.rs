/*
### AI Assist Note
**🛡️ Tadpole OS: Graph**
Core system module providing specialized functionality for the agent swarm.

### 🔍 Debugging & Observability
- **Failure Path**: Unexpected execution drift or type compatibility issues.
- **Telemetry Link**: Traced via active system logging channels.
*/

//! Symbol-Level Knowledge Graph — Codebase Topology
//!
//! @docs ARCHITECTURE:Intelligence
//!
//! ### AI Assist Note
//! **Knowledge Graph**: Builds a directed graph of code symbols 
//! (functions, structs, traits) and their interdependencies. 
//! Enables **Blast Radius Analysis**: helps agents understand the 
//! impact of changing a specific symbol by tracing outgoing edges. 

use std::collections::HashMap;
use std::path::{PathBuf};
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::EdgeRef;
use crate::utils::parser::{SymbolExtractor};
use serde::{Deserialize, Serialize};
use specta::Type;
use walkdir::WalkDir;
use rayon::prelude::*;

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
    pub index: HashMap<(String, String), NodeIndex>, // key: (path, name)
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
        tracing::info!("🔍 [Graph] Building symbol-level knowledge graph for {}...", self.root.display());

        // 1. Gather all target files to scan
        let files: Vec<PathBuf> = WalkDir::new(&self.root)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
            .map(|e| e.path().to_path_buf())
            .filter(|path| {
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if ext != "rs" && ext != "ts" && ext != "tsx" {
                    return false;
                }

                // Skip infrastructure dirs
                let path_str = path.to_string_lossy();
                if path_str.contains("target") || path_str.contains("node_modules") || path_str.contains(".git") {
                    return false;
                }

                // 🛡️ [DoS Protection] Enforce 2MB size limit to avoid scanning massive database/build/artifact dumps
                if let Ok(metadata) = std::fs::metadata(path) {
                    if metadata.len() > 2 * 1024 * 1024 {
                        tracing::warn!("⚠️ [Graph] Skipping oversized file ({} bytes): {}", metadata.len(), path.display());
                        return false;
                    }
                } else {
                    return false;
                }
                true
            })
            .collect();

        // 2. Extract symbols & references in parallel using Rayon (Single-Pass reading)
        let parsed_files: Vec<(String, Vec<crate::utils::parser::Symbol>, Vec<crate::utils::parser::Reference>)> = files
            .par_iter()
            .filter_map(|path| {
                match std::fs::read_to_string(path) {
                    Ok(content) => {
                        let rel_path = path.strip_prefix(&self.root).unwrap_or(path).to_string_lossy().to_string().replace('\\', "/");
                        let mut extractor = SymbolExtractor::new();
                        let symbols = extractor.extract_symbols(path, &content);
                        let refs = extractor.extract_references(path, &content);
                        Some((rel_path, symbols, refs))
                    }
                    Err(e) => {
                        tracing::warn!("⚠️ [Graph] Failed to read file {}: {}", path.display(), e);
                        None
                    }
                }
            })
            .collect();

        // 3. Add nodes to graph and compile Inverted Name Index
        let mut name_to_indices: HashMap<String, Vec<NodeIndex>> = HashMap::new();
        for (rel_path, symbols, _) in &parsed_files {
            for sym in symbols {
                let key = (rel_path.clone(), sym.name.clone());
                let node = SymbolNode {
                    name: sym.name.clone(),
                    path: rel_path.clone(),
                    kind: sym.kind.clone(),
                    signature: sym.signature.clone(),
                };
                let idx = self.graph.add_node(node);
                self.index.insert(key, idx);
                name_to_indices.entry(sym.name.clone()).or_default().push(idx);
            }
        }

        tracing::info!("✅ [Graph] Indexed {} symbols.", self.index.len());

        // 4. Extract references and add edges (Dependencies)
        let mut added_edges = std::collections::HashSet::new();
        for (rel_path, symbols, refs) in &parsed_files {
            for r in refs {
                // 🚀 O(1) Lookup of matching target symbol names
                if let Some(target_indices) = name_to_indices.get(&r.name) {
                    for &target_idx in target_indices {
                        // Find the tightest (deepest nested) source symbol in THIS file that contains this reference range
                        let mut tightest_src: Option<(&crate::utils::parser::Symbol, usize)> = None;
                        for src_sym in symbols {
                            if r.range.start_byte >= src_sym.range.start_byte && r.range.end_byte <= src_sym.range.end_byte {
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
                            let src_key = (rel_path.clone(), src_sym.name.clone());
                            if let Some(&src_idx) = self.index.get(&src_key) {
                                if src_idx != target_idx {
                                    if added_edges.insert((src_idx, target_idx)) {
                                        self.graph.add_edge(src_idx, target_idx, SymbolEdge { kind: "ref".to_string() });
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

    /// Audits the graph for structural anomalies (dead code).
    pub fn find_anomalies(&self) -> Vec<String> {
        let mut anomalies = Vec::new();

        for idx in self.graph.node_indices() {
            if let Some(node) = self.graph.node_weight(idx) {
                // Skip TypeScript/JavaScript files due to AST reference resolution limitations
                if node.path.ends_with(".ts") || node.path.ends_with(".tsx") {
                    continue;
                }

                // Skip backend files since Rust compiler dead-code and public-export patterns are handled natively
                if node.path.starts_with("server-rs/") {
                    continue;
                }

                // Skip scratch/ files since they are temporary development scripts
                if node.path.contains("scratch/") {
                    continue;
                }

                // Skip entrypoints, tests, and standard route/event handlers
                let name_lower = node.name.to_lowercase();
                if name_lower == "main"
                    || name_lower.contains("test")
                    || name_lower.contains("route")
                    || name_lower.contains("handler")
                    || name_lower.contains("register")
                {
                    continue;
                }

                let incoming = self.graph.edges_directed(idx, petgraph::Direction::Incoming).count();
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

    /// Calculates the "Blast Radius" for a given symbol.
    /// Returns a list of symbols that directly or indirectly depend on it.
    pub fn calculate_blast_radius(&self, symbol_name: &str, path: &str) -> Vec<SymbolNode> {
        let key = (path.to_string(), symbol_name.to_string());
        let mut affected = Vec::new();
        
        if let Some(&start_idx) = self.index.get(&key) {
            // BFS to find all symbols that reference this one up to depth 50
            // Note: edges are (source -> target), so we need to traverse in REVERSE (target -> source)
            let mut visited = std::collections::HashSet::new();
            let mut queue = std::collections::VecDeque::new();
            queue.push_back((start_idx, 0));
            visited.insert(start_idx);

            let mut affected_indices = Vec::new();
            while let Some((current_idx, depth)) = queue.pop_front() {
                if depth >= 50 {
                    continue; // Shield against malicious/adversarial large depth chains
                }
                // Find all neighbors that point to current_idx
                for edge in self.graph.edges_directed(current_idx, petgraph::Direction::Incoming) {
                    let neighbor_idx = edge.source();
                    if visited.insert(neighbor_idx) {
                        affected_indices.push(neighbor_idx);
                        queue.push_back((neighbor_idx, depth + 1));
                    }
                }
            }

            // Perform single contiguous clone of final affected payloads to avoid traversal allocation pressure
            for idx in affected_indices {
                affected.push(self.graph[idx].clone());
            }
        }
        
        affected
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use std::fs::File;
    use std::io::Write;

    #[test]
    fn test_empty_blast_radius_nonexistent() {
        let dir = tempdir().unwrap();
        let graph = CodeSymbolGraph::new(dir.path().to_path_buf());
        let affected = graph.calculate_blast_radius("nonexistent", "src/lib.rs");
        assert!(affected.is_empty(), "Blast radius of nonexistent symbol must be empty");
    }

    #[test]
    fn test_happy_path_symbol_dependency() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("main.rs");
        
        // Write mock code content with two symbols: main and helper
        let mut file = File::create(&file_path).unwrap();
        writeln!(file, "fn helper() {{ }}").unwrap();
        writeln!(file, "fn main() {{ helper(); }}").unwrap();
        
        let mut graph = CodeSymbolGraph::new(dir.path().to_path_buf());
        graph.build();
        
        // Check that nodes and edges are populated
        assert!(graph.graph.node_count() >= 2, "Should index at least 2 symbols");
        
        // Calculate blast radius for helper() - main() should be affected
        let affected = graph.calculate_blast_radius("helper", "main.rs");
        assert!(!affected.is_empty(), "helper blast radius should not be empty");
        let has_main = affected.iter().any(|node| node.name == "main");
        assert!(has_main, "main should depend on helper");
    }

    #[test]
    fn test_circular_dependency_handling() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("main.rs");
        
        // Write circular dependency mock code
        let mut file = File::create(&file_path).unwrap();
        writeln!(file, "fn alpha() {{ beta(); }}").unwrap();
        writeln!(file, "fn beta() {{ alpha(); }}").unwrap();
        
        let mut graph = CodeSymbolGraph::new(dir.path().to_path_buf());
        graph.build();
        
        // BFS should handle the cycle gracefully and terminate without infinite loop
        let affected_alpha = graph.calculate_blast_radius("alpha", "main.rs");
        let affected_beta = graph.calculate_blast_radius("beta", "main.rs");
        
        assert!(!affected_alpha.is_empty());
        assert!(!affected_beta.is_empty());
    }
}

// Metadata: [graph]

// Metadata: [graph]

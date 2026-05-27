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
    pub tokens: usize,
}

/// An edge in the knowledge graph representing a dependency.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct SymbolEdge {
    pub kind: String,
}

use sha2::{Sha256, Digest};
use std::path::Path;

/// Helper to obfuscate physical file path structures deterministically
/// while preserving UX force-graph clustering and file basenames.
pub fn obfuscate_path(path_str: &str, salt: &str) -> String {
    let path = Path::new(path_str);
    let file_name = path.file_name().and_then(|f| f.to_str()).unwrap_or("unknown");
    let parent = path.parent().unwrap_or(Path::new("")).to_string_lossy();
    
    let parent_to_hash = if parent.is_empty() {
        "__root__"
    } else {
        &parent
    };

    let mut hasher = Sha256::new();
    hasher.update(salt.as_bytes());
    hasher.update(parent_to_hash.as_bytes());
    let result = hasher.finalize();
    let hash_val = hex::encode(result);
    format!("{}/{}", hash_val, file_name)
}

/// The core Knowledge Graph engine.
pub struct CodeSymbolGraph {
    pub graph: DiGraph<SymbolNode, SymbolEdge>,
    pub index: HashMap<(String, String), NodeIndex>, // key: (path, name)
    pub reverse_obfuscation_index: HashMap<String, String>, // key: obfuscated_path, val: physical_path
    root: PathBuf,
}

pub type ParsedFiles = Vec<(String, Vec<crate::utils::parser::Symbol>, Vec<crate::utils::parser::Reference>)>;

impl CodeSymbolGraph {
    /// Creates a new, empty knowledge graph.
    pub fn new(root: PathBuf) -> Self {
        Self {
            graph: DiGraph::new(),
            index: HashMap::new(),
            reverse_obfuscation_index: HashMap::new(),
            root,
        }
    }

    /// Scans the workspace files and extracts symbols and references.
    pub fn scan_workspace(root: &std::path::Path) -> ParsedFiles {
        // 1. Gather all target files to scan
        let files: Vec<PathBuf> = WalkDir::new(root)
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
        // Bound compilation compute resources to prevent host thread pool starvation (half of cores, minimum 1)
        let num_threads = std::thread::available_parallelism()
            .map(|n| n.get() / 2)
            .unwrap_or(2)
            .max(1);

        let pool_res = rayon::ThreadPoolBuilder::new()
            .num_threads(num_threads)
            .build();

        match pool_res {
            Ok(pool) => pool.install(|| {
                files
                    .par_iter()
                    .filter_map(|path| {
                        match std::fs::read_to_string(path) {
                            Ok(content) => {
                                let rel_path = path.strip_prefix(root).unwrap_or(path).to_string_lossy().to_string().replace('\\', "/");
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
                    .collect()
            }),
            Err(e) => {
                tracing::warn!("⚠️ [Graph] Failed to create custom Rayon pool: {}. Falling back to default pool.", e);
                files
                    .par_iter()
                    .filter_map(|path| {
                        match std::fs::read_to_string(path) {
                            Ok(content) => {
                                let rel_path = path.strip_prefix(root).unwrap_or(path).to_string_lossy().to_string().replace('\\', "/");
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
                    .collect()
            }
        }
    }

    /// Rebuilds the symbol-level knowledge graph from pre-parsed workspace files in-memory.
    pub fn build_from_parsed(&mut self, parsed_files: ParsedFiles, salt: &str) {
        self.graph.clear();
        self.index.clear();
        self.reverse_obfuscation_index.clear();

        // 3. Add nodes to graph and compile Inverted Name Index
        let mut name_to_indices: HashMap<String, Vec<NodeIndex>> = HashMap::new();
        let bpe = tiktoken_rs::cl100k_base().ok();
        let count_tokens = |bpe_opt: Option<&tiktoken_rs::CoreBPE>, text: &str| -> usize {
            bpe_opt
                .map(|bpe| bpe.encode_with_special_tokens(text).len())
                .unwrap_or_else(|| text.len() / 4)
        };

        for (rel_path, symbols, _) in &parsed_files {
            let obf_path = obfuscate_path(rel_path, salt);
            self.reverse_obfuscation_index.insert(obf_path, rel_path.clone());

            for sym in symbols {
                let key = (rel_path.clone(), sym.name.clone());
                let token_count = count_tokens(bpe.as_ref(), &sym.signature)
                    + count_tokens(bpe.as_ref(), &sym.name)
                    + count_tokens(bpe.as_ref(), rel_path)
                    + count_tokens(bpe.as_ref(), &sym.kind);

                let node = SymbolNode {
                    name: sym.name.clone(),
                    path: rel_path.clone(),
                    kind: sym.kind.clone(),
                    signature: sym.signature.clone(),
                    tokens: token_count,
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

    /// Scans the workspace and populates the graph with symbols and references.
    pub fn build(&mut self, salt: &str) {
        tracing::info!("🔍 [Graph] Building symbol-level knowledge graph for {}...", self.root.display());
        let parsed = Self::scan_workspace(&self.root);
        self.build_from_parsed(parsed, salt);
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

    pub fn calculate_blast_radius(&self, symbol_name: &str, path: &str) -> Vec<SymbolNode> {
        let normalized_path = path.replace('\\', "/");
        let key = (normalized_path, symbol_name.to_string());
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

    /// Resolves dependent symbols for a given target symbol within a specified token budget.
    /// Prioritizes the target symbol, then walks backwards through the dependency graph (incoming edges)
    /// to add callers until the token budget is reached.
    pub fn resolve_context(&self, symbol_name: &str, path: &str, budget: usize) -> Vec<SymbolNode> {
        let normalized_path = path.replace('\\', "/");
        let key = (normalized_path, symbol_name.to_string());
        let mut results = Vec::new();
        let mut accumulated_tokens = 0;

        if let Some(&start_idx) = self.index.get(&key) {
            let start_node = &self.graph[start_idx];
            let start_tokens = start_node.tokens;
            
            // Add start node first
            let mut start_clone = start_node.clone();
            
            if start_tokens > budget {
                // Truncate signature to fit budget
                let budget_chars = budget * 4;
                if start_clone.signature.len() > budget_chars {
                    start_clone.signature = format!("{}...", &start_clone.signature[..budget_chars]);
                }
                results.push(start_clone);
                return results;
            }

            results.push(start_clone);
            accumulated_tokens += start_tokens;

            // BFS for callers
            let mut visited = std::collections::HashSet::new();
            let mut queue = std::collections::VecDeque::new();
            queue.push_back((start_idx, 0));
            visited.insert(start_idx);

            while let Some((current_idx, depth)) = queue.pop_front() {
                if depth >= 50 {
                    continue;
                }
                let mut budget_exceeded = false;
                for edge in self.graph.edges_directed(current_idx, petgraph::Direction::Incoming) {
                    let neighbor_idx = edge.source();
                    if visited.insert(neighbor_idx) {
                        let node = &self.graph[neighbor_idx];
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

        results
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
        graph.build("test-salt");
        
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
        graph.build("test-salt");
        
        // BFS should handle the cycle gracefully and terminate without infinite loop
        let affected_alpha = graph.calculate_blast_radius("alpha", "main.rs");
        let affected_beta = graph.calculate_blast_radius("beta", "main.rs");
        
        assert!(!affected_alpha.is_empty());
        assert!(!affected_beta.is_empty());
    }

    #[test]
    fn test_token_budgeted_context_resolution() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("main.rs");
        
        let mut file = File::create(&file_path).unwrap();
        writeln!(file, "fn helper() {{ }}").unwrap();
        writeln!(file, "fn main() {{ helper(); }}").unwrap();
        
        let mut graph = CodeSymbolGraph::new(dir.path().to_path_buf());
        graph.build("test-salt");
        
        // Query with large budget - both symbols should fit
        let resolved_large = graph.resolve_context("helper", "main.rs", 1000);
        assert_eq!(resolved_large.len(), 2);
        
        // Query with small budget - only target should fit and be truncated
        let resolved_small = graph.resolve_context("helper", "main.rs", 2);
        assert!(resolved_small[0].signature.ends_with("..."));
    }

    #[test]
    fn test_obfuscate_path_hash_length() {
        let path = "src/routes/intelligence.rs";
        let salt = "test-salt-string-value";
        let obfuscated = obfuscate_path(path, salt);
        
        // Obfuscated output format: <64-char hex>/<filename>
        let parts: Vec<&str> = obfuscated.split('/').collect();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].len(), 64, "Obfuscation prefix hash MUST be the full 64-character SHA-256 string");
        assert_eq!(parts[1], "intelligence.rs");
    }

    #[test]
    fn test_scan_workspace_parallel_execution() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("main.rs");
        let mut file = File::create(&file_path).unwrap();
        writeln!(file, "fn test() {{ }}").unwrap();

        // Verifies scan_workspace compiles correctly under custom Rayon ThreadPool
        let parsed = CodeSymbolGraph::scan_workspace(dir.path());
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].0, "main.rs");
    }
}

// Metadata: [graph]

// Metadata: [graph]

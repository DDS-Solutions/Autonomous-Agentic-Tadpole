//! @docs ARCHITECTURE:Intelligence
//!
//! ### AI Assist Note
//! **Symbol-Level Knowledge Graph — Codebase Topology**
//! Orchestrates discovery → cache-check → parse → synthesis into a live
//! petgraph `DiGraph`. Also exposes blast-radius BFS, anomaly detection,
//! and token-budgeted context resolution.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Runtime logic error, state desynchronization, or resource exhaustion.
//! - **Telemetry Link**: Search `[engine]` in tracing logs.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::EdgeRef;

use crate::intelligence::graph::{
    cache::{CacheManagementService, CacheManager},
    config::GraphConfig,
    constants::MAX_DISCOVERED_FILES,
    discover::{FileDiscoverer, FileDiscoveryService},
    error::GraphError,
    key::index_key,
    parse::{CodeParser, CodeParsingService},
    synthesize::{GraphSynthesisEngine, GraphSynthesizer},
    types::{GraphStateRepository, SymbolEdge, SymbolNode},
};

/// The core Knowledge Graph engine.
///
/// Holds a petgraph `DiGraph<SymbolNode, SymbolEdge>` plus the symbol index,
/// obfuscation map, parse-cache repository, and build configuration. Call
/// [`CodeSymbolGraph::build`] to populate/incrementally update the graph.
pub struct CodeSymbolGraph {
    pub graph: DiGraph<SymbolNode, SymbolEdge>,
    /// Composite-key index: `"<rel_path>\0<name>"` → `NodeIndex`.
    pub index: HashMap<String, NodeIndex>,
    /// Obfuscated display path → real relative path.
    pub obfuscated_to_real_path: HashMap<String, String>,
    pub repository: GraphStateRepository,
    pub config: GraphConfig,
    pub(crate) root: PathBuf,
}

impl CodeSymbolGraph {
    /// Creates a new, empty knowledge graph rooted at `root`.
    pub fn new(root: PathBuf) -> Self {
        Self {
            graph: DiGraph::new(),
            index: HashMap::new(),
            obfuscated_to_real_path: HashMap::new(),
            repository: GraphStateRepository::default(),
            config: GraphConfig::default(),
            root,
        }
    }

    /// Scans the workspace and populates (or incrementally updates) the graph
    /// with symbols and references extracted from `.rs`, `.ts`, and `.tsx` files.
    ///
    /// The pipeline is:
    /// 1. **Discover** — walk the workspace, apply exclusions + DoS guards.
    /// 2. **Cache-check** — compare mtime/size against stored metadata.
    /// 3. **Parse** — parallel AST extraction for changed files only.
    /// 4. **Synthesize** — integrate into the live graph, reindex, rebuild edges.
    #[allow(clippy::type_complexity)]
    pub fn build(&mut self, salt: &str) -> Result<bool, GraphError> {
        tracing::info!(
            "🔍 [graph] Building symbol-level knowledge graph for {}...",
            self.root.display()
        );

        let discovery = FileDiscoveryService::default();
        let cache_mgr = CacheManagementService;
        let parser = CodeParsingService;
        let synthesizer = GraphSynthesisEngine;

        // 1. Discovery
        let mut discovered_files = discovery.discover(&self.root)?;

        if discovered_files.len() > MAX_DISCOVERED_FILES {
            tracing::warn!(
                "⚠️ [graph] Workspace size limit exceeded: found {} files, max allowed is {}. Truncating list of files to synthesize.",
                discovered_files.len(),
                MAX_DISCOVERED_FILES
            );
            discovered_files.truncate(MAX_DISCOVERED_FILES);
        }

        // 2. Cache check
        let (to_parse, to_delete) =
            cache_mgr.check_changes(&discovered_files, &self.repository.file_metadata, &self.root);

        // Optimization: return early if no updates and graph is populated
        if to_parse.is_empty() && to_delete.is_empty() && !self.index.is_empty() {
            tracing::info!(
                "✅ [graph] Knowledge graph is already up-to-date. (Nodes: {}, Edges: {})",
                self.graph.node_count(),
                self.graph.edge_count()
            );
            return Ok(true);
        }

        // 3. Parsing
        let updates = parser.parse_files(&to_parse, &self.root)?;

        // 4. Synthesis
        let success = synthesizer.synthesize(self, salt, &to_delete, updates)?;

        Ok(success)
    }

    /// Audits the graph for structural anomalies (dead code — symbols with 0
    /// incoming references, excluding known framework hooks and test/generated paths).
    pub fn find_anomalies(&self) -> Vec<String> {
        let mut anomalies = Vec::new();

        for idx in self.graph.node_indices() {
            if let Some(node) = self.graph.node_weight(idx) {
                let real_path = self
                    .obfuscated_to_real_path
                    .get(&node.path)
                    .map(|p| p.as_str())
                    .unwrap_or(&node.path);

                // Skip declaration files, config files, and generated code
                if real_path.ends_with(".d.ts")
                    || real_path.ends_with("vite.config.ts")
                    || real_path.ends_with("playwright.config.ts")
                    || real_path.ends_with(".test.ts")
                    || real_path.ends_with(".test.tsx")
                    || real_path.ends_with(".spec.ts")
                    || real_path.ends_with(".spec.tsx")
                {
                    continue;
                }

                // Skip backend, shell, and WASM codec crates, plus scratch files
                let path_obj = Path::new(real_path);
                let has_excluded_component = path_obj.components().any(|c| {
                    let name = c.as_os_str().to_string_lossy();
                    self.config.excluded_paths.iter().any(|p| name == *p)
                });
                if has_excluded_component {
                    continue;
                }

                if real_path.contains("pages/")
                    || real_path.contains("components/ui/")
                    || real_path.ends_with("App.tsx")
                    || real_path.contains("main.tsx")
                {
                    continue;
                }

                if self.config.excluded_symbols.contains(&node.name)
                    || node.kind == "module"
                    || node.name == "__module__"
                {
                    continue;
                }

                // Skip entrypoints, tests, and standard route/event handlers
                let name_lower = node.name.to_lowercase();
                if name_lower == "main"
                    || name_lower == "app"
                    || name_lower.contains("test")
                    || name_lower.contains("route")
                    || name_lower.contains("handler")
                    || name_lower.contains("register")
                    || name_lower.contains("force_")
                    || name_lower.contains("invalidate_")
                    || name_lower.contains("persist_")
                    || name_lower.contains("breaker")
                {
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
    fn resolve_path(&self, path: &str) -> String {
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
            let mut visited = std::collections::HashSet::new();
            let mut queue = std::collections::VecDeque::new();
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
    /// exceeds the budget.
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

            if start_tokens > budget {
                // Truncate signature to fit budget using BPE tokenizer if available
                if let Some(tokenizer) = bpe {
                    let tokens = tokenizer.encode_with_special_tokens(&start_clone.signature);
                    if tokens.len() > budget {
                        if let Ok(truncated_text) = tokenizer.decode(&tokens[..budget]) {
                            start_clone.signature = format!("{}...", truncated_text);
                        }
                    }
                } else {
                    let budget_chars = budget * 4;
                    if start_clone.signature.len() > budget_chars {
                        // SEC: Safe truncation at char boundary to prevent panics on multi-byte UTF-8
                        let safe_bound = start_clone.signature.floor_char_boundary(budget_chars);
                        start_clone.signature =
                            format!("{}...", &start_clone.signature[..safe_bound]);
                    }
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

        results
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intelligence::graph::{
        cache::CacheManagementService,
        key::index_key,
        parse::CodeParsingService,
        types::SymbolEdge,
    };
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_empty_blast_radius_nonexistent() {
        let dir = tempdir().unwrap();
        let graph = CodeSymbolGraph::new(dir.path().to_path_buf());
        let affected = graph.calculate_blast_radius("nonexistent", "src/lib.rs");
        assert!(
            affected.is_empty(),
            "Blast radius of nonexistent symbol must be empty"
        );
    }

    #[test]
    fn test_happy_path_symbol_dependency() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("main.rs");

        let mut file = File::create(&file_path).unwrap();
        writeln!(file, "fn helper() {{ }}").unwrap();
        writeln!(file, "fn main() {{ helper(); }}").unwrap();

        let mut graph = CodeSymbolGraph::new(dir.path().to_path_buf());
        let salt = uuid::Uuid::new_v4().to_string();
        graph.build(&salt).unwrap();

        assert!(
            graph.graph.node_count() >= 2,
            "Should index at least 2 symbols"
        );

        let affected = graph.calculate_blast_radius("helper", "main.rs");
        assert!(
            !affected.is_empty(),
            "helper blast radius should not be empty"
        );
        let has_main = affected.iter().any(|node| node.name == "main");
        assert!(has_main, "main should depend on helper");
    }

    #[test]
    fn test_circular_dependency_handling() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("main.rs");

        let mut file = File::create(&file_path).unwrap();
        writeln!(file, "fn alpha() {{ beta(); }}").unwrap();
        writeln!(file, "fn beta() {{ alpha(); }}").unwrap();

        let mut graph = CodeSymbolGraph::new(dir.path().to_path_buf());
        let salt = uuid::Uuid::new_v4().to_string();
        graph.build(&salt).unwrap();

        let affected_alpha = graph.calculate_blast_radius("alpha", "main.rs");
        let affected_beta = graph.calculate_blast_radius("beta", "main.rs");

        assert!(!affected_alpha.is_empty());
        assert!(!affected_beta.is_empty());
    }

    #[test]
    fn test_incremental_ast_caching() {
        let dir = tempdir().unwrap();
        let file_a = dir.path().join("a.rs");
        let file_b = dir.path().join("b.rs");

        let mut f_a = File::create(&file_a).unwrap();
        writeln!(f_a, "fn helper() {{ }}").unwrap();
        drop(f_a);

        let mut f_b = File::create(&file_b).unwrap();
        writeln!(f_b, "fn main() {{ helper(); }}").unwrap();
        drop(f_b);

        let mut graph = CodeSymbolGraph::new(dir.path().to_path_buf());
        let salt = "test_salt".to_string();

        // 1. Initial build
        graph.build(&salt).unwrap();
        assert_eq!(graph.repository.file_metadata.len(), 2);
        assert_eq!(graph.repository.parse_cache.len(), 2);
        assert!(graph.index.contains_key(&index_key("a.rs", "helper")));
        assert!(graph.index.contains_key(&index_key("b.rs", "main")));

        let meta_a_before = *graph.repository.file_metadata.get(&file_a).unwrap();
        let meta_b_before = *graph.repository.file_metadata.get(&file_b).unwrap();

        std::thread::sleep(std::time::Duration::from_millis(10));

        // 2. Modify file_b, keep file_a untouched
        let mut f_b_mod = File::create(&file_b).unwrap();
        writeln!(f_b_mod, "fn main() {{ helper(); // modified comment \n }}").unwrap();
        drop(f_b_mod);

        graph.build(&salt).unwrap();

        let meta_a_after = *graph.repository.file_metadata.get(&file_a).unwrap();
        assert_eq!(meta_a_before, meta_a_after);

        let meta_b_after = *graph.repository.file_metadata.get(&file_b).unwrap();
        assert_ne!(meta_b_before, meta_b_after);

        // 3. Delete file_a and verify cleanup
        std::fs::remove_file(&file_a).unwrap();
        graph.build(&salt).unwrap();

        assert_eq!(graph.repository.file_metadata.len(), 1);
        assert_eq!(graph.repository.parse_cache.len(), 1);
        assert!(!graph.repository.file_metadata.contains_key(&file_a));
        assert!(!graph.repository.parse_cache.contains_key("a.rs"));
        assert!(!graph.index.contains_key(&index_key("a.rs", "helper")));
        assert!(graph.index.contains_key(&index_key("b.rs", "main")));
    }

    #[test]
    fn test_blast_radius_deep_cycle_limit() {
        let dir = tempdir().unwrap();
        let mut graph = CodeSymbolGraph::new(dir.path().to_path_buf());

        let obf_path = "obf/path.rs".to_string();
        graph.obfuscated_to_real_path.insert(obf_path.clone(), "path.rs".to_string());

        let mut indices = Vec::new();
        for i in 1..=55 {
            let name = format!("S_{i}");
            let node = SymbolNode {
                name: name.clone(),
                path: obf_path.clone(),
                kind: "func".to_string(),
                signature: format!("fn S_{i}()"),
                start_line: i,
                end_line: i + 1,
                tokens: 5,
            };
            let idx = graph.graph.add_node(node);
            graph.index.insert(index_key("path.rs", &name), idx);
            indices.push(idx);
        }

        // S_N references S_N-1 (incoming to S_N-1 from S_N)
        for i in 1..55 {
            graph.graph.add_edge(
                indices[i],
                indices[i - 1],
                SymbolEdge { kind: "ref".to_string() },
            );
        }
        // S_1 references S_55
        graph.graph.add_edge(
            indices[0],
            indices[54],
            SymbolEdge { kind: "ref".to_string() },
        );

        let affected = graph.calculate_blast_radius("S_55", "path.rs");
        assert_eq!(affected.len(), 51, "Visited count should respect depth limit of 50 steps");
    }

    #[test]
    fn test_blast_radius_isolated_node() {
        let dir = tempdir().unwrap();
        let mut graph = CodeSymbolGraph::new(dir.path().to_path_buf());

        let obf_path = "obf/path.rs".to_string();
        graph.obfuscated_to_real_path.insert(obf_path.clone(), "path.rs".to_string());

        let node = SymbolNode {
            name: "X".to_string(),
            path: obf_path.clone(),
            kind: "func".to_string(),
            signature: "fn X()".to_string(),
            start_line: 1,
            end_line: 2,
            tokens: 5,
        };
        let idx = graph.graph.add_node(node);
        graph.index.insert(index_key("path.rs", "X"), idx);

        let affected = graph.calculate_blast_radius("X", "path.rs");
        assert_eq!(affected.len(), 1);
        assert_eq!(affected[0].name, "X");
    }

    #[test]
    fn test_full_cycle_with_mixed_changes() {
        let dir = tempdir().unwrap();
        let file_a = dir.path().join("a.rs");
        let file_b = dir.path().join("b.rs");
        let file_c = dir.path().join("c.rs");

        std::fs::write(&file_a, "fn a_func() {}").unwrap();
        std::fs::write(&file_b, "fn b_func() { a_func(); }").unwrap();
        std::fs::write(&file_c, "fn c_func() {}").unwrap();

        let mut graph = CodeSymbolGraph::new(dir.path().to_path_buf());
        let salt = "salt".to_string();

        graph.build(&salt).unwrap();
        assert_eq!(graph.repository.file_metadata.len(), 3);
        assert_eq!(graph.repository.parse_cache.len(), 3);
        assert!(graph.index.contains_key(&index_key("a.rs", "a_func")));
        assert!(graph.index.contains_key(&index_key("b.rs", "b_func")));
        assert!(graph.index.contains_key(&index_key("c.rs", "c_func")));

        let meta_b_before = *graph.repository.file_metadata.get(&file_b).unwrap();
        let meta_c_before = *graph.repository.file_metadata.get(&file_c).unwrap();

        std::thread::sleep(std::time::Duration::from_millis(10));

        std::fs::write(&file_b, "fn b_func() { c_func(); } // modified").unwrap();
        std::fs::remove_file(&file_a).unwrap();
        let file_d = dir.path().join("d.rs");
        std::fs::write(&file_d, "fn d_func() {}").unwrap();

        graph.build(&salt).unwrap();

        assert_eq!(graph.repository.file_metadata.len(), 3);
        assert_eq!(graph.repository.parse_cache.len(), 3);
        assert!(!graph.repository.file_metadata.contains_key(&file_a));
        assert!(!graph.repository.parse_cache.contains_key("a.rs"));
        assert!(graph.repository.file_metadata.contains_key(&file_b));
        assert!(graph.repository.file_metadata.contains_key(&file_c));
        assert!(graph.repository.file_metadata.contains_key(&file_d));

        let meta_b_after = *graph.repository.file_metadata.get(&file_b).unwrap();
        let meta_c_after = *graph.repository.file_metadata.get(&file_c).unwrap();
        assert_ne!(meta_b_before, meta_b_after);
        assert_eq!(meta_c_before, meta_c_after);

        assert!(!graph.index.contains_key(&index_key("a.rs", "a_func")));
        assert!(graph.index.contains_key(&index_key("b.rs", "b_func")));
        assert!(graph.index.contains_key(&index_key("c.rs", "c_func")));
        assert!(graph.index.contains_key(&index_key("d.rs", "d_func")));

        let affected_c = graph.calculate_blast_radius("c_func", "c.rs");
        assert!(affected_c.iter().any(|node| node.name == "b_func"));

        // Verify I/O error propagation for a missing file
        std::fs::remove_file(&file_b).unwrap();
        let files_list = vec![file_c, file_d, file_b.clone()];
        let cache_mgr = CacheManagementService;
        let (to_parse, _to_delete) =
            cache_mgr.check_changes(&files_list, &graph.repository.file_metadata, &graph.root);
        assert!(to_parse.contains(&file_b));

        let parser = CodeParsingService;
        let parse_res = parser.parse_files(&to_parse, &graph.root);
        assert!(parse_res.is_err(), "Unreadable or missing file should result in GraphError");
    }

    #[test]
    fn test_typescript_import_export_handling() {
        let dir = tempdir().unwrap();
        let file_a = dir.path().join("a.tsx");
        let file_b = dir.path().join("b.tsx");

        std::fs::write(&file_a, "export function foo() { return 42; }").unwrap();
        std::fs::write(&file_b, "import { foo } from './a';\nexport function bar() { foo(); }").unwrap();

        let mut graph = CodeSymbolGraph::new(dir.path().to_path_buf());
        let salt = uuid::Uuid::new_v4().to_string();
        graph.build(&salt).unwrap();

        assert!(graph.index.contains_key(&index_key("a.tsx", "foo")));
        assert!(graph.index.contains_key(&index_key("b.tsx", "bar")));

        let affected = graph.calculate_blast_radius("foo", "a.tsx");
        assert!(affected.iter().any(|node| node.name == "bar"));
    }

    #[test]
    fn test_typescript_circular_dependency() {
        let dir = tempdir().unwrap();
        let file_a = dir.path().join("a.tsx");
        let file_b = dir.path().join("b.tsx");

        std::fs::write(&file_a, "import { bar } from './b';\nexport function foo() { bar(); }").unwrap();
        std::fs::write(&file_b, "import { foo } from './a';\nexport function bar() { foo(); }").unwrap();

        let mut graph = CodeSymbolGraph::new(dir.path().to_path_buf());
        let salt = uuid::Uuid::new_v4().to_string();
        graph.build(&salt).unwrap();

        assert!(graph.index.contains_key(&index_key("a.tsx", "foo")));
        assert!(graph.index.contains_key(&index_key("b.tsx", "bar")));

        let affected_foo = graph.calculate_blast_radius("foo", "a.tsx");
        assert!(affected_foo.iter().any(|node| node.name == "bar"));

        let affected_bar = graph.calculate_blast_radius("bar", "b.tsx");
        assert!(affected_bar.iter().any(|node| node.name == "foo"));
    }

    #[test]
    fn test_token_budgeted_context_resolution() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("main.rs");

        let mut file = File::create(&file_path).unwrap();
        writeln!(file, "fn helper() {{ }}").unwrap();
        writeln!(file, "fn main() {{ helper(); }}").unwrap();

        let mut graph = CodeSymbolGraph::new(dir.path().to_path_buf());
        let salt = uuid::Uuid::new_v4().to_string();
        graph.build(&salt).unwrap();

        // Large budget — both symbols should fit
        let resolved_large = graph.resolve_context("helper", "main.rs", 1000, None);
        assert_eq!(resolved_large.len(), 2);

        // Tiny budget — only target should be returned and its signature truncated
        let resolved_small = graph.resolve_context("helper", "main.rs", 2, None);
        assert!(resolved_small[0].signature.ends_with("..."));
    }
}



// Metadata: [engine]

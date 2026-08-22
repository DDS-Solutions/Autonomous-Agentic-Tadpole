//! @docs ARCHITECTURE:Core
//!
//! ### AI Assist Note
//! **engine**: Core technical resource for the Tadpole OS infrastructure.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Unhandled errors, lock contention, or connection staling.
//! - **Telemetry Link**: Search `[engine]` in tracing logs.
//! - **Trace Scope**: `server-rs::intelligence::graph::engine`
//! 
//! Primary orchestrator for the Knowledge Graph engine.

pub mod analysis;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use petgraph::graph::{DiGraph, NodeIndex};

use crate::intelligence::graph::{
    cache::{CacheManagementService, CacheManager},
    config::GraphConfig,
    constants::MAX_DISCOVERED_FILES,
    discover::{FileDiscoveryService, FileDiscoverer},
    error::GraphError,
    parse::{CodeParsingService, CodeParser},
    synthesize::{GraphSynthesisEngine, GraphSynthesizer},
    types::{GraphStateRepository, SymbolEdge, SymbolNode},
};

/// The core Knowledge Graph engine.
///
/// Holds a petgraph `DiGraph<SymbolNode, SymbolEdge>` plus the symbol index,
/// obfuscation map, parse-cache repository, and build configuration. Call
/// [`CodeSymbolGraph::build`] to populate/incrementally update the graph.
pub struct CodeSymbolGraph {
    pub(crate) graph: DiGraph<SymbolNode, SymbolEdge>,
    /// Composite-key index: `"<rel_path>\0<name>"` → `NodeIndex`.
    pub(crate) index: HashMap<String, NodeIndex>,
    /// Obfuscated display path → real relative path.
    pub(crate) obfuscated_to_real_path: HashMap<String, String>,
    pub(crate) repository: GraphStateRepository,
    pub(crate) config: GraphConfig,
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

    pub fn graph(&self) -> &DiGraph<SymbolNode, SymbolEdge> {
        &self.graph
    }

    pub fn index(&self) -> &HashMap<String, NodeIndex> {
        &self.index
    }

    pub fn obfuscated_to_real_path(&self) -> &HashMap<String, String> {
        &self.obfuscated_to_real_path
    }

    pub fn repository(&self) -> &GraphStateRepository {
        &self.repository
    }

    pub fn config(&self) -> &GraphConfig {
        &self.config
    }

    pub fn root(&self) -> &Path {
        &self.root
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
}

// Metadata: [engine]

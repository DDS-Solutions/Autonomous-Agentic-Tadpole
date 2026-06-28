//! @docs ARCHITECTURE:Core
//!
//! ### AI Assist Note
//! **types**: Core technical resource for the Tadpole OS infrastructure.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Unhandled errors, lock contention, or connection staling.
//! - **Telemetry Link**: Search `[types]` in tracing logs.
//! - **Trace Scope**: `server-rs::intelligence::graph::types`

//! Core domain types for the `CodeSymbolGraph`.
//!
//! Defines the node/edge payload types used by the petgraph `DiGraph`,
//! and the `GraphStateRepository` that caches parsed AST data between builds.

use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::HashMap;
use std::path::PathBuf;

/// A node in the knowledge graph representing a code symbol
/// (function, struct, trait, interface, etc.).
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct SymbolNode {
    pub name: String,
    pub path: String,
    pub kind: String,
    pub signature: String,
    pub start_line: u32,
    pub end_line: u32,
    #[specta(type = u32)]
    pub tokens: usize,
}

/// An edge in the knowledge graph representing a dependency between symbols.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct SymbolEdge {
    pub kind: String,
}

/// Repository containing the cached AST parse structures and file metadata.
/// Acts as the incremental-rebuild cache: only files whose mtime or size has
/// changed since the last `build()` call are re-parsed.
#[derive(Clone)]
pub struct GraphStateRepository {
    /// Maps absolute `PathBuf` → `(mtime, size)` for each tracked file.
    pub file_metadata: HashMap<PathBuf, (std::time::SystemTime, u64)>,
    /// Maps relative Unix path → `(symbols, references)` parsed from that file.
    pub parse_cache: HashMap<
        String,
        (
            Vec<crate::utils::parser::Symbol>,
            Vec<crate::utils::parser::Reference>,
        ),
    >,
}

impl Default for GraphStateRepository {
    fn default() -> Self {
        Self {
            file_metadata: HashMap::new(),
            parse_cache: HashMap::new(),
        }
    }
}

// Metadata: [types]

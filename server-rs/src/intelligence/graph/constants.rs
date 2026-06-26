//! Hard upper-bound constants for the `CodeSymbolGraph`.
//!
//! These values guard against resource exhaustion (DoS) caused by
//! abnormally large workspaces or adversarial graph topologies.

/// Maximum number of files to discover in a single workspace.
pub const MAX_DISCOVERED_FILES: usize = 10_000;

/// Maximum number of nodes allowed in the `CodeSymbolGraph`.
pub const MAX_NODES: usize = 20_000;

/// Maximum number of edges allowed in the `CodeSymbolGraph`.
pub const MAX_EDGES: usize = 100_000;

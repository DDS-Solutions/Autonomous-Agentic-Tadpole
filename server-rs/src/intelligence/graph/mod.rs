//! @docs ARCHITECTURE:Core
//!
//! ### AI Assist Note
//! **mod**: Core technical resource for the Tadpole OS infrastructure.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Unhandled errors, lock contention, or connection staling.
//! - **Telemetry Link**: Search `[mod]` in tracing logs.
//! - **Trace Scope**: `server-rs::intelligence::graph::mod`
//! 
//! Public API surface for the `graph` module.
//!
//! All types and traits needed by consumers of the intelligence layer
//! are re-exported here. Internal helpers (e.g., `path::sanitize_log_path`)
//! remain `pub(crate)` or private and are **not** surfaced.

#![allow(unused_imports)]


pub mod cache;
pub mod config;
pub mod constants;
pub mod discover;
pub mod engine;
pub mod error;
pub mod key;
pub mod parse;
pub mod path;
pub mod synthesize;
pub mod types;

// ── Primary orchestrator ─────────────────────────────────────────────────────
pub use engine::CodeSymbolGraph;

// ── Error & result ───────────────────────────────────────────────────────────
pub use error::{GraphError, GraphResult};

// ── Configuration ────────────────────────────────────────────────────────────
pub use config::GraphConfig;

// ── Domain types ─────────────────────────────────────────────────────────────
pub use types::{GraphStateRepository, SymbolEdge, SymbolNode};

// ── Key normalisation ────────────────────────────────────────────────────────
pub use key::{DefaultKeyNormalizer, KeyNormalizer, index_key};

// ── Service traits ───────────────────────────────────────────────────────────
pub use cache::CacheManager;
pub use discover::FileDiscoverer;
pub use parse::CodeParser;
pub use synthesize::GraphSynthesizer;

// ── Service implementations ──────────────────────────────────────────────────
pub use cache::CacheManagementService;
pub use discover::FileDiscoveryService;
pub use parse::CodeParsingService;
pub use synthesize::GraphSynthesisEngine;

// ── Constants ────────────────────────────────────────────────────────────────
pub use constants::{MAX_DISCOVERED_FILES, MAX_EDGES, MAX_NODES};

// ── Path utilities ───────────────────────────────────────────────────────────
pub use path::obfuscate_path;

// Metadata: [mod]

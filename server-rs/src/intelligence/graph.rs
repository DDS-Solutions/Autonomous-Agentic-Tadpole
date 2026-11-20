//! @docs ARCHITECTURE:CodeBaseIntelligence
//!
//! ### AI Assist Note
//! **Code Symbol Graph Facade**: Re-exports `CodeSymbolGraph`, blast-radius analysis,
//! and graph sub-modules from the `tadpole-intelligence` workspace crate.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Graph traversal bounds exceeded or path resolution mismatch.
//! - **Telemetry Link**: Search `[graph]` in tracing logs.

pub use tadpole_intelligence::graph::*;

// Metadata: [graph]

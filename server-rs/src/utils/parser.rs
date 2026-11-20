//! @docs ARCHITECTURE:CodeBaseIntelligence
//!
//! ### AI Assist Note
//! **Symbol Extraction Facade**: Re-exports Tree-sitter AST parsing primitives from the
//! `tadpole-intelligence` workspace crate.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Tree-sitter grammar load error.
//! - **Telemetry Link**: Search `[parser]` in tracing logs.

pub use tadpole_intelligence::parser::*;

// Metadata: [parser]

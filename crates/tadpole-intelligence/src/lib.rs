//! @docs ARCHITECTURE:CodeBaseIntelligence
//!
//! ### AI Assist Note
//! **Tadpole Intelligence Workspace Crate**: Encapsulates Tree-sitter AST symbol extraction,
//! petgraph dependency indexing, cooperative-cancellation blast-radius BFS, and token-budgeted
//! context resolution in an independently compiled workspace crate.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Parse cache miss, graph traversal truncation, or grammar mismatch.
//! - **Telemetry Link**: Search `[tadpole_intelligence]` in tracing logs.

pub mod graph;
pub mod parser;

pub use graph::*;
pub use parser::{Reference, Symbol, SymbolExtractor, SymbolRange};

// Metadata: [tadpole_intelligence]

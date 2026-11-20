//! @docs ARCHITECTURE:Core
//!
//! ### AI Assist Note
//! **RAG Scoring Facade**: Re-exports multi-factor memory scoring primitives from the
//! `tadpole-types` workspace crate.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Score normalization drift.
//! - **Telemetry Link**: Search `[rag_scoring]` in tracing logs.

pub use tadpole_types::rag_scoring::*;

// Metadata: [rag_scoring]

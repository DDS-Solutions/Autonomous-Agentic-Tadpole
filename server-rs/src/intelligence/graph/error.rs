//! @docs ARCHITECTURE:Core
//!
//! ### AI Assist Note
//! **error**: Core technical resource for the Tadpole OS infrastructure.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Unhandled errors, lock contention, or connection staling.
//! - **Telemetry Link**: Search `[error]` in tracing logs.
//! - **Trace Scope**: `server-rs::intelligence::graph::error`
//! 
//! Error types for the `CodeSymbolGraph` module.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Runtime logic error, state desynchronization, or resource exhaustion.
//! - **Telemetry Link**: Search `[graph]` in tracing logs.

/// Unified error type for all graph operations.
#[derive(Debug, thiserror::Error)]
pub enum GraphError {
    #[error("Workspace root not found: {0}")]
    WorkspaceRootNotFound(String),
    #[error("Path lies outside workspace boundary: {0}")]
    PathOutOfBounds(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Invalid key normalizer state: {0}")]
    KeyNormalization(String),
    #[error("Internal error: {0}")]
    Internal(String),
}

/// Convenience `Result` alias for graph operations.
#[allow(dead_code)]
pub type GraphResult<T> = Result<T, GraphError>;

// Metadata: [error]

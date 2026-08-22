//! @docs ARCHITECTURE:State
//!
//! ### AI Assist Note
//! **System Services**: Orchestrates the core background processes and
//! long-running logic for the Tadpole OS engine. Features the
//! **Service Layer** pattern: separates business rules from
//! transport-specific route handlers. Includes **Discovery** (agent
//! registry scanning) and **Privacy** (data anonymization) services.
//! AI agents should utilize these services for cross-cutting logic
//! that exceeds the scope of a single request handler (SERV-01).
//!
//! ### 🔍 Debugging & Observability
//! - **Telemetry Link**: Search `[mod]` in tracing logs.
//! - **Failure Path**: Service-init timeouts, state-drift between
//!   in-memory registries and persistent storage, or performance
//!   bottlenecks during large-scale discovery scans.
//! - **Trace Scope**: `server-rs::services`

pub mod discovery;
pub mod privacy;
pub mod acl_service;
pub mod parser;
pub mod bm25_memory;
pub mod rag_fusion;

#[cfg(test)]
mod tests;





// Metadata: [mod]

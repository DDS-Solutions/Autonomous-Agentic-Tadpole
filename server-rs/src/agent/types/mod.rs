//! @docs ARCHITECTURE:Agent
//!
//! ### AI Assist Note
//! **Unified Agent Schemas Module Root**: Re-exports pure domain contracts from the
//! `tadpole-types` workspace crate alongside `AppState`-coupled `EngineAgent` and `ToolContext` runtime types.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Serialization drift between Rust engine and TypeScript frontend.
//! - **Telemetry Link**: Search `[types]` in tracing logs.

mod agent;
#[cfg(test)]
mod tests;

pub use agent::*;
pub use crate::agent::merge::AgentConfigUpdate;
pub use tadpole_types::{enums::*, mission::*, model::*};

// Metadata: [types]

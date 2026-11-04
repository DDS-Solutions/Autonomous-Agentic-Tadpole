//! @docs ARCHITECTURE:AgenticEngine
//!
//! ### AI Assist Note
//! Agentic Engine route facade preserving the existing public handler paths.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Route family module or SQL operation failure.
//! - **Telemetry Link**: Search [agentic_engine] in tracing logs.

mod maintenance;
mod receipts;
mod skills;

pub use maintenance::*;
pub use receipts::*;
pub use skills::*;

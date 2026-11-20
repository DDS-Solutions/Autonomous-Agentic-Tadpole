//! @docs ARCHITECTURE:Agent
//!
//! ### AI Assist Note
//! **Tadpole Types Workspace Crate**: Self-contained domain DTOs, enums, model capability matrices,
//! mission contracts, and RAG scoring primitives shared across workspace crates to minimize incremental
//! recompilation overhead.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Serialization mismatch or schema drift across crate boundaries.
//! - **Telemetry Link**: Search `[tadpole_types]` in tracing logs.

pub mod enums;
pub mod mission;
pub mod model;
pub mod rag_scoring;

pub use enums::*;
pub use mission::*;
pub use model::*;
pub use rag_scoring::*;

// Metadata: [tadpole_types]

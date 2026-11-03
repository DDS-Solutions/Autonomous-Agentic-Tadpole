//! @docs ARCHITECTURE:Runner
//!
//! ### AI Assist Note
//! **Modular Prompt Synthesizer Facade**: Orchestrates the multi-stage modular prompt
//! synthesis pipeline, BPE tokenizer caching, token compression, capability-based tool manifestation,
//! repo map generation, and security-fenced context assembly.
//! - **System Identity**: TadpoleOS/1.1.59
//!
//! ### 🛡️ Hardened Protocols
//! - **Context Pruning** (PERF-07): Precise context-window-aware prompt truncation.
//! - **Hierarchy Enforcement** (SEC-06): Specialized access control (e.g., blocking
//!   Agent 1 from direct filesystem tools to enforce delegation).
//! - **Identity Masking**: Specialists are anonymized in cluster directories for
//!   high-level managers (OVERLORD/ORCHESTRATOR) to prioritize strategic delegation.
//! - **Safe Mode** (SEC-05): Strict capability allow-list for read-only tools.
//! - **Untrusted Data Fencing**: Fences external/peer inputs with secret redaction.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Tokenizer failure, mission context retrieval fault, or identity lookup cache miss.
//! - **Telemetry Link**: Search `[synthesis]` in tracing logs.
//! - **Trace Scope**: `server-rs::agent::runner::synthesis`

pub mod budget;
pub mod context;
pub mod prompt;
pub mod repo_map;
#[cfg(test)]
pub mod tests;
pub mod tools;

// Re-export budget & tokenizer utilities
pub use budget::{
    count_tokens_fast, format_budgeted_items, BPE_TOKENIZER, SYSTEM_TOKEN_REGEXES,
};

// Re-export safe mode tools allow-list
pub use tools::SAFE_MODE_ALLOWED_TOOLS;

// Re-export identity classifier
pub use prompt::is_orchestrator_identity;

// Metadata: [synthesis]


//! @docs ARCHITECTURE:Continuity
//!
//! ### AI Assist Note
//! **Continuity Subsystem**: Orchestrates the background persistence
//! and long-running mission scheduling for the Tadpole OS engine.
//! Features **Scheduled Mission Execution**: manages the lifecycle of
//! recurring tasks and long-lived agent workflows. Implements
//! **Mission Recovery**: ensures that interrupted missions can be
//! resumed or rolled back based on the `ContinuityState`. AI agents
//! should utilize this module for tasks that exceed the scope of a
//! single user-interactive session (CONT-01).
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Job execution stalls due to worker pool
//!   exhaustion, database deadlocks during state-persistence, or
//!   cron-expression validation failures.
//! - **Trace Scope**: `server-rs::agent::continuity`

pub mod executor;
pub mod scheduler;
pub mod types;
pub mod workflow;
pub mod ssd;
pub mod arbiter;
#[cfg(test)]
mod sscp_tests;
#[cfg(test)]
mod sscp_smoke;

pub use ssd::{SSDManager, ContextBlock};
pub use arbiter::ContextArbiter;

use tiktoken_rs::cl100k_base;

/// Partitions a long history string into manageable SSCP ContextBlocks.
/// Returns an empty vec if the tokenizer fails to initialize.
pub fn partition_context(agent_id: &str, mission_id: &str, history: &str) -> Vec<ContextBlock> {
    let bpe = match cl100k_base() {
        Ok(b) => b,
        Err(e) => {
            tracing::error!("🚨 [SSCP] Tokenizer init failed: {:?}. Skipping partitioning.", e);
            return Vec::new();
        }
    };
    let tokens = bpe.encode_with_special_tokens(history);
    
    // Chunk size: 2000 tokens (SSCP Standard)
    const CHUNK_SIZE: usize = 2000;
    let mut blocks = Vec::new();
    
    for (i, chunk) in tokens.chunks(CHUNK_SIZE).enumerate() {
        let chunk_text = bpe.decode(chunk).unwrap_or_default();
        blocks.push(ContextBlock {
            id: format!("block-{}", i),
            agent_id: agent_id.to_string(),
            mission_id: mission_id.to_string(),
            tokens: vec![chunk_text],
            metadata: std::collections::HashMap::new(),
            timestamp: chrono::Utc::now().timestamp(),
        });
    }
    
    blocks
}

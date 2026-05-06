//! @docs ARCHITECTURE:Agent
//!
//! ### AI Assist Note
//! **Context Density Orchestrator**: Manages context window saturation and
//! performing **Tiered Compression**. Features heurstic compaction
//! (Tier 1) for redundancy removal and semantic summarization (Tier 2)
//! when mission history exceeds token thresholds (80% of `max_tokens`).
//! Uses `tiktoken-rs` for precise parity with OpenAI tokenization.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Token limit breach due to high-latency summarization
//!   loop, or loss of fact-preservation (file paths, error codes)
//!   during heuristic compaction.
//! - **Trace Scope**: `server-rs::agent::context_manager`

use crate::agent::runner::{AgentRunner, RunContext};
use crate::error::AppError;
use tiktoken_rs::{cl100k_base, CoreBPE};
use once_cell::sync::Lazy;

static TOKENIZER: Lazy<CoreBPE> = Lazy::new(|| {
    cl100k_base().expect("Tadpole Error: Failed to initialize cl100k_base tokenizer. Check tiktoken-rs dependencies.")
});

pub struct ContextManager;

impl ContextManager {
    /// Applies deterministic rules to reduce history size before LLM summarization.
    ///
    /// Focuses on removing redundant CLI output, conversational filler,
    /// and collapsing repeated failed attempts while preserving Atomic Blocks.
    pub fn compact(history: &str) -> String {
        let lines: Vec<&str> = history.lines().collect();
        let mut result = Vec::new();
        
        let mut in_code_block = false;
        let mut block_buffer = Vec::new();

        for line in lines {
            let trimmed = line.trim();
            
            // 1. Atomic Block Detection: Code Blocks
            if trimmed.starts_with("```") {
                in_code_block = !in_code_block;
                block_buffer.push(line);
                if !in_code_block {
                    // Just finished an atomic block, flush the buffer
                    result.extend(block_buffer.drain(..));
                }
                continue;
            }

            if in_code_block {
                block_buffer.push(line);
                continue;
            }

            // 2. Collapse repeating "Success" or "Error" lines from tool calls
            if trimmed.contains("tool_result") && trimmed.contains("Success") {
                if let Some(last) = result.last() {
                    if last.trim().contains("tool_result") && last.trim().contains("Success") {
                        continue;
                    }
                }
            }

            result.push(line);
        }

        // Safety: Flush any unclosed block buffer
        result.extend(block_buffer);

        result.join("\n")
    }
}

impl ContextManager {
    /// Calculates the token count of a given text content.
    pub fn calculate_tokens(text: &str) -> usize {
        TOKENIZER.encode_with_special_tokens(text).len()
    }

    /// Performs tiered history compression.
    ///
    /// Tier 1: Local Heuristics (HeuristicCompactor)
    /// Tier 2: Semantic Summarization (LLM)
    pub async fn summarize_history(
        runner: &AgentRunner,
        ctx: &RunContext,
        history: &str,
    ) -> Result<String, AppError> {
        // --- Tier 1: Local Heuristics ---
        let heuristically_compacted = Self::compact(history);

        tracing::info!(
            "🧠 [ContextManager] Tier 1 Compaction: {} -> {} tokens",
            Self::calculate_tokens(history),
            Self::calculate_tokens(&heuristically_compacted)
        );

        // Generate the Sovereign State Manifest for context anchoring
        let manifest = crate::system::manifest::SovereignStateManifest::generate(&runner.state).await;

        // --- Tier 2: Semantic Summarization ---
        let summarization_prompt = format!(
            "You are the Context Management Engine for Tadpole OS.\n\n\
             ### CURRENT SYSTEM STATE:\n\
             {}\n\n\
             ### MISSION OBJECTIVE:\n\
             Summarize the following mission history into a concise, high-density 'Condensed State'. \
             Preserve all critical findings, file paths, and established facts. \
             Remove conversational filler and redundant reasoning.\n\n\
             ### MISSION HISTORY:\n\
             {}\n\n\
             ### OUTPUT FORMAT:\n\
             Provide ONLY the condensed summary. Do not include any meta-commentary.",
            manifest,
            heuristically_compacted
        );

        let (summary, _, _) = runner
            .call_provider_for_synthesis(ctx, &summarization_prompt, None)
            .await?;

        tracing::info!(
            "✅ [ContextManager] Tier 2 Compaction complete. Final length: {} tokens",
            Self::calculate_tokens(&summary)
        );

        Ok(summary)
    }
}

// Metadata: [context_manager]

// Metadata: [context_manager]

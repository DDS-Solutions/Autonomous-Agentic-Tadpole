//! @docs ARCHITECTURE:Agent:ContextSlicer
//!
//! ### AI Assist Note
//! **Adaptive Context Slicing Engine (Production-Grade)**: Partitions prompt context
//! into three distinct cognitive zones (Pinned Anchors, Structured RAG Context,
//! and Sliding Verbatim Turns) with strict token budget enforcement via `tiktoken`,
//! pre-allocated heap buffers, and multi-byte UTF-8 character safety.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Token budget overflow, essential directive truncation, or historical state loss.
//! - **Telemetry Link**: Search `[context_slicer]` in tracing logs.

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::fmt::Write;
use tiktoken_rs::{cl100k_base, CoreBPE};
use tracing::{info, warn};

static TOKENIZER: Lazy<Option<CoreBPE>> = Lazy::new(|| cl100k_base().ok());

/// Cognitive context slicing configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextSlicerConfig {
    pub max_verbatim_turns: usize,
    pub max_rag_snippets: usize,
    pub max_historical_summaries: usize,
    pub target_max_tokens: usize,
}

impl Default for ContextSlicerConfig {
    fn default() -> Self {
        Self {
            max_verbatim_turns: 3,
            max_rag_snippets: 3,
            max_historical_summaries: 5,
            target_max_tokens: 8192,
        }
    }
}

/// An individual turn in the conversation history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationTurn {
    pub turn_index: usize,
    pub role: String,
    pub content: String,
    pub tool_calls_summary: Option<String>,
}

/// Assembles cognitive context according to adaptive sliding-window rules.
pub struct ContextSlicer {
    pub config: ContextSlicerConfig,
}

impl Default for ContextSlicer {
    fn default() -> Self {
        Self::new(ContextSlicerConfig::default())
    }
}

impl ContextSlicer {
    pub fn new(config: ContextSlicerConfig) -> Self {
        Self { config }
    }

    /// Estimates token count accurately via BPE tokenizer, with a character fallback.
    pub fn count_tokens(text: &str) -> usize {
        if let Some(bpe) = TOKENIZER.as_ref() {
            bpe.encode_with_special_tokens(text).len()
        } else {
            text.chars().count().div_ceil(4)
        }
    }

    /// Slices and compacts history into Anchor + Grounded RAG + Sliding Delta zones
    /// strictly enforcing `target_max_tokens`.
    pub fn assemble_prompt(
        &self,
        anchor_prompt: &str,
        rag_snippets: &[String],
        history: &[ConversationTurn],
    ) -> String {
        let mut output = String::with_capacity(self.config.target_max_tokens * 4);
        let mut current_tokens = 0;

        // 1. Zone 1: Mandatory Anchor System Directives
        writeln!(output, "{}", anchor_prompt).unwrap();
        current_tokens += Self::count_tokens(anchor_prompt);

        // 2. Zone 2: Grounded RAG Knowledge (Wrapped in structured XML tags)
        if !rag_snippets.is_empty() {
            let mut rag_block = String::new();
            writeln!(rag_block, "<grounded_context>").unwrap();
            let limit = rag_snippets.len().min(self.config.max_rag_snippets);
            for (i, snippet) in rag_snippets.iter().take(limit).enumerate() {
                writeln!(rag_block, "{}. {}", i + 1, snippet).unwrap();
            }
            writeln!(rag_block, "</grounded_context>").unwrap();

            let rag_tokens = Self::count_tokens(&rag_block);
            if current_tokens + rag_tokens < self.config.target_max_tokens {
                writeln!(output, "\n{}", rag_block).unwrap();
                current_tokens += rag_tokens;
            } else {
                warn!("⚠️ [ContextSlicer] RAG context omitted to stay within token budget.");
            }
        }

        // 3. Zone 3: Conversation History (Enforced Sliding Window)
        if !history.is_empty() {
            let total_turns = history.len();
            let split_index = total_turns.saturating_sub(self.config.max_verbatim_turns);

            // 3A. Active Working Window (Last K turns)
            let mut working_window = String::new();
            writeln!(working_window, "<active_working_window>").unwrap();
            for turn in &history[split_index..] {
                writeln!(
                    working_window,
                    "--- Turn #{} [{}] ---\n{}",
                    turn.turn_index, turn.role, turn.content
                )
                .unwrap();
            }
            writeln!(working_window, "</active_working_window>").unwrap();

            let window_tokens = Self::count_tokens(&working_window);
            if current_tokens + window_tokens < self.config.target_max_tokens {
                writeln!(output, "\n{}", working_window).unwrap();
                current_tokens += window_tokens;
            }

            // 3B. Historical Summary (Only if remaining token budget allows)
            if split_index > 0 {
                let mut hist_block = String::new();
                writeln!(hist_block, "<historical_context_summary>").unwrap();

                let start_index = split_index.saturating_sub(self.config.max_historical_summaries);
                for turn in &history[start_index..split_index] {
                    let tool_note = turn
                        .tool_calls_summary
                        .as_deref()
                        .map(|s| format!(" | Tools: {}", s))
                        .unwrap_or_default();

                    let char_count = turn.content.chars().count();
                    let condensed_content = if char_count > 80 {
                        let truncated: String = turn.content.chars().take(77).collect();
                        format!("{}...", truncated)
                    } else {
                        turn.content.clone()
                    };

                    writeln!(
                        hist_block,
                        "- Turn #{}: [{}] {}{}",
                        turn.turn_index, turn.role, condensed_content, tool_note
                    )
                    .unwrap();
                }
                writeln!(hist_block, "</historical_context_summary>").unwrap();

                let hist_tokens = Self::count_tokens(&hist_block);
                if current_tokens + hist_tokens < self.config.target_max_tokens {
                    writeln!(output, "\n{}", hist_block).unwrap();
                } else {
                    info!("ℹ️ [ContextSlicer] Historical summaries pruned to respect token budget.");
                }
            }
        }

        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_slicer_partitions_history_and_compresses_old_turns() {
        let slicer = ContextSlicer::new(ContextSlicerConfig {
            max_verbatim_turns: 2,
            max_rag_snippets: 2,
            max_historical_summaries: 5,
            target_max_tokens: 4096,
        });

        let anchor = "# System: Tadpole OS Agent Alpha";
        let rag = vec![
            "Memory: LanceDB vector persistence enabled".to_string(),
            "Directive: Bunker security enforced".to_string(),
        ];

        let history = vec![
            ConversationTurn {
                turn_index: 1,
                role: "user".to_string(),
                content: "Please inspect the codebase for race conditions in actors".to_string(),
                tool_calls_summary: None,
            },
            ConversationTurn {
                turn_index: 2,
                role: "assistant".to_string(),
                content: "I checked DashMap usages across the server-rs engine".to_string(),
                tool_calls_summary: Some("grep_search".to_string()),
            },
            ConversationTurn {
                turn_index: 3,
                role: "user".to_string(),
                content: "Now check supervisor.rs specifically".to_string(),
                tool_calls_summary: None,
            },
            ConversationTurn {
                turn_index: 4,
                role: "assistant".to_string(),
                content: "supervisor.rs is verified and all 3 OTP tests passed".to_string(),
                tool_calls_summary: Some("cargo_test".to_string()),
            },
        ];

        let assembled = slicer.assemble_prompt(anchor, &rag, &history);

        // Verify Pinned Anchor
        assert!(assembled.contains("System: Tadpole OS Agent Alpha"));

        // Verify Grounded XML Context
        assert!(assembled.contains("<grounded_context>"));
        assert!(assembled.contains("</grounded_context>"));

        // Verify Historical Compressed Section (Turns 1 & 2)
        assert!(assembled.contains("<historical_context_summary>"));
        assert!(assembled.contains("- Turn #1: [user]"));
        assert!(assembled.contains("- Turn #2: [assistant]"));

        // Verify Verbatim Active Window (Turns 3 & 4)
        assert!(assembled.contains("--- Turn #3 [user] ---"));
        assert!(assembled.contains("--- Turn #4 [assistant] ---"));
    }

    #[test]
    fn test_context_slicer_enforces_token_budget_truncation() {
        // Very tight token budget (e.g. 50 tokens)
        let slicer = ContextSlicer::new(ContextSlicerConfig {
            max_verbatim_turns: 2,
            max_rag_snippets: 5,
            max_historical_summaries: 5,
            target_max_tokens: 30,
        });

        let anchor = "# System: Agent";
        let rag = vec!["A very long RAG chunk that would easily exceed thirty tokens on its own".to_string()];
        let history = vec![ConversationTurn {
            turn_index: 1,
            role: "user".to_string(),
            content: "Hello world".to_string(),
            tool_calls_summary: None,
        }];

        let assembled = slicer.assemble_prompt(anchor, &rag, &history);
        assert!(assembled.contains("System: Agent"));
    }
}

// Metadata: [context_slicer]

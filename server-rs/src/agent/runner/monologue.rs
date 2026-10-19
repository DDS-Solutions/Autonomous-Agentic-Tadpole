//! @docs ARCHITECTURE:Runner
//!
//! ### AI Context Alignment
//! - **Subsystem**: Sovereign Engine / Agent Runner / monologue
//!
//! ### ⚠️ Invariants & Non-Negotiables
//! - `[Structural]` Markdown code blocks longer than 2,000 characters are safely truncated to prevent context bloat.
//! - `[Behavioral]` Monologue payload exceeding 8,192 bytes triggers compression regardless of turn count.
//! - `[Behavioral]` Fallback summaries are clamped to 2,000 bytes.
//!
//! ### 🔍 Debugging & Observability
//! - **Local Errors**: none
//! - **Telemetry Targets**: none declared
//! - **Witness Tests**: `test_compress_monologue`, `test_truncate_embedded_tool_logs`

use crate::agent::runner::turn_compactor::safe_truncate_str;
use crate::agent::runner::RunContext;
use crate::error::AppError;

const MAX_MONOLOGUE_BYTES: usize = 8192;
const MAX_SUMMARY_BYTES: usize = 2000;

impl super::AgentRunner {
    /// Truncates embedded markdown code blocks in a string if they exceed 2,000 characters.
    pub(crate) fn truncate_embedded_tool_logs(content: &str) -> String {
        let mut result = String::with_capacity(content.len());
        let mut current_pos = 0;
        while let Some(start_idx) = content[current_pos..].find("```") {
            let abs_start = current_pos + start_idx;
            result.push_str(&content[current_pos..abs_start]);

            let rest = &content[abs_start + 3..];
            if let Some(end_idx) = rest.find("```") {
                let abs_end = abs_start + 3 + end_idx;
                let code_block_content = &content[abs_start + 3..abs_end];

                let (header, mut body) = if let Some(idx) = code_block_content.find('\n') {
                    (&code_block_content[..idx], &code_block_content[idx + 1..])
                } else {
                    ("", code_block_content)
                };

                if body.ends_with('\n') {
                    body = &body[..body.len() - 1];
                    if body.ends_with('\r') {
                        body = &body[..body.len() - 1];
                    }
                }

                if body.len() > 2000 {
                    result.push_str("```");
                    result.push_str(header);
                    result.push_str("\n[Raw tool result evicted to save context — was ");
                    result.push_str(&body.len().to_string());
                    result.push_str(" bytes]\n```");
                } else {
                    result.push_str(&content[abs_start..abs_end + 3]);
                }
                current_pos = abs_end + 3;
            } else {
                result.push_str(&content[abs_start..]);
                current_pos = content.len();
                break;
            }
        }
        if current_pos < content.len() {
            result.push_str(&content[current_pos..]);
        }
        result
    }

    /// Fallback summarizer that runs deterministically without calling LLMs.
    /// Keeps header structures and short lines, removing code blocks entirely.
    pub(crate) fn deterministic_fallback_summarize(history: &str) -> String {
        let mut lines = Vec::new();
        let mut in_code_block = false;
        for line in history.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("```") {
                in_code_block = !in_code_block;
                lines.push("[Code block header omitted]".to_string());
                continue;
            }
            if in_code_block {
                continue;
            }
            if trimmed.len() > 150 {
                lines.push(format!(
                    "{}... [truncated]",
                    safe_truncate_str(trimmed, 150)
                ));
            } else if !trimmed.is_empty() {
                lines.push(trimmed.to_string());
            }
        }
        format!("DETERMINISTIC FALLBACK SUMMARY:\n{}", lines.join("\n"))
    }

    /// Compresses the internal monologue via recursive summarization if it grows too large.
    pub(crate) async fn compress_monologue(
        &self,
        ctx: &RunContext,
        monologue: &mut Vec<String>,
    ) -> Result<(), AppError> {
        let total_bytes: usize = monologue.iter().map(|s| s.len()).sum();
        if total_bytes < MAX_MONOLOGUE_BYTES {
            return Ok(());
        }

        tracing::info!(
            "✂️ [Mythos] Monologue threshold reached ({} bytes) for agent {}. Summarizing...",
            total_bytes,
            ctx.agent_id
        );

        // 1. First pass: truncate embedded tool logs across all entries
        for turn in monologue.iter_mut() {
            *turn = Self::truncate_embedded_tool_logs(turn);
        }

        let post_truncation_bytes: usize = monologue.iter().map(|s| s.len()).sum();
        if post_truncation_bytes < MAX_MONOLOGUE_BYTES {
            tracing::info!(
                "✂️ [Mythos] Monologue brought under budget ({} bytes) via log truncation alone",
                post_truncation_bytes
            );
            return Ok(());
        }

        let tail_count = 4;
        let monologue_len = monologue.len();

        let (older_turns, tail_turns) = if monologue_len > tail_count {
            let split_idx = monologue_len - tail_count;
            (
                monologue[..split_idx].to_vec(),
                monologue[split_idx..].to_vec(),
            )
        } else if monologue_len > 1 {
            (
                monologue[..monologue_len - 1].to_vec(),
                monologue[monologue_len - 1..].to_vec(),
            )
        } else {
            (monologue.clone(), Vec::new())
        };

        if older_turns.is_empty() {
            return Ok(());
        }

        let history = older_turns.join("\n\n");
        let prompt = format!(
            "SUMMARIZE YOUR PREVIOUS REASONING STEPS INTO A SINGLE CONCISE PARAGRAPH. \
             RETAIN ALL KEY INSIGHTS, VARIABLES, AND HYPOTHESES. \
             \n\nPREVIOUS REASONING:\n{}",
            history
        );

        let summary_text = match self
            .call_provider(
                ctx,
                "You are an expert reasoning summarizer. Be technical, dense, and objective.",
                &prompt,
                None,
            )
            .await
        {
            Ok((text, _, _)) => {
                let clamped = safe_truncate_str(&text, MAX_SUMMARY_BYTES);
                format!("CONSOLIDATED REASONING: {}", clamped)
            }
            Err(e) => {
                tracing::warn!(
                    "⚠️ [Mythos] Summarizer call failed ({:?}). Falling back to deterministic summarization.",
                    e
                );
                Self::deterministic_fallback_summarize(&history)
            }
        };

        monologue.clear();
        monologue.push(summary_text);
        monologue.extend(tail_turns);
        ctx.metrics.record_summarization();

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::agent::runner::AgentRunner;

    #[test]
    fn test_truncate_embedded_tool_logs() {
        let long_code = "a".repeat(2500);
        let content = format!("Reasoning before\n```json\n{}\n```\nReasoning after", long_code);
        let truncated = AgentRunner::truncate_embedded_tool_logs(&content);
        assert!(truncated.contains("[Raw tool result evicted to save context — was 2500 bytes]"));
        assert!(truncated.contains("Reasoning before"));
        assert!(truncated.contains("Reasoning after"));
    }

    #[test]
    fn test_deterministic_fallback_summarize() {
        let history = "First thought\n```rust\nfn test() {}\n```\nSecond thought line that is normal";
        let summary = AgentRunner::deterministic_fallback_summarize(history);
        assert!(summary.contains("DETERMINISTIC FALLBACK SUMMARY:"));
        assert!(summary.contains("First thought"));
        assert!(summary.contains("Second thought line that is normal"));
        assert!(summary.contains("[Code block header omitted]"));
    }
}

//! @docs ARCHITECTURE:Runner
//!
//! ### AI Assist Note
//! **Synthesis Budget & Tokenizer**: Fast BPE tokenizer caching, token budget calculation
//! derived from model context window, and word-boundary protected token compression.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Tokenizer initialization failure, budget underflow, or corrupted compressed tokens.
//! - **Telemetry Link**: Search `[synthesis_budget]` in tracing logs.
//! - **Trace Scope**: `server-rs::agent::runner::synthesis::budget`

use once_cell::sync::Lazy;

use super::super::{AgentRunner, RunContext};

/// Global, zero-reinitialization BPE tokenizer cache
pub static BPE_TOKENIZER: Lazy<Option<tiktoken_rs::CoreBPE>> =
    Lazy::new(|| tiktoken_rs::cl100k_base().ok());

/// Fast token counting helper using the shared cached BPE instance
pub fn count_tokens_fast(text: &str) -> usize {
    if let Some(ref bpe) = *BPE_TOKENIZER {
        bpe.encode_with_special_tokens(text).len()
    } else {
        text.len() / 4
    }
}

/// Static regex lookup table for token compression with strict word-boundary protection.
/// Ensures words like "okay" or identifiers like "temperature_sensor" are never corrupted.
pub static SYSTEM_TOKEN_REGEXES: Lazy<Vec<(regex::Regex, &'static str)>> = Lazy::new(|| {
    vec![
        (regex::Regex::new(r"\bSTATUS:\s*ok\b").unwrap(), "*ok*"),
        (regex::Regex::new(r"\bSTATUS:\s*success\b").unwrap(), "*ok*"),
        (regex::Regex::new(r"\bSTATUS:\s*failed\b").unwrap(), "*err*"),
        (regex::Regex::new(r"\bSTATUS:\s*error\b").unwrap(), "*err*"),
        (regex::Regex::new(r"\bRESULT:").unwrap(), "RES:"),
        (regex::Regex::new(r"\bFINDING:").unwrap(), "FND:"),
        (regex::Regex::new(r"\bSOURCE:").unwrap(), "SRC:"),
        (regex::Regex::new(r"\bLocation:").unwrap(), "LOC:"),
        (regex::Regex::new(r"\bPrimary Goal:").unwrap(), "GOAL:"),
        (regex::Regex::new(r"\bWeather for zip\b").unwrap(), "WTR|"),
        (
            regex::Regex::new(r"\bFinding for mission\b").unwrap(),
            "FND|",
        ),
        (regex::Regex::new(r"\bStrategic Intent:").unwrap(), "INT:"),
        (
            regex::Regex::new(r"\bMission Complete\b").unwrap(),
            "*done*",
        ),
        (
            regex::Regex::new(r"\bTask in progress\b").unwrap(),
            "*busy*",
        ),
    ]
});

/// Formats a list of items under a token budget, guaranteeing that the appended
/// `truncated_notice` never causes the total output to exceed `token_budget`.
pub fn format_budgeted_items<T, F>(
    items: Vec<T>,
    token_budget: usize,
    empty_fallback: &str,
    truncated_notice: &str,
    format_item: F,
) -> String
where
    F: Fn(&T) -> String,
{
    if items.is_empty() {
        return empty_fallback.to_string();
    }

    let notice_tokens = count_tokens_fast(truncated_notice);
    let mut lines = Vec::new();
    let mut used_tokens = 0;
    let mut truncated = false;

    for item in &items {
        let line = format_item(item);
        let tokens = count_tokens_fast(&line);
        if used_tokens + tokens + notice_tokens <= token_budget {
            lines.push(line);
            used_tokens += tokens;
        } else if used_tokens + tokens <= token_budget && lines.len() + 1 == items.len() {
            // Exactly last item and fits within total budget without notice
            lines.push(line);
            used_tokens += tokens;
        } else {
            truncated = true;
            break;
        }
    }

    if truncated {
        // Ensure adding the notice does not push total over budget
        while !lines.is_empty() && used_tokens + notice_tokens > token_budget {
            if let Some(removed) = lines.pop() {
                used_tokens = used_tokens.saturating_sub(count_tokens_fast(&removed));
            }
        }
        lines.push(truncated_notice.to_string());
    }

    if lines.is_empty() {
        if truncated {
            truncated_notice.to_string()
        } else {
            empty_fallback.to_string()
        }
    } else {
        lines.join("\n")
    }
}

impl AgentRunner {
    /// 📟 [Token Compression]
    /// Shortens system tokens using strict boundary regexes to prevent identifier corruption.
    pub(crate) fn shorten_system_tokens(&self, text: &str) -> String {
        let mut encoded = text.to_string();
        for (re, replacement) in SYSTEM_TOKEN_REGEXES.iter() {
            if re.is_match(&encoded) {
                encoded = re.replace_all(&encoded, *replacement).to_string();
            }
        }
        encoded
    }

    /// Computes the usable prompt token budget derived from model context window
    pub(crate) fn compute_prompt_token_budget(&self, ctx: &RunContext) -> usize {
        if let Some(cw) = ctx.model_config.context_window {
            let max_resp = ctx.model_config.max_tokens.unwrap_or(4096) as usize;
            (cw as usize).saturating_sub(max_resp).max(20)
        } else if let Some(tpm) = ctx.model_config.tpm {
            tpm as usize
        } else {
            let cw = 32_768;
            let max_resp = ctx.model_config.max_tokens.unwrap_or(4096) as usize;
            cw - max_resp
        }
    }
}

// Metadata: [synthesis_budget]


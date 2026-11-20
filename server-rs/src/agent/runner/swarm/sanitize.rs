//! @docs ARCHITECTURE:Runner
//!
//! ### AI Assist Note
//! **Swarm Sanitizer & Prompt Framing**: Validates sub-agent slugs, identifies privileged mutating skills, and escapes XML/control tokens in untrusted sub-agent reports.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Malformed identifier slug or untrusted control-token injection.
//! - **Telemetry Link**: Search `[sanitize]` in tracing logs.

use super::SubAgentOutcome;
use crate::agent::runner::tools::error::ToolExecutionError;
use once_cell::sync::Lazy;
use regex::Regex;

static CONTROL_TOKEN_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?is)<\|[A-Za-z0-9_]+\|>|</?\s*subagent_report[^>]*>")
        .expect("Static subagent control-token regex MUST be valid")
});


/// Validates a slug identifier (`agent_id`, `task.id`, skill/workflow name).
/// Rejects empty values, overlong strings (> 64 chars), and non-slug characters
/// so substring matching and XML attribute framing cannot be bypassed.
pub(super) fn validate_slug<'a>(raw: &'a str, field_name: &str) -> Result<&'a str, ToolExecutionError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.len() > 64 {
        return Err(ToolExecutionError::ExecutionFailed(format!(
            "PROTOCOL_VIOLATION: '{}' must be between 1 and 64 characters",
            field_name
        )));
    }
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(ToolExecutionError::ExecutionFailed(format!(
            "PROTOCOL_VIOLATION: '{}' contains illegal characters (allowed: ASCII alphanumeric, '-', '_')",
            field_name
        )));
    }
    Ok(trimmed)
}

pub(super) fn is_privileged_mutating_skill(name: &str) -> bool {
    matches!(
        name.trim().to_ascii_lowercase().as_str(),
        "write_file"
            | "write_to_file"
            | "patch_file"
            | "edit_file"
            | "create_file"
            | "delete_file"
            | "execute_shell"
            | "run_command"
            | "exec_command"
            | "terminal"
            | "shell"
            | "synthesize_micro_script"
            | "refactor_synthesized_skill"
            | "query_financial_logs"
    )
}

pub(super) fn escape_xml_text(raw: &str) -> String {
    raw.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

pub(super) fn escape_xml_attr(raw: &str) -> String {
    escape_xml_text(raw)
        .replace('"', "&quot;")
        .replace(['\n', '\r'], " ")
}

/// Security boundary & prompt framing sanitizer for untrusted sub-agent outputs.
/// Threat model: prevents prompt/XML breakout via injected `</subagent_report>` tags,
/// opening `<subagent_report ...>` spoofing, `&lt;` confusion, or chat template tokens (`<|...|>`).
pub(super) fn sanitize_subagent_text(raw: &str) -> String {
    let stripped = CONTROL_TOKEN_RE.replace_all(raw, "");
    escape_xml_text(&stripped)
}

pub(super) fn truncate_utf8(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    &s[..s.floor_char_boundary(max_bytes)]
}

pub(super) fn should_skip_synthesis(outcomes: &[SubAgentOutcome]) -> bool {
    !outcomes
        .iter()
        .any(|o| matches!(o, SubAgentOutcome::Success { .. }))
}


// Metadata: [sanitize]

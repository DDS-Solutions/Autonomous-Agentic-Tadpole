//! @docs ARCHITECTURE:Runner
//!
//! ### AI Assist Note
//! **Swarm Outcome Synthesis & Budgeting**: Formats and character-budgets pooled sub-agent XML reports while keeping wrapper tags balanced.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Oversized sub-agent output requiring UTF-8-safe truncation.
//! - **Telemetry Link**: Search `[synthesis]` in tracing logs.

use super::sanitize::{escape_xml_attr, sanitize_subagent_text, truncate_utf8};
use super::{SubAgentOutcome, MAX_SYNTHESIS_PAYLOAD_CHARS};

pub(super) fn outcome_parts(outcome: &SubAgentOutcome) -> (&str, &str, &str, String) {
    match outcome {
        SubAgentOutcome::Success {
            task_id,
            agent_id,
            output,
        } => (task_id, agent_id, "success", sanitize_subagent_text(output)),
        SubAgentOutcome::ProtocolBlocked {
            task_id,
            agent_id,
            reason,
        } => (
            task_id,
            agent_id,
            "protocol_blocked",
            format!("PROTOCOL_VIOLATION: {}", sanitize_subagent_text(reason)),
        ),
        SubAgentOutcome::Failed {
            task_id,
            agent_id,
            error,
        } => (
            task_id,
            agent_id,
            "failed",
            format!("SUB-AGENT EXECUTION ERROR: {}", sanitize_subagent_text(error)),
        ),
        SubAgentOutcome::Skipped {
            task_id,
            agent_id,
            reason,
        } => (
            task_id,
            agent_id,
            "skipped",
            format!("SKIPPED: {}", sanitize_subagent_text(reason)),
        ),
    }
}

/// Formats outcomes with strict total character budgeting so wrapper `<subagent_report>`
/// tags always stay balanced and valid even when many workers return large payloads.
pub(super) fn format_pooled_outcomes(outcomes: &[SubAgentOutcome]) -> String {
    const TRUNC_MARKER: &str = "\n... [Result truncated for synthesis context]";
    const SEPARATOR: &str = "\n\n---\n\n";
    let count = outcomes.len().max(1);
    let separator_overhead = SEPARATOR.len() * outcomes.len().saturating_sub(1);
    let remaining_total = MAX_SYNTHESIS_PAYLOAD_CHARS.saturating_sub(separator_overhead);
    let per_slot_budget = (remaining_total / count).max(160);

    let mut blocks = Vec::with_capacity(outcomes.len());
    let mut used = 0usize;

    for outcome in outcomes {
        let (task_id, agent_id, status, body) = outcome_parts(outcome);
        let open = format!(
            "<subagent_report task_id=\"{}\" agent_id=\"{}\" status=\"{}\">\n",
            escape_xml_attr(task_id),
            escape_xml_attr(agent_id),
            escape_xml_attr(status)
        );
        let close = "\n</subagent_report>";
        let frame_len = open.len() + close.len();
        let left_total = MAX_SYNTHESIS_PAYLOAD_CHARS.saturating_sub(used + separator_overhead);
        let max_frame_and_body = per_slot_budget.min(left_total.max(frame_len));
        let body_budget = max_frame_and_body.saturating_sub(frame_len);

        let bounded_body = if body.len() <= body_budget {
            body
        } else if body_budget <= TRUNC_MARKER.len() {
            truncate_utf8(&body, body_budget).to_string()
        } else {
            let cut = truncate_utf8(&body, body_budget - TRUNC_MARKER.len());
            format!("{}{}", cut, TRUNC_MARKER)
        };
        let block = format!("{}{}{}", open, bounded_body, close);
        used = used.saturating_add(block.len());
        blocks.push(block);
    }

    blocks.join(SEPARATOR)
}


// Metadata: [synthesis]

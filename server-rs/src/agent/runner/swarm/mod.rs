//! @docs ARCHITECTURE:Runner
//!
//! ### AI Assist Note
//! **Swarm Coordinator Module Root**: Coordinates recursive sub-agent delegation, DAG planning, XML prompt sanitization, tiered recruitment, and pooled outcome synthesis.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Circular recursion (SEC-01), max swarm depth exceeded, or sub-agent recruitment failure.
//! - **Telemetry Link**: Search `[swarm]` in tracing logs.

mod executor;
mod planner;
mod recruitment;
mod sanitize;
mod synthesis;
#[cfg(test)]
mod tests;

use super::RunContext;
use crate::agent::types::ModelConfig;

pub(super) const MAX_SYNTHESIS_PAYLOAD_CHARS: usize = 12_000;
pub(super) const MAX_UPSTREAM_SNIPPET_CHARS: usize = 4_000;
pub(super) const MAX_UPSTREAM_CONTEXT_CHARS: usize = 12_000;
pub(super) const DEFAULT_SUBAGENT_BUDGET_CAP_USD: f64 = 10.0;
pub(super) const DEFAULT_ALPHA_TIMEOUT_SECS: u64 = 240;

pub(crate) struct SubAgentOptions<'a> {
    pub agent_id: &'a str,
    pub parent_config: &'a ModelConfig,
    pub extra_skills: Option<&'a [serde_json::Value]>,
    pub extra_workflows: Option<&'a [serde_json::Value]>,
    pub role_override: Option<&'a str>,
    pub caller_ctx: Option<&'a RunContext>,
}

/// Typed outcome returned by a sub-agent DAG node, eliminating string-matching heuristics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SubAgentOutcome {
    Success {
        task_id: String,
        agent_id: String,
        output: String,
    },
    ProtocolBlocked {
        task_id: String,
        agent_id: String,
        reason: String,
    },
    Failed {
        task_id: String,
        agent_id: String,
        error: String,
    },
    Skipped {
        task_id: String,
        agent_id: String,
        reason: String,
    },
}

#[derive(Debug, Clone)]
pub(super) struct SubTaskSpec {
    pub(super) id: String,
    pub(super) agent_id: String,
    pub(super) message: String,
    pub(super) depends_on: Vec<String>,
}


// Metadata: [swarm]

//! @docs ARCHITECTURE:Runner
//!
//! ### AI Assist Note
//! **Synthesis Repo Map Engine**: Generates architectural graph summaries, hot paths,
//! and topological code maps dynamically tailored to task complexity and role domain.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: CodeGraph lock contention, empty summary fallback, or token budget pruning.
//! - **Telemetry Link**: Search `[synthesis_repo_map]` in tracing logs.
//! - **Trace Scope**: `server-rs::agent::runner::synthesis::repo_map`

use super::super::{AgentRunner, RunContext};

impl AgentRunner {
    /// Generates the Repo Map summary, applying pruning logic if needed.
    pub(crate) async fn generate_repo_map(
        &self,
        ctx: &RunContext,
        payload_message: &str,
    ) -> (crate::utils::graph::GraphSummary, String) {
        let graph: std::sync::Arc<parking_lot::RwLock<crate::utils::graph::CodeGraph>> =
            self.state.resources.get_code_graph().await;

        let summary = graph.read().generate_summary();

        // Only include repo map for non-trivial tasks or technical roles to save tokens
        if payload_message.len() > 10 || ctx.department.contains("Technical") {
            (summary.clone(), summary.text)
        } else {
            (
                crate::utils::graph::GraphSummary {
                    text: "Architecture Map: Omitted for high-level greeting.".to_string(),
                    hot_paths: vec![],
                },
                "Architecture Map: Omitted for high-level greeting.".to_string(),
            )
        }
    }
}

// Metadata: [synthesis_repo_map]


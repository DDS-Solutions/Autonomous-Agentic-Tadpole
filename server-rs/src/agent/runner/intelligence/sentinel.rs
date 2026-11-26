//! @docs ARCHITECTURE:Runner
//!
//! ### AI Assist Note
//! **Sentinel Gate, Halting Detection & Recovery Synthesis**: Enforces specialist
//! tool-use autonomy, orchestrator stall detection, Mythos halting-signal extraction
//! (`<halting_signal/>`, `<halt/>`, `set_confidence`), malformed tool-call self-annealing,
//! and silent-completion narrative synthesis.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Specialist stuck in narrative mode after 2 sentinel retries, or malformed JSON after 2 anneal attempts.
//! - **Telemetry Link**: Search `[Sentinel]` or `[Self-Annealing]` in tracing logs.

use super::super::{AgentRunner, RunContext};
use crate::agent::constants::*;
use crate::agent::types::{TokenUsage, ToolCall, ToolDefinition};
use crate::error::AppError;

/// Detects tag-based or `set_confidence`-based halting signals and removes `set_confidence`
/// meta-calls from `function_calls`.
pub(super) fn detect_halting_signal(
    ctx: &RunContext,
    turn_text: &str,
    function_calls: &mut Vec<ToolCall>,
) -> bool {
    let mut halted = false;

    if turn_text.contains("<halting_signal/>") || turn_text.contains("<halt/>") {
        tracing::info!(
            "🛑 [Mythos] Halting signal detected for agent {}",
            ctx.agent_id
        );
        halted = true;
    }

    for fc in function_calls.iter() {
        if fc.name == "set_confidence" {
            let score = fc.args.get("score").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
            if score >= ctx.act_threshold {
                tracing::info!(
                    "🛑 [Mythos] Confidence-based halt for agent {}: {:.2} >= {:.2}",
                    ctx.agent_id,
                    score,
                    ctx.act_threshold
                );
                halted = true;
            }
        }
    }
    function_calls.retain(|fc| fc.name != "set_confidence");

    halted
}

impl AgentRunner {
    /// Enforces the Sentinel Gate protocol: Specialist agents are forbidden from text-only turns,
    /// and orchestrators are prevented from stalling with user-input requests.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn enforce_sentinel_gate(
        &self,
        ctx: &RunContext,
        system_prompt: &str,
        user_directive: &str,
        output_text: &mut String,
        function_calls: &mut Vec<ToolCall>,
        usage: &mut Option<TokenUsage>,
        attempt_count: &mut u32,
        tools: &[ToolDefinition],
    ) -> Result<(), AppError> {
        if *attempt_count >= 2 {
            tracing::error!(
                "🚨 [Sentinel] Patience limit reached for {}. Agent is stuck in narrative mode.",
                ctx.agent_id
            );
            return Err(AppError::InternalServerError(
                "Sentinel Gate: Model failed to transition to tool-use after 2 attempts."
                    .to_string(),
            ));
        }

        let is_orchestrator = ctx.agent_id == AGENT_CEO
            || ctx.agent_id == AGENT_COO
            || ctx.agent_id == AGENT_ALPHA
            || ctx.role.to_lowercase().contains("ceo")
            || ctx.role.to_lowercase().contains("coo")
            || ctx.name.to_lowercase().contains("alpha");

        if !is_orchestrator
            && !ctx.safe_mode
            && function_calls.is_empty()
            && !output_text.contains("complete_mission")
        {
            tracing::warn!(
                "🛡️ [Sentinel] Specialist {} attempted narrative leak. Enforcing tactical autonomy...",
                ctx.agent_id
            );

            let sentinel_directive = format!(
                "SYSTEM_SENTINEL: Your turn resulted in a narrative-only response. As an AGENT (Task Specialist), you are FORBIDDEN from text-only progress reports or roadblock apologies. \
                 You MUST execute tools or call 'complete_mission' with results. Directive: {}",
                user_directive
            );

            let sentinel_result = self
                .call_provider(
                    ctx,
                    system_prompt,
                    &sentinel_directive,
                    Some(tools.to_vec()),
                )
                .await;

            *attempt_count += 1;
            let (sent_text, sent_calls, sent_usage) = sentinel_result?;
            *output_text = sent_text;
            *function_calls = sent_calls;
            self.accumulate_usage(usage, sent_usage);
        }

        if is_orchestrator
            && !ctx.safe_mode
            && function_calls.is_empty()
            && !output_text.contains("complete_mission")
        {
            let is_stalled =
                crate::agent::cascade_router::CascadeRouter::detect_stall_pattern(output_text)
                    .is_some();

            if is_stalled {
                tracing::warn!(
                    "🛡️ [Sentinel] Orchestrator {} stalled (asking for input instead of acting). Re-prompting with mission directive...",
                    ctx.agent_id
                );

                let sentinel_directive = format!(
                    "SYSTEM_SENTINEL: You are an AUTONOMOUS ORCHESTRATOR. Your response was a STALL — you asked the user for instructions instead of acting. \
                     This is a PROTOCOL VIOLATION. You have the mission objective: '{}'. \
                     IMMEDIATELY begin work: recruit agents from the CLUSTER DIRECTORY, delegate sub-tasks, or call tools. \
                     DO NOT ask for instructions. DO NOT wait. ACT NOW.",
                    user_directive
                );

                let sentinel_result = self
                    .call_provider(
                        ctx,
                        system_prompt,
                        &sentinel_directive,
                        Some(tools.to_vec()),
                    )
                    .await;

                *attempt_count += 1;
                let (sent_text, sent_calls, sent_usage) = sentinel_result?;
                *output_text = sent_text;
                *function_calls = sent_calls;
                self.accumulate_usage(usage, sent_usage);
            }
        }

        Ok(())
    }

    /// Handles re-prompting when the model produces malformed tool calls.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn handle_extraction_failure(
        &self,
        ctx: &RunContext,
        system_prompt: &str,
        user_directive: &str,
        error_detail: &str,
        output_text: &mut String,
        function_calls: &mut Vec<ToolCall>,
        usage: &mut Option<TokenUsage>,
        attempt_count: &mut u32,
        tools: &[ToolDefinition],
    ) -> Result<(), AppError> {
        if *attempt_count >= 2 {
            tracing::error!(
                "🚨 [Self-Annealing] Max attempts reached for {}. Giving up on malformed JSON.",
                ctx.agent_id
            );
            return Err(AppError::BadRequest(format!(
                "Self-Annealing failed after 2 attempts: {}",
                error_detail
            )));
        }

        tracing::warn!(
            "🛠️ [Self-Annealing] Model {} produced malformed tool call: {}. Re-prompting...",
            ctx.agent_id,
            error_detail
        );

        let correction_prompt = format!(
            "SYSTEM_ANNEAL: Your previous tool call was MALFORMED. \
             Error: {}\n\n\
             You MUST fix the JSON syntax. Ensure all arguments are properly quoted and brackets are balanced. \
             Directive: {}",
            error_detail, user_directive
        );

        let result = self
            .call_provider(ctx, system_prompt, &correction_prompt, Some(tools.to_vec()))
            .await?;

        *attempt_count += 1;
        let (sent_text, sent_calls, sent_usage) = result;
        *output_text = sent_text;
        *function_calls = sent_calls;
        self.accumulate_usage(usage, sent_usage);

        Ok(())
    }

    /// Synthesizes a narrative fallback summary with tool calls disabled (`None`) when a turn
    /// completes silently without human-readable output.
    pub(super) async fn synthesize_silent_completion(
        &self,
        ctx: &RunContext,
        system_prompt: &str,
        prompt: &str,
        usage: &mut Option<TokenUsage>,
    ) -> Option<String> {
        if let Ok((summary, _, s_usage)) = self
            .call_provider(ctx, system_prompt, prompt, None)
            .await
        {
            self.accumulate_usage(usage, s_usage);
            Some(summary)
        } else {
            None
        }
    }
}

// Metadata: [sentinel]

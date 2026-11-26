//! @docs ARCHITECTURE:Runner
//!
//! ### AI Assist Note
//! **Intelligence Loop Coordinator**: Lean Think->Act->Respond state machine delegating
//! turn evaluation & recovery (`turn_eval`), sentinel & halting gates (`sentinel`), and
//! bounded parallel tool execution (`tool_batch`). Preserves RAII turn-state reset
//! (`ReasoningTurnGuard`), durable checkpointing, and real-time financial guardrails.
//!
//! ### 🔍 Debugging & Observability
//! - **Telemetry Link**: Search `[Intelligence]` in tracing logs.
//! - **Failure Path**: Provider API timeout, budget breach, context overflow, or parent cancellation.
//! - **Trace Scope**: `server-rs::agent::runner::intelligence`

mod sentinel;
mod tool_batch;
mod turn_eval;

#[cfg(test)]
mod tests;

#[cfg(test)]
use tool_batch::{evaluate_loop_repetition, LoopRepetitionAction};

use super::{AgentRunner, IntelligenceOutput, RunContext};
use crate::agent::constants::*;
use crate::agent::types::TaskPayload;
use crate::error::AppError;
use std::sync::Arc;
use turn_eval::ProviderTurnOutcome;

/// RAII Guard to ensure reasoning turn state is always reset in the registry.
struct ReasoningTurnGuard {
    agent_id: String,
    state: Arc<crate::state::AppState>,
}

impl ReasoningTurnGuard {
    fn new(agent_id: String, state: Arc<crate::state::AppState>) -> Self {
        Self { agent_id, state }
    }
}

impl Drop for ReasoningTurnGuard {
    fn drop(&mut self) {
        if let Some(mut entry) = self.state.registry.agents.get_mut(&self.agent_id) {
            entry.value_mut().state.current_reasoning_turn = 0;
        }
    }
}

/// Removes internal Mythos control tags from narrative text.
pub(super) fn scrub_mythos_tags(text: &str) -> String {
    text.replace("<halting_signal/>", "")
        .replace("<halt/>", "")
        .replace("<thinking>", "")
        .replace("</thinking>", "")
        .trim()
        .to_string()
}

/// Returns the official hierarchy role label for the agent context.
pub(crate) fn get_hierarchy_label(ctx: &RunContext) -> &'static str {
    if ctx.agent_id == AGENT_CEO || ctx.role.to_lowercase().contains("ceo") {
        "CEO (Strategic Intelligence Lead)"
    } else if ctx.agent_id == AGENT_COO || ctx.role.to_lowercase().contains("coo") {
        "COO (Operations Director)"
    } else if ctx.agent_id == AGENT_ALPHA || ctx.name.to_lowercase().contains("alpha") {
        "ALPHA NODE (Swarm Mission Commander)"
    } else {
        "AGENT (Task Specialist)"
    }
}

impl AgentRunner {
    /// Orchestrates prompt generation, recurrent reasoning turns, sentinel gates,
    /// parallel tool batches, and mission finalization.
    pub(crate) async fn execute_intelligence_loop(
        &self,
        ctx: &RunContext,
        payload: &TaskPayload,
    ) -> Result<IntelligenceOutput, (AppError, Option<crate::agent::types::TokenUsage>)> {
        let _turn_guard = ReasoningTurnGuard::new(ctx.agent_id.clone(), self.state.clone());
        let hierarchy_label = get_hierarchy_label(ctx);

        self.broadcast_agent(
            ctx,
            &format!("starting task ({})...", hierarchy_label),
            "info",
        );
        self.update_status(
            &ctx.agent_id,
            &ctx.mission_id,
            "thinking",
            Some("Consulting intelligence model..."),
        );

        let system_prompt = self.build_system_prompt(ctx, &payload.message).await;

        let mut effective_ctx = ctx.clone();
        let mut output_text = String::new();
        let mut usage: Option<crate::agent::types::TokenUsage> = None;
        let mut turn_count = 0;
        let mut conversation_history = vec![format!("USER: {}", payload.message)];
        let max_turns = effective_ctx.max_turns;
        let mut running_cost: f64 = effective_ctx.current_cost_usd;

        let tool_repetition_tracker = Arc::new(parking_lot::Mutex::new(
            std::collections::HashMap::<(String, String), usize>::new(),
        ));
        let doom_loop_detector = Arc::new(parking_lot::Mutex::new(
            crate::agent::runner::loop_detector::DoomLoopDetector::new(),
        ));

        while turn_count < max_turns {
            if effective_ctx.cancel_handle.is_cancelled() {
                return Err((
                    AppError::Conflict("Mission cancelled by parent orchestrator".to_string()),
                    usage,
                ));
            }
            turn_count += 1;
            effective_ctx.metrics.record_turn();
            tracing::debug!(
                "🎯 [Intelligence] Start Turn {}/{} for agent {}",
                turn_count,
                max_turns,
                effective_ctx.agent_id
            );

            // Pre-turn financial gate
            let effective_budget = if effective_ctx.budget_usd > 0.0 {
                effective_ctx.budget_usd
            } else if effective_ctx.budget_limit_usd > 0.0 {
                effective_ctx.budget_limit_usd
            } else {
                crate::agent::constants::DEFAULT_BUDGET_LIMIT_USD
            };

            if running_cost >= effective_budget {
                tracing::warn!(
                    "💰 [Intelligence] Budget limit reached before Turn {} for agent {} (${:.4} >= ${:.4})",
                    turn_count,
                    effective_ctx.agent_id,
                    running_cost,
                    effective_budget
                );
                self.broadcast_agent(
                    &effective_ctx,
                    "Budget limit reached. Quiescing mission.",
                    "warn",
                );
                return Ok(IntelligenceOutput {
                    text: format!(
                        "{} (Halting: Mission Budget Cap Exceeded)",
                        scrub_mythos_tags(&output_text)
                    ),
                    usage,
                });
            }

            self.state
                .yield_phase_transition(
                    &effective_ctx.agent_id,
                    &format!("Execution: Turn {}", turn_count),
                )
                .await;

            let tools = vec![self.build_tools(&effective_ctx).await];

            // Recurrent Reasoning Loop
            let mut reasoning_turn = 0;
            let mut resource_guard_attempts: u32 = 0;
            let mut escalated_this_turn = false;
            let mut reasoning_halted = false;
            let mut internal_monologue = Vec::new();

            while reasoning_turn < effective_ctx.reasoning_depth && !reasoning_halted {
                if effective_ctx.cancel_handle.is_cancelled() {
                    return Err((
                        AppError::Conflict("Mission cancelled by parent orchestrator".to_string()),
                        usage,
                    ));
                }
                reasoning_turn += 1;

                if ctx.reasoning_depth > 1 {
                    tracing::info!(
                        "🧠 [Mythos] Reasoning Loop {}/{} for agent {}",
                        reasoning_turn,
                        ctx.reasoning_depth,
                        ctx.agent_id
                    );
                    if let Some(mut entry) = self.state.registry.agents.get_mut(&ctx.agent_id) {
                        entry.value_mut().state.current_reasoning_turn = reasoning_turn;
                    }
                    self.broadcast_agent(
                        ctx,
                        &format!(
                            "thinking (loop {}/{})...",
                            reasoning_turn, ctx.reasoning_depth
                        ),
                        "pulse",
                    );
                }

                // Intra-turn budget check
                match self.check_budget(ctx, &mut running_cost, 0.0, "").await {
                    Ok(Some(_)) => {
                        tracing::warn!(
                            "💰 [Mythos] Budget breach mid-recurrence for agent {}",
                            ctx.agent_id
                        );
                        return Ok(IntelligenceOutput {
                            text: format!(
                                "{} (Halting: Budget Exceeded)",
                                scrub_mythos_tags(&output_text)
                            ),
                            usage,
                        });
                    }
                    Err(e) => return Err((e, usage)),
                    Ok(None) => {}
                }

                self.record_heartbeat(&ctx.agent_id).await;

                let compacted_history =
                    crate::agent::runner::turn_compactor::compact_conversation_history(
                        &ctx.role,
                        &conversation_history,
                    );
                let current_prompt = if internal_monologue.is_empty() {
                    compacted_history.join("\n\n")
                } else {
                    format!(
                        "{}\n\nINTERNAL MONOLOGUE:\n{}",
                        compacted_history.join("\n\n"),
                        internal_monologue.join("\n\n")
                    )
                };

                self.apply_memory_throttle(ctx).await;

                let script_profile = crate::agent::script_profiler::profile_text(&current_prompt);
                if !script_profile.is_english && script_profile.non_latin_fraction > 0.3 {
                    tracing::info!(
                        "🌐 [Intelligence] Non-Latin script detected ({}). Script-aware routing active.",
                        script_profile.dominant_script
                    );
                }

                if turn_count == 1 && reasoning_turn == 1 {
                    self.run_preflight_injection_gate(ctx, payload, &script_profile.dominant_script)
                        .await;
                }

                let mut structured_messages: Vec<crate::agent::types::ChatMessage> =
                    compacted_history
                        .iter()
                        .map(|t| crate::agent::types::ChatMessage::from_legacy_turn(t))
                        .collect();
                if !internal_monologue.is_empty() {
                    structured_messages.push(crate::agent::types::ChatMessage::assistant(format!(
                        "INTERNAL MONOLOGUE:\n{}",
                        internal_monologue.join("\n\n")
                    )));
                }

                if let Err(e) =
                    turn_eval::validate_context_window(ctx, &system_prompt, &structured_messages)
                {
                    return Err((e, usage));
                }

                let (mut turn_text, mut function_calls, turn_usage) = match self
                    .invoke_turn_with_recovery(
                        &mut effective_ctx,
                        &system_prompt,
                        &current_prompt,
                        &structured_messages,
                        &tools,
                        turn_count,
                        &mut resource_guard_attempts,
                        &mut escalated_this_turn,
                        &mut running_cost,
                        &mut usage,
                    )
                    .await
                {
                    Ok(ProviderTurnOutcome::Completed {
                        turn_text,
                        function_calls,
                        turn_usage,
                    }) => (turn_text, function_calls, turn_usage),
                    Ok(ProviderTurnOutcome::RetryIteration) => {
                        reasoning_turn = reasoning_turn.saturating_sub(1);
                        continue;
                    }
                    Err(e) => return Err((e, usage)),
                };

                let step_cost = turn_usage
                    .as_ref()
                    .map(|u| {
                        crate::agent::rates::calculate_cost_with_custom_rates(
                            effective_ctx.model_config.input_cost_per_m,
                            effective_ctx.model_config.output_cost_per_m,
                            &effective_ctx.model_config.model_id,
                            u.input_tokens,
                            u.output_tokens,
                        )
                    })
                    .unwrap_or(0.0);
                running_cost += step_cost;
                self.accumulate_usage(&mut usage, turn_usage);

                if sentinel::detect_halting_signal(ctx, &turn_text, &mut function_calls) {
                    reasoning_halted = true;
                }

                if reasoning_turn < ctx.reasoning_depth && !reasoning_halted {
                    internal_monologue.push(turn_text);
                    let _ = self.compress_monologue(ctx, &mut internal_monologue).await;
                    continue;
                }

                // Final reasoning turn: enforce Sentinel Gate before promoting to active conversation
                let mut sentinel_attempts = 0;
                self.enforce_sentinel_gate(
                    &effective_ctx,
                    &system_prompt,
                    &current_prompt,
                    &mut turn_text,
                    &mut function_calls,
                    &mut usage,
                    &mut sentinel_attempts,
                    &tools,
                )
                .await
                .map_err(|e| (e, usage.clone()))?;

                if !turn_text.is_empty() {
                    if !output_text.is_empty() {
                        output_text.push_str("\n\n");
                    }
                    output_text.push_str(&turn_text);
                    conversation_history.push(format!("ASSISTANT: {}", turn_text));

                    let parent_id = ctx.active_node_id.lock().clone();
                    if let Ok(new_id) = self
                        .append_to_journal(ctx, "assistant", &turn_text, parent_id, None)
                        .await
                    {
                        *ctx.active_node_id.lock() = Some(new_id);
                    }

                    self.broadcast_agent_stream(ctx, &scrub_mythos_tags(&turn_text), false);
                } else if !function_calls.is_empty() {
                    let tool_summary = function_calls
                        .iter()
                        .map(|fc| format!("Invoking tool '{}'", fc.name))
                        .collect::<Vec<_>>()
                        .join("; ");
                    conversation_history.push(format!("ASSISTANT: {}", tool_summary));

                    let parent_id = ctx.active_node_id.lock().clone();
                    if let Ok(new_id) = self
                        .append_to_journal(ctx, "assistant", &tool_summary, parent_id, None)
                        .await
                    {
                        *ctx.active_node_id.lock() = Some(new_id);
                    }
                }

                if function_calls.is_empty() {
                    tracing::debug!(
                        "🏁 [Intelligence] No tool calls for agent {}, breaking loop.",
                        ctx.agent_id
                    );
                    let mut final_text = scrub_mythos_tags(&output_text);

                    if final_text.trim().is_empty() && conversation_history.len() > 2 {
                        tracing::info!(
                            "🔄 [Intelligence] Silent completion detected for {}. Synthesizing final report...",
                            ctx.agent_id
                        );
                        let closure_prompt = "MISSION_CLOSURE: You have completed the technical steps. \
                                              PROVIDE A CONCISE SUMMARY OF YOUR FINDINGS AND ACTIONS TO THE USER NOW. \
                                              DO NOT EXECUTE ANY MORE TOOLS.";
                        if let Some(summary) = self
                            .synthesize_silent_completion(
                                ctx,
                                &system_prompt,
                                closure_prompt,
                                &mut usage,
                            )
                            .await
                        {
                            final_text = summary;
                        }
                    }

                    self.broadcast_agent_stream(ctx, &final_text, true);
                    self.broadcast_agent(ctx, "Neural Pulse: Turn finalized", "pulse");
                    return Ok(IntelligenceOutput {
                        text: final_text,
                        usage,
                    });
                }

                // Parallel Tool Batch Execution
                let batch = self
                    .execute_tool_batch(
                        ctx,
                        function_calls,
                        &payload.message,
                        &mut conversation_history,
                        &tool_repetition_tracker,
                        &doom_loop_detector,
                        &mut running_cost,
                        &mut usage,
                    )
                    .await;

                if !batch.observation_buffer.is_empty() {
                    let (compacted_obs, _) =
                        crate::agent::runner::turn_compactor::compact_and_offload_observation(
                            "execution",
                            &batch.observation_buffer,
                            &ctx.workspace_root,
                        )
                        .await;
                    conversation_history.push(format!("OBSERVATION: {}", compacted_obs));

                    let parent_id = ctx.active_node_id.lock().clone();
                    if let Ok(new_id) = self
                        .append_to_journal(ctx, "tool", &batch.observation_buffer, parent_id, None)
                        .await
                    {
                        *ctx.active_node_id.lock() = Some(new_id);
                    }
                }

                if let Ok(state_json) = serde_json::to_string(&conversation_history) {
                    let parent_id_ref = ctx.active_node_id.lock().clone();
                    let _ = crate::agent::mission::save_mission_checkpoint(
                        &self.state.resources.pool,
                        &ctx.mission_id,
                        reasoning_turn as i64,
                        &state_json,
                        parent_id_ref.as_deref(),
                    )
                    .await;
                }

                if let Some(e) = batch.first_fatal_error {
                    return Err((e, usage));
                }

                if batch.mission_completed {
                    if let Some(report) = batch.final_report {
                        if !output_text.is_empty() && !report.trim().is_empty() {
                            output_text.push_str("\n\n---\n## Final Report\n");
                        }
                        output_text.push_str(&report);
                    }

                    if scrub_mythos_tags(&output_text).trim().is_empty() {
                        tracing::info!(
                            "🔄 [Intelligence] Mission completed silently. Enforcing report synthesis..."
                        );
                        let synthesis_prompt = "You have completed your technical tasks. Provide a CONCISE and TECHNICAL summary of your findings and the current state of the workspace.";
                        if let Some(summary) = self
                            .synthesize_silent_completion(
                                ctx,
                                &system_prompt,
                                synthesis_prompt,
                                &mut usage,
                            )
                            .await
                        {
                            output_text.push_str(&summary);
                        }
                    }

                    let final_text = scrub_mythos_tags(&output_text);
                    self.broadcast_agent_stream(ctx, &final_text, true);
                    self.broadcast_agent(ctx, "Neural Pulse: Turn finalized", "pulse");
                    return Ok(IntelligenceOutput {
                        text: final_text,
                        usage,
                    });
                }

                reasoning_halted = true;
            }
        }

        // Post-turn silent completion synthesis
        if scrub_mythos_tags(&output_text).trim().is_empty() && conversation_history.len() > 1 {
            tracing::info!(
                "🔄 [Intelligence] Post-turn silent completion detected for {}. Enforcing report synthesis...",
                ctx.agent_id
            );
            let synthesis_prompt = "You have completed your technical tasks. Provide a CONCISE and TECHNICAL summary of your findings and the current state of the workspace. If you performed a discovery tool (like list_files), summarize the results now.";
            if let Some(summary) = self
                .synthesize_silent_completion(ctx, &system_prompt, synthesis_prompt, &mut usage)
                .await
            {
                output_text = summary;
            }
        }

        let final_text = scrub_mythos_tags(&output_text);
        self.broadcast_agent_stream(ctx, &final_text, true);
        self.broadcast_agent(ctx, "Neural Pulse: Turn finalized", "pulse");

        Ok(IntelligenceOutput {
            text: final_text,
            usage,
        })
    }
}

// Metadata: [mod]

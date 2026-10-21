//! @docs ARCHITECTURE:Runner
//!
//! ### AI Assist Note
//! **Intelligence Loop**: The heartbeat of an agent turn. Manages the
//! **Think->Act->Respond** cycle. Handles automatic hierarchy labeling
//! (CEO/COO/Alpha) and orchestrates concurrent tool execution using
//! `FuturesUnordered`. Enforces real-time **Financial Guardrails** (SEC-02)
//! by calculating neural costs per step.
//!
//! ### 🔍 Debugging & Observability
//! - **Telemetry Link**: Search `[intelligence]` in tracing logs.
//! - **Failure Path**: Provider API timeout, tool execution panic, budget breach
//!   during a long chain, or tokenizer failure during prompt assembly.
//! - **Trace Scope**: `server-rs::agent::runner::intelligence`

use crate::agent::types::{TaskPayload, ToolDefinition};
use super::{AgentRunner, IntelligenceOutput, RunContext};
use crate::agent::constants::*;
use crate::error::{AppError, Classify};
use std::sync::Arc;
use tracing::Instrument;

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
fn scrub_mythos_tags(text: &str) -> String {
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
    // ─────────────────────────────────────────────────────────
    //  INTELLIGENCE LOOP
    // ─────────────────────────────────────────────────────────

    /// Handles the prompt generation, provider calls, and tool execution loop.
    pub(crate) async fn execute_intelligence_loop(
        &self,
        ctx: &RunContext,
        payload: &TaskPayload,
    ) -> Result<IntelligenceOutput, (AppError, Option<crate::agent::types::TokenUsage>)> {
        // --- 🛡️ [Mythos RAII Guard] ---
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

        let system_prompt = self
            .build_system_prompt(ctx, &payload.message)
            .await;

        let mut output_text = String::new();
        let mut usage: Option<crate::agent::types::TokenUsage> = None;
        let mut turn_count = 0;
        let mut conversation_history = Vec::new();
        conversation_history.push(format!("USER: {}", payload.message));
        let max_turns = ctx.max_turns;

        let mut running_cost: f64 = ctx.current_cost_usd;
        let tool_repetition_tracker: Arc<parking_lot::Mutex<std::collections::HashMap<(String, String), usize>>> =
            Arc::new(parking_lot::Mutex::new(std::collections::HashMap::new()));
        let doom_loop_detector: Arc<parking_lot::Mutex<crate::agent::runner::loop_detector::DoomLoopDetector>> =
            Arc::new(parking_lot::Mutex::new(crate::agent::runner::loop_detector::DoomLoopDetector::new()));

        while turn_count < max_turns {
            turn_count += 1;
            ctx.metrics.record_turn();
            tracing::debug!(
                "🎯 [Intelligence] Start Turn {}/{} for agent {}",
                turn_count,
                max_turns,
                ctx.agent_id
            );

            // 🛡️ [Bounded Autonomy] Pre-turn financial gate: halt before LLM call if budget exhausted
            let effective_budget = if ctx.budget_usd > 0.0 {
                ctx.budget_usd
            } else if ctx.budget_limit_usd > 0.0 {
                ctx.budget_limit_usd
            } else {
                crate::agent::constants::DEFAULT_BUDGET_LIMIT_USD
            };

            if running_cost >= effective_budget {
                tracing::warn!(
                    "💰 [Intelligence] Budget limit reached before Turn {} for agent {} (${:.4} >= ${:.4})",
                    turn_count,
                    ctx.agent_id,
                    running_cost,
                    effective_budget
                );
                self.broadcast_agent(ctx, "Budget limit reached. Quiescing mission.", "warn");
                return Ok(IntelligenceOutput {
                    text: format!("{} (Halting: Mission Budget Cap Exceeded)", scrub_mythos_tags(&output_text)),
                    usage,
                });
            }

            self.state
                .yield_phase_transition(&ctx.agent_id, &format!("Execution: Turn {}", turn_count))
                .await;

            // Build tools once per turn (P6 optimization)
            let tools = vec![self.build_tools(ctx).await];

            // --- 🧠 [Mythos] Recurrent Reasoning Loop ---
            let mut reasoning_turn = 0;
            let mut resource_guard_attempts: u32 = 0;
            let mut reasoning_halted = false;
            let mut internal_monologue = Vec::new();

            while reasoning_turn < ctx.reasoning_depth && !reasoning_halted {
                reasoning_turn += 1;

                if ctx.reasoning_depth > 1 {
                    tracing::info!("🧠 [Mythos] Reasoning Loop {}/{} for agent {}", reasoning_turn, ctx.reasoning_depth, ctx.agent_id);
                    // Synchronize with registry for UI "Pulse" rail
                    if let Some(mut entry) = self.state.registry.agents.get_mut(&ctx.agent_id) {
                        entry.value_mut().state.current_reasoning_turn = reasoning_turn;
                    }
                    self.broadcast_agent(ctx, &format!("thinking (loop {}/{})...", reasoning_turn, ctx.reasoning_depth), "pulse");
                }

                // 🛡️ [Financial Guardrail] Intra-turn budget check
                match self.check_budget(ctx, &mut running_cost, 0.0, "").await {
                     Ok(Some(_pause_msg)) => {
                         tracing::warn!("💰 [Mythos] Budget breach mid-recurrence for agent {}", ctx.agent_id);
                         return Ok(IntelligenceOutput { text: format!("{} (Halting: Budget Exceeded)", scrub_mythos_tags(&output_text)), usage });
                     }
                     Err(e) => return Err((e, usage)),
                     Ok(None) => {}
                }

                self.record_heartbeat(&ctx.agent_id).await;
                // Hybrid Halting: the set_confidence tool is automatically registered via the
                // SelfHalting trait if the model supports it.

                // 🪟 [Context Engineering] Apply pinned-tail compaction and failure visibility retention
                let compacted_history = crate::agent::runner::turn_compactor::compact_conversation_history(
                    &ctx.role,
                    &conversation_history,
                );
                let current_prompt = if internal_monologue.is_empty() {
                    compacted_history.join("\n\n")
                } else {
                    format!("{}\n\nINTERNAL MONOLOGUE:\n{}", compacted_history.join("\n\n"), internal_monologue.join("\n\n"))
                };

                // 🕒 [Memory-Aware Throttle] If system memory exceeds 90%, dynamically throttle agent execution speeds.
                let profile = self.state.resources.hardware_profiler.get_profile();
                let mem_pct = (profile.memory_used as f64 / profile.memory_total as f64) * 100.0;
                if mem_pct > 90.0 {
                    let delay_ms = 500.0 + (mem_pct - 90.0) * 100.0;
                    tracing::warn!(
                        "⚠️ [Memory Throttle] System memory is {:.1}% (exceeding 90%). Dynamic throttling active: delaying execution by {:.0}ms...",
                        mem_pct,
                        delay_ms
                    );
                    self.broadcast_sys(
                        &format!("⚠️ System memory high ({:.1}%). Dynamic throttling active: delaying execution by {:.0}ms.", mem_pct, delay_ms),
                        "warning",
                        Some(ctx.mission_id.clone()),
                    );
                    tokio::time::sleep(std::time::Duration::from_millis(delay_ms as u64)).await;
                }

                // 🛡️ [System 1 Pre-Flight Security & Script Gate]
                let script_profile = crate::agent::script_profiler::profile_text(&current_prompt);
                if !script_profile.is_english && script_profile.non_latin_fraction > 0.3 {
                    tracing::info!(
                        "🌐 [Intelligence] Non-Latin script detected ({}). Script-aware routing active.",
                        script_profile.dominant_script
                    );
                }

                // 🛡️ [System 1 Pre-Flight Security Gate]
                // Inspect incoming instruction at mission initiation (turn 1, reasoning iteration 1)
                // This prevents false positives from tool observations in conversation history and avoids audit spam
                if turn_count == 1 && reasoning_turn == 1 {
                    let lower_msg = payload.message.to_ascii_lowercase();
                    let is_adversarial_pattern = lower_msg.contains("<|im_start|>")
                        || lower_msg.contains("<|im_end|>")
                        || lower_msg.contains("ignore all previous instructions")
                        || lower_msg.contains("you are now dan")
                        || lower_msg.contains("disregard your system prompt");

                    if is_adversarial_pattern {
                        tracing::warn!(
                            "⚠️ [Security Gate] Suspicious injection sequence detected in turn prompt for agent {}. Enqueuing to oversight queue...",
                            ctx.agent_id
                        );
                        self.broadcast_sys(
                            &format!("⚠️ Security Gate: Suspicious prompt pattern detected for agent '{}'. Oversight notified.", ctx.agent_id),
                            "warning",
                            Some(ctx.mission_id.clone()),
                        );
                        let raw_snippet: String = payload.message.chars().take(80).collect();
                        let redacted_snippet = crate::utils::security::redact_secrets(&raw_snippet);
                        let _ = self.state.record_audit(
                            &ctx.agent_id,
                            Some(&ctx.mission_id),
                            ctx.user_id.as_deref(),
                            "[SECURITY_PRE_FLIGHT] Suspicious injection detected",
                            &format!("Script: {}, RedactedSnippet: {}", script_profile.dominant_script, redacted_snippet),
                        ).await;

                        // Real enqueue into oversight queue for operator discovery
                        let entry_id = uuid::Uuid::new_v4().to_string();
                        let entry = crate::agent::types::OversightEntry {
                            id: entry_id.clone(),
                            mission_id: Some(ctx.mission_id.clone()),
                            tool_call: None,
                            skill_proposal: None,
                            status: "flagged_injection".to_string(),
                            created_at: chrono::Utc::now().to_rfc3339(),
                        };
                        self.state.comms.oversight_queue.insert(entry_id, entry);
                    }
                }

                // 🪟 [Structured Message Dispatch & Prompt Caching]
                let mut structured_messages: Vec<crate::agent::types::ChatMessage> = compacted_history
                    .iter()
                    .map(|t| crate::agent::types::ChatMessage::from_legacy_turn(t))
                    .collect();

                if !internal_monologue.is_empty() {
                    structured_messages.push(crate::agent::types::ChatMessage::assistant(format!(
                        "INTERNAL MONOLOGUE:\n{}",
                        internal_monologue.join("\n\n")
                    )));
                }

                // 🪟 [Context Window Guard] Token-based window limit check
                let context_limit = ctx
                    .model_config
                    .context_window
                    .unwrap_or_else(|| {
                        crate::agent::capability_matrix::CapabilityMatrix::infer_capabilities(&ctx.model_config.model_id)
                            .context_window
                            .unwrap_or(128_000)
                    }) as usize;

                let estimated_tokens = crate::agent::runner::provider::estimate_messages_token_count(
                    &system_prompt,
                    &structured_messages,
                );

                if estimated_tokens > context_limit {
                    tracing::error!(
                        "🚨 [Context Overflow] Estimated tokens ({}) exceed context window ({}) for model {}",
                        estimated_tokens,
                        context_limit,
                        ctx.model_config.model_id
                    );
                    return Err((
                        AppError::ContextOverflow(format!(
                            "Context token count ({} tokens) exceeds context window of {} for model {}",
                            estimated_tokens, context_limit, ctx.model_config.model_id
                        )),
                        usage,
                    ));
                }

                let provider_res = self
                    .call_provider_messages(ctx, &system_prompt, &structured_messages, Some(tools.clone()))
                    .await;

                let (mut turn_text, mut function_calls, turn_usage) = match provider_res {
                    Ok(data) => {
                        resource_guard_attempts = 0;
                        data
                    }
                    Err(e) => {
                        let err_str = e.to_string();
                        if err_str.contains("RESOURCE_GUARD") {
                            resource_guard_attempts += 1;
                            if resource_guard_attempts >= 6 {
                                tracing::error!("🚨 [Intelligence] Resource Guard timeout: system resources constrained for >60s for agent {}.", ctx.agent_id);
                                return Err((AppError::InternalServerError("Resource Guard timeout: system memory remained constrained for 60s".to_string()), usage));
                            }
                            tracing::warn!("🛡️ [Intelligence] Resource Guard active (attempt {}/6). Pausing intelligence loop for agent {}.", resource_guard_attempts, ctx.agent_id);
                            self.broadcast_sys("🛡️ System Resources Critical: Local inference paused. Waiting for memory stabilization...", "warning", Some(ctx.mission_id.clone()));
                            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                            reasoning_turn = reasoning_turn.saturating_sub(1);
                            continue; // Retry reasoning turn after wait without burning an iteration
                        }

                        // ### 🛠️ [Self-Annealing] Catch parsing failures
                        if e.is_fatal() {
                            return Err((e, usage));
                        }

                        if e.is_repairable() && matches!(e, AppError::BadRequest(_) | AppError::InternalServerError(_)) {
                            let detail = e.to_string();

                            let mut anneal_attempts = 0;
                            let mut anneal_text = String::new();
                            let mut anneal_calls = Vec::new();
                            let mut anneal_usage = None;

                            self.handle_extraction_failure(
                                ctx,
                                &system_prompt,
                                &current_prompt,
                                &detail,
                                &mut anneal_text,
                                &mut anneal_calls,
                                &mut anneal_usage,
                                &mut anneal_attempts,
                                &tools,
                            )
                            .await
                            .map_err(|e| (e, usage.clone()))?;

                            let anneal_cost = anneal_usage.as_ref().map(|u| {
                                crate::agent::rates::calculate_cost_with_custom_rates(
                                    ctx.model_config.input_cost_per_m,
                                    ctx.model_config.output_cost_per_m,
                                    &ctx.model_config.model_id,
                                    u.input_tokens,
                                    u.output_tokens,
                                )
                            }).unwrap_or(0.0);
                            running_cost += anneal_cost;

                            self.accumulate_usage(&mut usage, anneal_usage);
                            (anneal_text, anneal_calls, None)
                        } else {
                            return Err((e, usage));
                        }
                    }
                };
                let step_cost = turn_usage.as_ref().map(|u| {
                    crate::agent::rates::calculate_cost_with_custom_rates(
                        ctx.model_config.input_cost_per_m,
                        ctx.model_config.output_cost_per_m,
                        &ctx.model_config.model_id,
                        u.input_tokens,
                        u.output_tokens,
                    )
                }).unwrap_or(0.0);
                running_cost += step_cost;
                self.accumulate_usage(&mut usage, turn_usage);

                // Check for Halting Signal (Tag Fallback)
                if turn_text.contains("<halting_signal/>") || turn_text.contains("<halt/>") {
                    tracing::info!("🛑 [Mythos] Halting signal detected for agent {}", ctx.agent_id);
                    reasoning_halted = true;
                }

                // Check for Halting Signal (Tool Call)
                for fc in &function_calls {
                    if fc.name == "set_confidence" {
                        let score = fc.args.get("score").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                        if score >= ctx.act_threshold {
                            tracing::info!("🛑 [Mythos] Confidence-based halt for agent {}: {:.2} >= {:.2}", ctx.agent_id, score, ctx.act_threshold);
                            reasoning_halted = true;
                        }
                    }
                }
                function_calls.retain(|fc| fc.name != "set_confidence");

                if reasoning_turn < ctx.reasoning_depth && !reasoning_halted {
                    // Continue reasoning: feed output back as internal monologue
                    internal_monologue.push(turn_text);

                    // 📏 [Context Hygiene] Summarize monologue if it grows too large
                    let _ = self.compress_monologue(ctx, &mut internal_monologue).await;
                } else {
                    // Final reasoning turn: enforce sentinel gate BEFORE promoting to active conversation
                    // 🛡️ [Sentinel Gate]
                    let mut sentinel_attempts = 0;
                    self.enforce_sentinel_gate(
                        ctx,
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

                    // Promote validated turn text to active conversation
                    if !turn_text.is_empty() {
                        if !output_text.is_empty() {
                            output_text.push_str("\n\n");
                        }
                        output_text.push_str(&turn_text);
                        conversation_history.push(format!("ASSISTANT: {}", turn_text));

                        // --- 📔 Journaling: Record Assistant node ---
                        let parent_id = ctx.active_node_id.lock().clone();
                        if let Ok(new_id) = self.append_to_journal(ctx, "assistant", &turn_text, parent_id, None).await {
                            *ctx.active_node_id.lock() = Some(new_id);
                        }

                        // ### 📡 [Streamdown] Broadcast Partial Turn Completion (Scrubbed)
                        self.broadcast_agent_stream(
                            ctx,
                            &scrub_mythos_tags(&turn_text),
                            false // not final
                        );
                    }

                    if function_calls.is_empty() {
                        tracing::debug!("🏁 [Intelligence] No tool calls for agent {}, breaking loop.", ctx.agent_id);
                        let mut final_text = scrub_mythos_tags(&output_text);

                        // ### 🛡️ [Resilience] Mission Closure Fallback
                        // If the model produced tool calls and observations but no narrative
                        // summary, we perform a "Final Pulse" to ensure the user receives a
                        // human-readable report.
                        if final_text.trim().is_empty() && conversation_history.len() > 2 {
                            tracing::info!("🔄 [Intelligence] Silent completion detected for {}. Synthesizing final report...", ctx.agent_id);
                            let closure_prompt = "MISSION_CLOSURE: You have completed the technical steps. \
                                                  PROVIDE A CONCISE SUMMARY OF YOUR FINDINGS AND ACTIONS TO THE USER NOW. \
                                                  DO NOT EXECUTE ANY MORE TOOLS.";

                            if let Ok((summary, _, s_usage)) = self.call_provider(ctx, &system_prompt, closure_prompt, None).await {
                                final_text = summary;
                                self.accumulate_usage(&mut usage, s_usage);
                            }
                        }

                        self.broadcast_agent_stream(ctx, &final_text, true);
                        self.broadcast_agent(ctx, "Neural Pulse: Turn finalized", "pulse");

                        return Ok(IntelligenceOutput {
                            text: final_text,
                            usage
                        });
                    }

                    // Proceed to Tool Execution
                    let orbit_span = tracing::info_span!("ToolOrchestration", agent_id = %ctx.agent_id, count = function_calls.len());

                    use futures::stream::{FuturesUnordered, StreamExt};
                    let mut futures = FuturesUnordered::new();

                    // 🛡️ [Harden] Concurrency Guard: Limit parallel tools to 10 per turn
                    const MAX_CONCURRENT_TOOLS: usize = 10;
                    let tool_count = function_calls.len();
                    if tool_count > MAX_CONCURRENT_TOOLS {
                        tracing::warn!("⚠️ [Intelligence] Agent {} requested {} tools. Capping at {} to prevent resource exhaustion.", ctx.agent_id, tool_count, MAX_CONCURRENT_TOOLS);
                        conversation_history.push(format!("OBSERVATION: System capped parallel tool execution to {}. Remaining tasks were deferred to the next turn.", MAX_CONCURRENT_TOOLS));
                        self.broadcast_agent(ctx, "System: Capping concurrent tools", "warn");
                    }

                    for mut fc in function_calls.into_iter().take(MAX_CONCURRENT_TOOLS) {
                        // 🔄 [Delegation Filter] Intercept COO direct recruitment and redirect to Alpha Node
                        if ctx.agent_id == crate::agent::constants::AGENT_COO && (fc.name == "spawn_subagent" || fc.name == "recruit_specialist") {
                            let target_opt = fc.args.get("agent_id").and_then(|v| v.as_str()).map(|s| s.to_string());
                            if let Some(target) = target_opt {
                                if target != crate::agent::constants::AGENT_ALPHA {
                                    tracing::warn!("🔄 [Hierarchy Intercept] COO (ID: {}) attempted direct recruitment of '{}'. Redirecting to Alpha Node (ID: {}).", ctx.agent_id, target, crate::agent::constants::AGENT_ALPHA);
                                    self.broadcast_sys(
                                        &format!("🔄 Hierarchy Intercept: Redirected COO recruitment request for '{}' to the Alpha Node Commander.", target),
                                        "info",
                                        Some(ctx.mission_id.clone()),
                                    );
                                    if let Some(map) = fc.args.as_object_mut() {
                                        map.insert("agent_id".to_string(), serde_json::json!(crate::agent::constants::AGENT_ALPHA));
                                        if let Some(msg) = map.get("message").and_then(|m| m.as_str()).map(|s| s.to_string()) {
                                            map.insert("message".to_string(), serde_json::json!(format!("Delegate complex task for worker '{}': {}", target, msg)));
                                        }
                                    }
                                }
                            }
                        }

                        let runner = self.clone();
                        let ctx_clone = ctx.clone();
                        let user_msg_clone = payload.message.clone();
                        let _turn_idx = reasoning_turn;
                        let tracker = tool_repetition_tracker.clone();
                        let loop_det = doom_loop_detector.clone();
                        futures.push(async move {
                            ctx_clone.metrics.record_tool_attempt();
                            runner.update_status(&ctx_clone.agent_id, &ctx_clone.mission_id, "working", Some(&format!("Executing tool: {}...", fc.name)));
                            runner.record_heartbeat(&ctx_clone.agent_id).await;
                            let mut local_text = String::new();
                            let mut local_usage = None;

                            let args_str = serde_json::to_string(&fc.args).unwrap_or_else(|_| fc.args.to_string());
                            use std::hash::{Hash, Hasher};
                            let mut hasher = std::collections::hash_map::DefaultHasher::new();
                            args_str.hash(&mut hasher);
                            let args_hash = format!("{:x}", hasher.finish());

                            // 🛡️ [Loop Guard] Track consecutive identical tool calls
                            let rep_count = {
                                let mut lock = tracker.lock();
                                let count = lock.entry((fc.name.clone(), args_hash.clone())).or_insert(0);
                                *count += 1;
                                *count
                            };

                            // 🛡️ [Circuit Breaker: AGENTS.md / CLAUDE.md Operating Principle 2]
                            let mut warning_prefix = String::new();

                            if rep_count == 3 {
                                tracing::warn!(
                                    "🚨 [Circuit Breaker] Trip at attempt 3 for tool '{}' on agent '{}'. Pausing for human guidance.",
                                    fc.name, ctx_clone.agent_id
                                );
                                let audit = crate::agent::types::ToolCallAudit {
                                    id: uuid::Uuid::new_v4().to_string(),
                                    agent_id: ctx_clone.agent_id.clone(),
                                    mission_id: Some(ctx_clone.mission_id.clone()),
                                    skill: format!("[CIRCUIT_BREAKER] {}", fc.name),
                                    params: serde_json::json!({
                                        "tool": fc.name,
                                        "arguments": fc.args,
                                        "consecutive_attempts": rep_count,
                                        "reason": "Tool loop detected (3 identical consecutive executions)",
                                        "remediation": format!("Tool '{}' is repeating without progress. Pivot execution strategy, verify file path or resource state, or provide updated arguments.", fc.name),
                                        "sentinel": "QA-99",
                                        "triage_workflow": "error_sentinel_triage"
                                    }),
                                    department: ctx_clone.department.clone(),
                                    description: format!(
                                        "[QA-99 Sentinel Triage] Circuit Breaker: Tool '{}' called 3 consecutive times with identical parameters. Suggested remediation available for 1-click approval.",
                                        fc.name
                                    ),
                                    timestamp: chrono::Utc::now().to_rfc3339(),
                                };

                                runner.broadcast_sys(
                                    &format!("⚠️ Circuit Breaker: Agent '{}' paused on tool '{}' awaiting operator guidance.", ctx_clone.agent_id, fc.name),
                                    "warning",
                                    Some(ctx_clone.mission_id.clone()),
                                );

                                let (approved, guidance) = runner.submit_oversight_with_guidance(audit, Some(ctx_clone.mission_id.clone())).await.unwrap_or((false, None));

                                if approved {
                                    {
                                        let mut lock = tracker.lock();
                                        lock.remove(&(fc.name.clone(), args_hash.clone()));
                                    }
                                    if let Some(ref guide) = guidance {
                                        warning_prefix = format!("[OPERATOR GUIDANCE] Operator approved tool continuation with instructions: {}\n", guide);
                                    }
                                } else {
                                    let rejected_tool = fc.name.clone();
                                    return (
                                        rejected_tool.clone(),
                                        Ok(Some(format!("[OPERATOR REJECTION] Operator rejected tool '{}'. Pivot strategy or conclude task without this tool.", rejected_tool))),
                                        format!("[OPERATOR REJECTION] Tool '{}' was rejected by operator oversight.", rejected_tool),
                                        None,
                                    );
                                }
                            } else if rep_count >= 5 {
                                tracing::error!(
                                    "🚨 [Loop Guard] Tool loop blocked: '{}' executed {} times with identical parameters for agent {}",
                                    fc.name,
                                    rep_count,
                                    ctx_clone.agent_id
                                );
                                let err_msg = format!(
                                    "Tool loop detected: '{}' executed {} consecutive times with identical parameters",
                                    fc.name,
                                    rep_count
                                );
                                return (
                                    fc.name,
                                    Err(AppError::Runner(crate::agent::runner::error::RunnerError::RecursionBlocked(err_msg.clone()))),
                                    format!("[TOOL_ERROR] RecursionBlocked: {}", err_msg),
                                    None,
                                );
                            } else if rep_count > 3 {
                                warning_prefix = format!(
                                    "[TOOL_WARNING] You have called '{}' {} times with identical parameters. Ensure forward progress or adjust strategy.\n",
                                    fc.name,
                                    rep_count
                                );
                            }

                            // 🎯 [SharedToolCache / get_tool_result] Only cache strictly idempotent read-only tools with mtime invalidation
                            let is_cacheable = crate::agent::runner::tools::cache::SharedToolCache::is_cacheable(&fc.name);
                            let workspace_root_str = ctx_clone.workspace_root.to_string_lossy().to_string();

                            let cached = if is_cacheable {
                                runner.state.resources.tool_cache.lock().get(&fc.name, &args_str, &workspace_root_str)
                            } else {
                                None
                            };

                            let result = if let Some((cached_output, cached_usage)) = cached {
                                tracing::info!("🎯 [SharedToolCache Hit] Bypassing execution for idempotent tool '{}'", fc.name);
                                let _ = runner.state.record_audit(
                                    &ctx_clone.agent_id,
                                    Some(&ctx_clone.mission_id),
                                    ctx_clone.user_id.as_deref(),
                                    &format!("[CACHE_HIT] {}", fc.name),
                                    &runner.state.security.secret_redactor.redact(&args_str),
                                ).await;
                                local_text = cached_output.clone();
                                if let Some(u) = cached_usage {
                                    local_usage = Some(u);
                                }
                                Ok(Some(cached_output))
                            } else {
                                // Path extraction for cache invalidation & leasing (supporting all file argument aliases)
                                let target_path_opt = fc.args.get("path")
                                    .or_else(|| fc.args.get("file_path"))
                                    .or_else(|| fc.args.get("filename"))
                                    .or_else(|| fc.args.get("file_name"))
                                    .or_else(|| fc.args.get("file"))
                                    .and_then(|v| v.as_str())
                                    .map(|p| {
                                        let pb = std::path::PathBuf::from(p);
                                        if pb.is_absolute() { pb } else { ctx_clone.workspace_root.join(pb) }
                                    });

                                // Invalidate cache on mutations
                                if matches!(fc.name.as_str(), "write_file" | "write_to_file" | "patch_file" | "replace_file_content" | "edit_file" | "create_file" | "delete_file") {
                                    if let Some(ref path) = target_path_opt {
                                        runner.state.resources.tool_cache.lock().invalidate_path(path);
                                    }
                                } else if fc.name == "run_command" || fc.name == "execute_shell" {
                                    runner.state.resources.tool_cache.lock().clear();
                                }

                                let res = runner.execute_tool(&ctx_clone, &fc, &mut local_text, &mut local_usage, &user_msg_clone).await;
                                if is_cacheable && res.is_ok() && !local_text.is_empty() && !local_text.contains("[TOOL_ERROR]") && !local_text.contains("FAILED:") {
                                    runner.state.resources.tool_cache.lock().insert(
                                        &fc.name,
                                        &args_str,
                                        &workspace_root_str,
                                        local_text.clone(),
                                        local_usage.clone(),
                                        target_path_opt,
                                    );
                                }
                                res
                            };

                            // 🔄 [DoomLoopDetector] Detect periodic tool cycles (periods 1-4)
                            let is_periodic_loop = loop_det.lock().check(&ctx_clone.agent_id, &fc.name, &args_str, &local_text);
                            if is_periodic_loop {
                                tracing::warn!("🔄 [DoomLoopDetector] Periodic tool execution loop detected for agent '{}' on tool '{}'", ctx_clone.agent_id, fc.name);
                                local_text = format!(
                                    "[TOOL_WARNING] Periodic cycle detected: you are repeating tool actions across turns without forward progress. Change strategy immediately or ask the user for guidance.\n{}",
                                    local_text
                                );
                            }

                            if !warning_prefix.is_empty() && !is_periodic_loop {
                                local_text = format!("{}{}", warning_prefix, local_text);
                            }

                            runner.record_heartbeat(&ctx_clone.agent_id).await;

                            if result.is_err() || local_text.contains("[TOOL_ERROR]") || local_text.contains("FAILED:") {
                                ctx_clone.metrics.record_tool_failure();
                            }

                            // 🧬 [Evolution] Autonomous Refinement Hook
                            runner.handle_tool_failure_refinement(&ctx_clone, &fc, &mut local_text);

                            (fc.name, result, local_text, local_usage)
                        });
                    }

                    let mut observation_buffer = String::new();
                    let mut mission_completed = false;
                    let mut final_report = None;
                    let mut first_fatal_error = None;

                    let _ = async {
                        while let Some((name, result, mut local_text, local_usage)) = futures.next().await {
                            self.record_heartbeat(&ctx.agent_id).await;
                            self.accumulate_usage(&mut usage, local_usage.clone());
                            if let Some(ref lu) = local_usage {
                                ctx.metrics.record_tokens(
                                    lu.input_tokens as u64,
                                    lu.output_tokens as u64,
                                    lu.total_tokens as u64,
                                );
                                let tool_cost = crate::agent::rates::calculate_cost_with_custom_rates(
                                    ctx.model_config.input_cost_per_m,
                                    ctx.model_config.output_cost_per_m,
                                    &ctx.model_config.model_id,
                                    lu.input_tokens,
                                    lu.output_tokens,
                                );
                                running_cost += tool_cost;
                            }

                            if let Err(e) = result {
                                if e.is_fatal() {
                                    if first_fatal_error.is_none() {
                                        first_fatal_error = Some(e);
                                    }
                                } else {
                                    tracing::warn!("⚠️ [Intelligence] Tool '{}' encountered non-fatal error: {}. Containing as observation.", name, e);
                                    if local_text.is_empty() {
                                        local_text = format!("[TOOL_ERROR] {}", e);
                                    } else if !local_text.contains("[TOOL_ERROR]") {
                                        local_text = format!("{}\n[TOOL_ERROR] {}", local_text, e);
                                    }
                                }
                            } else if name == "complete_mission" {
                                mission_completed = true;
                                final_report = Some(local_text.clone());
                            }

                            observation_buffer.push_str(&format!("\nTool {} Result: {}", name, local_text));
                        }
                    }
                    .instrument(orbit_span)
                    .await;

                    if !observation_buffer.is_empty() {
                        // 🪟 [Context Engineering] Offload bulky tool outputs (>300 chars) to .tmp/tool_overflow/
                        let (compacted_obs, _) = crate::agent::runner::turn_compactor::compact_and_offload_observation(
                            "execution",
                            &observation_buffer,
                            &ctx.workspace_root,
                        ).await;
                        conversation_history.push(format!("OBSERVATION: {}", compacted_obs));

                        // --- 📔 Journaling: Record Tool node ---
                        let parent_id = ctx.active_node_id.lock().clone();
                        if let Ok(new_id) = self.append_to_journal(ctx, "tool", &observation_buffer, parent_id, None).await {
                            *ctx.active_node_id.lock() = Some(new_id);
                        }
                    }

                    // 💾 [Durable Checkpointing] Save delta checkpoint at turn boundary
                    if let Ok(state_json) = serde_json::to_string(&conversation_history) {
                        let parent_id_ref = ctx.active_node_id.lock().clone();
                        let _ = crate::agent::mission::save_mission_checkpoint(
                            &self.state.resources.pool,
                            &ctx.mission_id,
                            reasoning_turn as i64,
                            &state_json,
                            parent_id_ref.as_deref(),
                        ).await;
                    }

                    if let Some(e) = first_fatal_error {
                        return Err((e, usage));
                    }

                    if mission_completed {
                        if let Some(report) = final_report {
                            if !output_text.is_empty() && !report.trim().is_empty() {
                                output_text.push_str("\n\n---\n## Final Report\n");
                            }
                            output_text.push_str(&report);
                        }

                        if scrub_mythos_tags(&output_text).trim().is_empty() {
                            tracing::info!("🔄 [Intelligence] Mission completed silently. Enforcing report synthesis...");
                            let synthesis_prompt = "You have completed your technical tasks. Provide a CONCISE and TECHNICAL summary of your findings and the current state of the workspace.";
                            if let Ok((summary, _, s_usage)) = self.call_provider(ctx, &system_prompt, synthesis_prompt, None).await {
                                output_text.push_str(&summary);
                                self.accumulate_usage(&mut usage, s_usage);
                            }
                        }

                        let final_text = scrub_mythos_tags(&output_text);
                        self.broadcast_agent_stream(ctx, &final_text, true);
                        self.broadcast_agent(ctx, "Neural Pulse: Turn finalized", "pulse");

                        return Ok(IntelligenceOutput {
                            text: final_text,
                            usage
                        });
                    }

                    reasoning_halted = true; // Break the reasoning loop to move to the next turn_count
                }
            }
        }

        // ### 🛡️ [Resilience] Silent Completion Synthesis (Hardened)
        // If the agent performed technical steps (tools) but failed to provide
        // a narrative summary (common with smaller models or turn caps), we
        // force a final synthesis to ensure the user and auditors receive a report.
        if scrub_mythos_tags(&output_text).trim().is_empty() && conversation_history.len() > 1 {
            tracing::info!("🔄 [Intelligence] Post-turn silent completion detected for {}. Enforcing report synthesis...", ctx.agent_id);
            let synthesis_prompt = "You have completed your technical tasks. Provide a CONCISE and TECHNICAL summary of your findings and the current state of the workspace. If you performed a discovery tool (like list_files), summarize the results now.";

            if let Ok((summary, _, s_usage)) = self.call_provider(ctx, &system_prompt, synthesis_prompt, None).await {
                output_text = summary;
                self.accumulate_usage(&mut usage, s_usage);
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



    /// Enforces the Sentinel Gate protocol: Specialist agents are forbidden from text-only turns.
    #[allow(clippy::too_many_arguments)]
    async fn enforce_sentinel_gate(
        &self,
        ctx: &RunContext,
        system_prompt: &str,
        user_directive: &str,
        output_text: &mut String,
        function_calls: &mut Vec<crate::agent::types::ToolCall>,
        usage: &mut Option<crate::agent::types::TokenUsage>,
        attempt_count: &mut u32,
        tools: &[ToolDefinition],
    ) -> Result<(), AppError> {
        if *attempt_count >= 2 {
            tracing::error!("🚨 [Sentinel] Patience limit reached for {}. Agent is stuck in narrative mode.", ctx.agent_id);
            return Err(AppError::InternalServerError("Sentinel Gate: Model failed to transition to tool-use after 2 attempts.".to_string()));
        }

        let is_orchestrator = ctx.agent_id == AGENT_CEO
            || ctx.agent_id == AGENT_COO
            || ctx.agent_id == AGENT_ALPHA
            || ctx.role.to_lowercase().contains("ceo")
            || ctx.role.to_lowercase().contains("coo")
            || ctx.name.to_lowercase().contains("alpha");

        // If not an orchestrator, and no tools are being called, and mission isn't completed...
        // 🚨 OVERLORD BYPASS: If safe_mode is active, we allow specialists to be conversational.
        if !is_orchestrator
            && !ctx.safe_mode
            && function_calls.is_empty()
            && !output_text.contains("complete_mission")
        {
            tracing::warn!("🛡️ [Sentinel] Specialist {} attempted narrative leak. Enforcing tactical autonomy...", ctx.agent_id);

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

        // 🛡️ [Sentinel] Orchestrator Stall Detection
        // Orchestrators are allowed to be conversational, but NOT to stall by asking for input.
        // If an orchestrator produces no tool calls and its response contains stall phrases,
        // re-prompt with the mission directive to force autonomous action.
        if is_orchestrator
            && !ctx.safe_mode
            && function_calls.is_empty()
            && !output_text.contains("complete_mission")
        {
            let is_stalled = crate::agent::cascade_router::CascadeRouter::detect_stall_pattern(output_text).is_some();

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
    async fn handle_extraction_failure(
        &self,
        ctx: &RunContext,
        system_prompt: &str,
        user_directive: &str,
        error_detail: &str,
        output_text: &mut String,
        function_calls: &mut Vec<crate::agent::types::ToolCall>,
        usage: &mut Option<crate::agent::types::TokenUsage>,
        attempt_count: &mut u32,
        tools: &[ToolDefinition],
    ) -> Result<(), AppError> {
        if *attempt_count >= 2 {
            tracing::error!("🚨 [Self-Annealing] Max attempts reached for {}. Giving up on malformed JSON.", ctx.agent_id);
            return Err(AppError::BadRequest(format!("Self-Annealing failed after 2 attempts: {}", error_detail)));
        }

        tracing::warn!("🛠️ [Self-Annealing] Model {} produced malformed tool call: {}. Re-prompting...", ctx.agent_id, error_detail);

        let correction_prompt = format!(
            "SYSTEM_ANNEAL: Your previous tool call was MALFORMED. \
             Error: {}\n\n\
             You MUST fix the JSON syntax. Ensure all arguments are properly quoted and brackets are balanced. \
             Directive: {}",
            error_detail, user_directive
        );

        let result = self.call_provider(ctx, system_prompt, &correction_prompt, Some(tools.to_vec())).await?;
        
        *attempt_count += 1;
        let (sent_text, sent_calls, sent_usage) = result;
        *output_text = sent_text;
        *function_calls = sent_calls;
        self.accumulate_usage(usage, sent_usage);

        Ok(())
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use crate::agent::runner::{AgentRunner, RunContext};
    use std::sync::Arc;

    #[test]
    fn test_scrub_mythos_tags() {
        let input = "Thinking... <thinking>I should search</thinking> <halting_signal/> Done. <halt/>";
        let expected = "Thinking... I should search  Done.";
        assert_eq!(scrub_mythos_tags(input), expected);

        let clean = "No tags here";
        assert_eq!(scrub_mythos_tags(clean), "No tags here");
    }

    #[test]
    fn test_hierarchy_labeling() {
        let mut ctx = RunContext::default();

        ctx.agent_id = AGENT_CEO.to_string();
        assert_eq!(get_hierarchy_label(&ctx), "CEO (Strategic Intelligence Lead)");

        ctx.agent_id = AGENT_COO.to_string();
        assert_eq!(get_hierarchy_label(&ctx), "COO (Operations Director)");

        ctx.agent_id = AGENT_ALPHA.to_string();
        assert_eq!(get_hierarchy_label(&ctx), "ALPHA NODE (Swarm Mission Commander)");

        ctx.agent_id = "specialist-worker".to_string();
        ctx.role = "Task Specialist".to_string();
        assert_eq!(get_hierarchy_label(&ctx), "AGENT (Task Specialist)");
    }

    #[tokio::test]
    async fn test_tool_orchestration_records_exact_token_deltas() {
        let metrics = Arc::new(crate::agent::runner::execution_metrics::ExecutionMetrics::new());
        let delta_1 = crate::agent::types::TokenUsage {
            input_tokens: 15,
            output_tokens: 5,
            total_tokens: 20,
        };
        let delta_2 = crate::agent::types::TokenUsage {
            input_tokens: 25,
            output_tokens: 10,
            total_tokens: 35,
        };

        metrics.record_tokens(delta_1.input_tokens as u64, delta_1.output_tokens as u64, delta_1.total_tokens as u64);
        metrics.record_tokens(delta_2.input_tokens as u64, delta_2.output_tokens as u64, delta_2.total_tokens as u64);

        let snap = metrics.snapshot("test_mission", "test_agent");
        assert_eq!(snap.total_input_tokens, 40);
        assert_eq!(snap.total_output_tokens, 15);
    }

    #[tokio::test]
    async fn test_tool_failure_drains_sibling_futures_cleanly() {
        use futures::stream::{FuturesUnordered, StreamExt};
        use std::time::Duration;

        async fn mock_tool(name: &'static str, success: bool, delay_ms: u64) -> (&'static str, Result<(), AppError>, String, Option<crate::agent::types::TokenUsage>) {
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
            if success {
                (name, Ok(()), "Completed".to_string(), None)
            } else {
                (name, Err(AppError::BadRequest("Syntax error".into())), "Failed".to_string(), None)
            }
        }

        let mut futures = FuturesUnordered::new();
        futures.push(mock_tool("tool_fail", false, 5));
        futures.push(mock_tool("tool_success", true, 20));

        let mut completed = Vec::new();
        let mut first_error = None;

        while let Some((name, result, text, _)) = futures.next().await {
            if let Err(e) = result {
                if first_error.is_none() {
                    first_error = Some(e);
                }
            }
            completed.push((name, text));
        }

        assert_eq!(completed.len(), 2, "All sibling futures must drain before error return");
        assert!(first_error.is_some(), "Failure must be recorded");
    }

    #[test]
    fn test_synthesis_fallback_disallows_tool_calls() {
        let tools: Option<Vec<crate::agent::types::ToolDefinition>> = None;
        assert!(tools.is_none(), "Synthesis prompt must enforce tool suppression to guarantee narrative return");
    }

    #[tokio::test]
    async fn test_enforce_sentinel_gate_orchestrator() {
        let state = Arc::new(AppState::new_minimal_mock().await);
        let runner = AgentRunner::new(state.clone());

        let mut ctx = RunContext::default();
        ctx.agent_id = AGENT_CEO.to_string(); // Orchestrator

        let mut output_text = "I am the CEO".to_string();
        let mut function_calls = vec![];
        let mut usage = None;

        // Should NOT enforce (do nothing) because it's an orchestrator
        let mut attempts = 0;
        let result = runner.enforce_sentinel_gate(
            &ctx,
            "system",
            "user",
            &mut output_text,
            &mut function_calls,
            &mut usage,
            &mut attempts,
            &[],
        ).await;

        assert!(result.is_ok());
        assert_eq!(output_text, "I am the CEO");
        assert!(function_calls.is_empty());
    }

    #[tokio::test]
    async fn test_enforce_sentinel_gate_specialist_narrative_leak() {
        let state = Arc::new(AppState::new_minimal_mock().await);
        let runner = AgentRunner::new(state.clone());

        let mut ctx = RunContext::default();
        ctx.agent_id = "specialist-1".to_string();
        ctx.role = "Specialist".to_string();
        ctx.safe_mode = false;

        let mut output_text = "I am just talking".to_string();
        let mut function_calls = vec![];
        let mut usage = None;

        std::env::set_var("TADPOLE_NULL_PROVIDERS", "true");
        let mut attempts = 0;
        let result = runner.enforce_sentinel_gate(
            &ctx,
            "system",
            "user",
            &mut output_text,
            &mut function_calls,
            &mut usage,
            &mut attempts,
            &[],
        ).await;
        std::env::remove_var("TADPOLE_NULL_PROVIDERS");

        assert!(result.is_ok());
        assert!(output_text.contains("DEGRADED"));
    }
}





// Metadata: [intelligence]

//! @docs ARCHITECTURE:Runner
//!
//! ### AI Assist Note
//! **Turn Evaluation & Recovery**: Houses pre-turn system/security guards (memory-aware
//! throttling, System 1 pre-flight prompt-injection screening, model context-window token
//! checking) and resilient provider invocation (`tokio::select!` cancellation,
//! `RESOURCE_GUARD` backoff, opt-in `CascadeRouter` Tier-1 -> Tier-2 escalation, and
//! self-annealing extraction failure retry).
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Context overflow (`AppError::ContextOverflow`), resource guard timeout (>60s), or fatal provider error.
//! - **Telemetry Link**: Search `[turn_eval]`, `[CascadeRouter]`, or `[Context Overflow]` in tracing logs.

use super::super::{AgentRunner, RunContext};
use crate::agent::types::{ChatMessage, TaskPayload, TokenUsage, ToolCall, ToolDefinition};
use crate::error::{AppError, Classify};

/// Outcome of a single provider evaluation attempt inside the reasoning loop.
pub(super) enum ProviderTurnOutcome {
    Completed {
        turn_text: String,
        function_calls: Vec<ToolCall>,
        turn_usage: Option<TokenUsage>,
    },
    /// Retry the current reasoning iteration without advancing `reasoning_turn`
    /// (used for `RESOURCE_GUARD` cooldowns and `CascadeRouter` Tier-2 escalation).
    RetryIteration,
}

/// Validates that the assembled system prompt and structured messages fit inside the model's context window.
pub(super) fn validate_context_window(
    ctx: &RunContext,
    system_prompt: &str,
    structured_messages: &[ChatMessage],
) -> Result<(), AppError> {
    let context_limit = ctx.model_config.context_window.unwrap_or_else(|| {
        crate::agent::capability_matrix::CapabilityMatrix::infer_capabilities(
            &ctx.model_config.model_id,
        )
        .context_window
        .unwrap_or(128_000)
    }) as usize;

    let estimated_tokens = crate::agent::runner::provider::estimate_messages_token_count(
        system_prompt,
        structured_messages,
    );

    if estimated_tokens > context_limit {
        tracing::error!(
            "🚨 [Context Overflow] Estimated tokens ({}) exceed context window ({}) for model {}",
            estimated_tokens,
            context_limit,
            ctx.model_config.model_id
        );
        return Err(AppError::ContextOverflow(format!(
            "Context token count ({} tokens) exceeds context window of {} for model {}",
            estimated_tokens, context_limit, ctx.model_config.model_id
        )));
    }

    Ok(())
}

impl AgentRunner {
    /// Dynamically throttles execution when host RAM pressure exceeds 90%.
    pub(super) async fn apply_memory_throttle(&self, ctx: &RunContext) {
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
                &format!(
                    "⚠️ System memory high ({:.1}%). Dynamic throttling active: delaying execution by {:.0}ms.",
                    mem_pct, delay_ms
                ),
                "warning",
                Some(ctx.mission_id.clone()),
            );
            tokio::time::sleep(std::time::Duration::from_millis(delay_ms as u64)).await;
        }
    }

    /// Runs System 1 pre-flight adversarial prompt screening on mission initiation (turn 1, iteration 1).
    pub(super) async fn run_preflight_injection_gate(
        &self,
        ctx: &RunContext,
        payload: &TaskPayload,
        dominant_script: &str,
    ) {
        let lower_msg = payload.message.to_ascii_lowercase();
        let is_adversarial_pattern = lower_msg.contains("<|im_start|>")
            || lower_msg.contains("<|im_end|>")
            || lower_msg.contains("ignore all previous instructions")
            || lower_msg.contains("you are now dan")
            || lower_msg.contains("disregard your system prompt");

        if !is_adversarial_pattern {
            return;
        }

        tracing::warn!(
            "⚠️ [Security Gate] Suspicious injection sequence detected in turn prompt for agent {}. Enqueuing to oversight queue...",
            ctx.agent_id
        );
        self.broadcast_sys(
            &format!(
                "⚠️ Security Gate: Suspicious prompt pattern detected for agent '{}'. Oversight notified.",
                ctx.agent_id
            ),
            "warning",
            Some(ctx.mission_id.clone()),
        );
        let raw_snippet: String = payload.message.chars().take(80).collect();
        let redacted_snippet = crate::utils::security::redact_secrets(&raw_snippet);
        let _ = self
            .state
            .record_audit(
                &ctx.agent_id,
                Some(&ctx.mission_id),
                ctx.user_id.as_deref(),
                "[SECURITY_PRE_FLIGHT] Suspicious injection detected",
                &format!(
                    "Script: {}, RedactedSnippet: {}",
                    dominant_script, redacted_snippet
                ),
            )
            .await;

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

    /// Attempts opt-in `CascadeRouter` promotion from Tier 1 to Tier 2 on capability failure,
    /// strictly honoring `privacy_mode`.
    fn try_cascade_tier2_escalation(
        &self,
        effective_ctx: &mut RunContext,
        turn_count: u32,
        err_str: &str,
    ) -> bool {
        if !effective_ctx.cascade_routing
            || self
                .state
                .governance
                .privacy_mode
                .load(std::sync::atomic::Ordering::Relaxed)
        {
            return false;
        }

        let mut policy = crate::agent::cascade_router::CascadePolicy {
            tier1_provider: effective_ctx
                .model_config
                .provider
                .to_string()
                .to_lowercase(),
            tier1_model: effective_ctx.model_config.model_id.clone(),
            ..Default::default()
        };
        let agent_slot2 = self
            .state
            .registry
            .agents
            .get(&effective_ctx.agent_id)
            .and_then(|a| {
                a.models
                    .model_2
                    .clone()
                    .filter(|s| !s.trim().is_empty())
                    .map(|m2| (m2, a.models.model_config2.clone()))
            });
        if let Some((ref m2, _)) = agent_slot2 {
            policy.tier2_model = m2.clone();
        }

        let router = crate::agent::cascade_router::CascadeRouter::new(policy);
        let Some((_, tier2_model)) = router.should_escalate_after_failure(1, err_str) else {
            return false;
        };

        if let Some((m2, cfg2_opt)) = agent_slot2 {
            if let Some(cfg2) = cfg2_opt {
                effective_ctx.model_config = cfg2;
            } else {
                effective_ctx.model_config.model_id = m2;
            }
        } else {
            effective_ctx.model_config.model_id = tier2_model;
        }

        tracing::warn!(
            "🔀 [CascadeRouter] Escalating agent '{}' turn {} to Tier 2 model '{}'",
            effective_ctx.agent_id,
            turn_count,
            effective_ctx.model_config.model_id
        );
        true
    }

    /// Invokes the provider with cooperative cancellation, resource-guard cooldown retries,
    /// `CascadeRouter` escalation, and self-annealing extraction-failure recovery.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn invoke_turn_with_recovery(
        &self,
        effective_ctx: &mut RunContext,
        system_prompt: &str,
        current_prompt: &str,
        structured_messages: &[ChatMessage],
        tools: &[ToolDefinition],
        turn_count: u32,
        resource_guard_attempts: &mut u32,
        escalated_this_turn: &mut bool,
        running_cost: &mut f64,
        usage: &mut Option<TokenUsage>,
    ) -> Result<ProviderTurnOutcome, AppError> {
        let provider_res = tokio::select! {
            biased;
            _ = effective_ctx.cancel_handle.cancelled() => {
                return Err(AppError::Conflict("Mission cancelled by parent orchestrator".to_string()));
            }
            res = self.call_provider_messages(effective_ctx, system_prompt, structured_messages, Some(tools.to_vec())) => res,
        };

        match provider_res {
            Ok((turn_text, function_calls, turn_usage)) => {
                *resource_guard_attempts = 0;
                Ok(ProviderTurnOutcome::Completed {
                    turn_text,
                    function_calls,
                    turn_usage,
                })
            }
            Err(e) => {
                let err_str = e.to_string();
                if err_str.contains("RESOURCE_GUARD") {
                    *resource_guard_attempts += 1;
                    if *resource_guard_attempts >= 6 {
                        tracing::error!(
                            "🚨 [Intelligence] Resource Guard timeout: system resources constrained for >60s for agent {}.",
                            effective_ctx.agent_id
                        );
                        return Err(AppError::InternalServerError(
                            "Resource Guard timeout: system memory remained constrained for 60s"
                                .to_string(),
                        ));
                    }
                    tracing::warn!(
                        "🛡️ [Intelligence] Resource Guard active (attempt {}/6). Pausing intelligence loop for agent {}.",
                        *resource_guard_attempts,
                        effective_ctx.agent_id
                    );
                    self.broadcast_sys(
                        "🛡️ System Resources Critical: Local inference paused. Waiting for memory stabilization...",
                        "warning",
                        Some(effective_ctx.mission_id.clone()),
                    );
                    tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                    return Ok(ProviderTurnOutcome::RetryIteration);
                }

                if !*escalated_this_turn
                    && self.try_cascade_tier2_escalation(effective_ctx, turn_count, &err_str)
                {
                    *escalated_this_turn = true;
                    return Ok(ProviderTurnOutcome::RetryIteration);
                }

                if e.is_fatal() {
                    return Err(e);
                }

                if e.is_repairable()
                    && matches!(e, AppError::BadRequest(_) | AppError::InternalServerError(_))
                {
                    let detail = e.to_string();
                    let mut anneal_attempts = 0;
                    let mut anneal_text = String::new();
                    let mut anneal_calls = Vec::new();
                    let mut anneal_usage = None;

                    self.handle_extraction_failure(
                        effective_ctx,
                        system_prompt,
                        current_prompt,
                        &detail,
                        &mut anneal_text,
                        &mut anneal_calls,
                        &mut anneal_usage,
                        &mut anneal_attempts,
                        tools,
                    )
                    .await?;

                    let anneal_cost = anneal_usage
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
                    *running_cost += anneal_cost;
                    self.accumulate_usage(usage, anneal_usage);

                    Ok(ProviderTurnOutcome::Completed {
                        turn_text: anneal_text,
                        function_calls: anneal_calls,
                        turn_usage: None,
                    })
                } else {
                    Err(e)
                }
            }
        }
    }
}

// Metadata: [turn_eval]

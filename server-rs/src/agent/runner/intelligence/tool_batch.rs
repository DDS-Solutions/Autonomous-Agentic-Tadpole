//! @docs ARCHITECTURE:Runner
//!
//! ### AI Assist Note
//! **Parallel Tool Batch Orchestrator**: Executes up to `MAX_CONCURRENT_TOOLS` (10)
//! tool calls per turn via `FuturesUnordered`, enforcing COO-to-Alpha hierarchy
//! interception, consecutive repetition circuit-breaking (`evaluate_loop_repetition`),
//! `SharedToolCache` mtime lookup/invalidation, periodic `DoomLoopDetector` checks,
//! and clean sibling future draining on error.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Tool loop recursion block, QA-99 circuit-breaker rejection, or fatal sibling tool error.
//! - **Telemetry Link**: Search `[tool_batch]`, `ToolOrchestration`, or `[DoomLoopDetector]` in tracing logs.

use super::super::{AgentRunner, RunContext};
use crate::agent::types::{TokenUsage, ToolCall};
use crate::error::{AppError, Classify};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::Instrument;

pub(super) const MAX_CONCURRENT_TOOLS: usize = 10;

/// Deterministic decision returned by the tool loop guard based on consecutive identical calls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LoopRepetitionAction {
    Allow,
    CircuitBreakerOversight,
    WarnAndContinue,
    BlockRecursion,
}

/// Evaluates how the intelligence loop should handle `rep_count` consecutive identical tool calls.
pub(crate) fn evaluate_loop_repetition(rep_count: usize) -> LoopRepetitionAction {
    if rep_count == 3 {
        LoopRepetitionAction::CircuitBreakerOversight
    } else if rep_count >= 5 {
        LoopRepetitionAction::BlockRecursion
    } else if rep_count > 3 {
        LoopRepetitionAction::WarnAndContinue
    } else {
        LoopRepetitionAction::Allow
    }
}

pub(super) struct ToolBatchOutcome {
    pub observation_buffer: String,
    pub mission_completed: bool,
    pub final_report: Option<String>,
    pub first_fatal_error: Option<AppError>,
}

impl AgentRunner {
    /// Redirects COO direct worker recruitment requests to the Alpha Node commander.
    fn apply_coo_hierarchy_intercept(&self, ctx: &RunContext, fc: &mut ToolCall) {
        if ctx.agent_id != crate::agent::constants::AGENT_COO
            || (fc.name != "spawn_subagent" && fc.name != "recruit_specialist")
        {
            return;
        }

        let target_opt = fc
            .args
            .get("agent_id")
            .and_then(|v| v.as_str())
            .map(ToString::to_string);

        let Some(target) = target_opt else {
            return;
        };

        if target == crate::agent::constants::AGENT_ALPHA {
            return;
        }

        tracing::warn!(
            "🔄 [Hierarchy Intercept] COO (ID: {}) attempted direct recruitment of '{}'. Redirecting to Alpha Node (ID: {}).",
            ctx.agent_id,
            target,
            crate::agent::constants::AGENT_ALPHA
        );
        self.broadcast_sys(
            &format!(
                "🔄 Hierarchy Intercept: Redirected COO recruitment request for '{}' to the Alpha Node Commander.",
                target
            ),
            "info",
            Some(ctx.mission_id.clone()),
        );
        if let Some(map) = fc.args.as_object_mut() {
            map.insert(
                "agent_id".to_string(),
                serde_json::json!(crate::agent::constants::AGENT_ALPHA),
            );
            if let Some(msg) = map
                .get("message")
                .and_then(|m| m.as_str())
                .map(ToString::to_string)
            {
                map.insert(
                    "message".to_string(),
                    serde_json::json!(format!(
                        "Delegate complex task for worker '{}': {}",
                        target, msg
                    )),
                );
            }
        }
    }

    /// Executes a bounded batch of parallel tool calls, draining all sibling futures before returning.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn execute_tool_batch(
        &self,
        ctx: &RunContext,
        function_calls: Vec<ToolCall>,
        user_message: &str,
        conversation_history: &mut Vec<String>,
        tool_repetition_tracker: &Arc<parking_lot::Mutex<HashMap<(String, String), usize>>>,
        doom_loop_detector: &Arc<
            parking_lot::Mutex<crate::agent::runner::loop_detector::DoomLoopDetector>,
        >,
        running_cost: &mut f64,
        usage: &mut Option<TokenUsage>,
    ) -> ToolBatchOutcome {
        let orbit_span = tracing::info_span!(
            "ToolOrchestration",
            agent_id = %ctx.agent_id,
            count = function_calls.len()
        );

        use futures::stream::{FuturesUnordered, StreamExt};
        let mut futures = FuturesUnordered::new();

        let tool_count = function_calls.len();
        if tool_count > MAX_CONCURRENT_TOOLS {
            tracing::warn!(
                "⚠️ [Intelligence] Agent {} requested {} tools. Capping at {} to prevent resource exhaustion.",
                ctx.agent_id,
                tool_count,
                MAX_CONCURRENT_TOOLS
            );
            conversation_history.push(format!(
                "OBSERVATION: System capped parallel tool execution to {}. Remaining tasks were deferred to the next turn.",
                MAX_CONCURRENT_TOOLS
            ));
            self.broadcast_agent(ctx, "System: Capping concurrent tools", "warn");
        }

        for mut fc in function_calls.into_iter().take(MAX_CONCURRENT_TOOLS) {
            self.apply_coo_hierarchy_intercept(ctx, &mut fc);

            let runner = self.clone();
            let ctx_clone = ctx.clone();
            let user_msg_clone = user_message.to_string();
            let tracker = tool_repetition_tracker.clone();
            let loop_det = doom_loop_detector.clone();

            futures.push(async move {
                ctx_clone.metrics.record_tool_attempt();
                runner.update_status(
                    &ctx_clone.agent_id,
                    &ctx_clone.mission_id,
                    "working",
                    Some(&format!("Executing tool: {}...", fc.name)),
                );
                runner.record_heartbeat(&ctx_clone.agent_id).await;
                let mut local_text = String::new();
                let mut local_usage = None;

                let args_str = serde_json::to_string(&fc.args).unwrap_or_else(|_| fc.args.to_string());
                use std::hash::{Hash, Hasher};
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                args_str.hash(&mut hasher);
                let args_hash = format!("{:x}", hasher.finish());

                let rep_count = {
                    let mut lock = tracker.lock();
                    let count = lock.entry((fc.name.clone(), args_hash.clone())).or_insert(0);
                    *count += 1;
                    *count
                };

                let mut warning_prefix = String::new();

                match evaluate_loop_repetition(rep_count) {
                    LoopRepetitionAction::CircuitBreakerOversight => {
                        tracing::warn!(
                            "🚨 [Circuit Breaker] Trip at attempt 3 for tool '{}' on agent '{}'. Pausing for human guidance.",
                            fc.name,
                            ctx_clone.agent_id
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
                            &format!(
                                "⚠️ Circuit Breaker: Agent '{}' paused on tool '{}' awaiting operator guidance.",
                                ctx_clone.agent_id, fc.name
                            ),
                            "warning",
                            Some(ctx_clone.mission_id.clone()),
                        );

                        let (approved, guidance) = runner
                            .submit_oversight_with_guidance(audit, Some(ctx_clone.mission_id.clone()))
                            .await
                            .unwrap_or((false, None));

                        if approved {
                            {
                                let mut lock = tracker.lock();
                                lock.remove(&(fc.name.clone(), args_hash.clone()));
                            }
                            if let Some(ref guide) = guidance {
                                warning_prefix = format!(
                                    "[OPERATOR GUIDANCE] Operator approved tool continuation with instructions: {}\n",
                                    guide
                                );
                            }
                        } else {
                            let rejected_tool = fc.name.clone();
                            return (
                                rejected_tool.clone(),
                                Ok(Some(format!(
                                    "[OPERATOR REJECTION] Operator rejected tool '{}'. Pivot strategy or conclude task without this tool.",
                                    rejected_tool
                                ))),
                                format!(
                                    "[OPERATOR REJECTION] Tool '{}' was rejected by operator oversight.",
                                    rejected_tool
                                ),
                                None,
                            );
                        }
                    }
                    LoopRepetitionAction::BlockRecursion => {
                        tracing::error!(
                            "🚨 [Loop Guard] Tool loop blocked: '{}' executed {} times with identical parameters for agent {}",
                            fc.name,
                            rep_count,
                            ctx_clone.agent_id
                        );
                        let err_msg = format!(
                            "Tool loop detected: '{}' executed {} consecutive times with identical parameters",
                            fc.name, rep_count
                        );
                        return (
                            fc.name,
                            Err(AppError::Runner(
                                crate::agent::runner::error::RunnerError::RecursionBlocked(
                                    err_msg.clone(),
                                ),
                            )),
                            format!("[TOOL_ERROR] RecursionBlocked: {}", err_msg),
                            None,
                        );
                    }
                    LoopRepetitionAction::WarnAndContinue => {
                        warning_prefix = format!(
                            "[TOOL_WARNING] You have called '{}' {} times with identical parameters. Ensure forward progress or adjust strategy.\n",
                            fc.name, rep_count
                        );
                    }
                    LoopRepetitionAction::Allow => {}
                }

                // SharedToolCache: strictly idempotent read-only tools with mtime invalidation
                let is_cacheable =
                    crate::agent::runner::tools::cache::SharedToolCache::is_cacheable(&fc.name);
                let workspace_root_str = ctx_clone.workspace_root.to_string_lossy().to_string();

                let cached = if is_cacheable {
                    runner
                        .state
                        .resources
                        .tool_cache
                        .lock()
                        .get(&fc.name, &args_str, &workspace_root_str)
                } else {
                    None
                };

                let result = if let Some((cached_output, cached_usage)) = cached {
                    tracing::info!(
                        "🎯 [SharedToolCache Hit] Bypassing execution for idempotent tool '{}'",
                        fc.name
                    );
                    let _ = runner
                        .state
                        .record_audit(
                            &ctx_clone.agent_id,
                            Some(&ctx_clone.mission_id),
                            ctx_clone.user_id.as_deref(),
                            &format!("[CACHE_HIT] {}", fc.name),
                            &runner.state.security.secret_redactor.redact(&args_str),
                        )
                        .await;
                    local_text = cached_output.clone();
                    if let Some(u) = cached_usage {
                        local_usage = Some(u);
                    }
                    Ok(Some(cached_output))
                } else {
                    let target_path_opt = crate::agent::runner::tools::extract_target_path(&fc.args)
                        .map(|p| {
                            let pb = std::path::PathBuf::from(p);
                            if pb.is_absolute() {
                                pb
                            } else {
                                ctx_clone.workspace_root.join(pb)
                            }
                        });

                    if matches!(
                        fc.name.as_str(),
                        "write_file"
                            | "write_to_file"
                            | "patch_file"
                            | "replace_file_content"
                            | "edit_file"
                            | "create_file"
                            | "delete_file"
                    ) {
                        if let Some(ref path) = target_path_opt {
                            runner.state.resources.tool_cache.lock().invalidate_path(path);
                        }
                    } else if fc.name == "run_command" || fc.name == "execute_shell" {
                        runner.state.resources.tool_cache.lock().clear();
                    }

                    let res = runner
                        .execute_tool(&ctx_clone, &fc, &mut local_text, &mut local_usage, &user_msg_clone)
                        .await;
                    if is_cacheable
                        && res.is_ok()
                        && !local_text.is_empty()
                        && !local_text.contains("[TOOL_ERROR]")
                        && !local_text.contains("FAILED:")
                    {
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

                // DoomLoopDetector: periodic tool cycles (periods 1-4)
                let loop_check = loop_det
                    .lock()
                    .check(&ctx_clone.agent_id, &fc.name, &args_str, &local_text);
                match loop_check {
                    crate::agent::runner::loop_detector::LoopCheckResult::Terminal => {
                        tracing::error!(
                            "🚨 [DoomLoopDetector] Terminal cyclic repetition loop detected for agent '{}' on tool '{}'",
                            ctx_clone.agent_id,
                            fc.name
                        );
                        let err_msg = format!(
                            "Doom loop detected: cyclic repetition breached threshold on tool '{}'. Halting execution to prevent token drain.",
                            fc.name
                        );
                        return (
                            fc.name,
                            Err(AppError::Runner(
                                crate::agent::runner::error::RunnerError::RecursionBlocked(
                                    err_msg.clone(),
                                ),
                            )),
                            format!("[TOOL_ERROR] RecursionBlocked: {}", err_msg),
                            None,
                        );
                    }
                    crate::agent::runner::loop_detector::LoopCheckResult::Warning => {
                        tracing::warn!(
                            "🔄 [DoomLoopDetector] Periodic tool execution warning for agent '{}' on tool '{}'",
                            ctx_clone.agent_id,
                            fc.name
                        );
                        local_text = format!(
                            "[TOOL_WARNING] Periodic cycle detected: you are repeating tool actions across turns without forward progress. Change strategy immediately or ask the user for guidance.\n{}",
                            local_text
                        );
                    }
                    crate::agent::runner::loop_detector::LoopCheckResult::Ok => {}
                }

                if !warning_prefix.is_empty() && !loop_check.is_warning() {
                    local_text = format!("{}{}", warning_prefix, local_text);
                }

                runner.record_heartbeat(&ctx_clone.agent_id).await;

                if result.is_err() || local_text.contains("[TOOL_ERROR]") || local_text.contains("FAILED:") {
                    ctx_clone.metrics.record_tool_failure();
                }

                runner.handle_tool_failure_refinement(&ctx_clone, &fc, &mut local_text);

                (fc.name, result, local_text, local_usage)
            });
        }

        let mut outcome = ToolBatchOutcome {
            observation_buffer: String::new(),
            mission_completed: false,
            final_report: None,
            first_fatal_error: None,
        };

        let _ = async {
            while let Some((name, result, mut local_text, local_usage)) = futures.next().await {
                self.record_heartbeat(&ctx.agent_id).await;
                self.accumulate_usage(usage, local_usage.clone());
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
                    *running_cost += tool_cost;
                }

                if let Err(e) = result {
                    if e.is_fatal() {
                        if outcome.first_fatal_error.is_none() {
                            outcome.first_fatal_error = Some(e);
                        }
                    } else {
                        tracing::warn!(
                            "⚠️ [Intelligence] Tool '{}' encountered non-fatal error: {}. Containing as observation.",
                            name,
                            e
                        );
                        if local_text.is_empty() {
                            local_text = format!("[TOOL_ERROR] {}", e);
                        } else if !local_text.contains("[TOOL_ERROR]") {
                            local_text = format!("{}\n[TOOL_ERROR] {}", local_text, e);
                        }
                    }
                } else if name == "complete_mission" {
                    outcome.mission_completed = true;
                    outcome.final_report = Some(local_text.clone());
                }

                outcome
                    .observation_buffer
                    .push_str(&format!("\nTool {} Result: {}", name, local_text));
            }
        }
        .instrument(orbit_span)
        .await;

        outcome
    }
}

// Metadata: [tool_batch]

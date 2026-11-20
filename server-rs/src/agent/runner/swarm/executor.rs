//! @docs ARCHITECTURE:Runner
//!
//! ### AI Assist Note
//! **Swarm Parallel DAG Executor**: Executes parallel sub-agent waves via FuturesUnordered, enforces depth ceilings, and routes Tadpole Alpha directives.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Recursive loop detected, sub-agent timeout, or upstream DAG wave failure.
//! - **Telemetry Link**: Search `[executor]` in tracing logs.

use super::planner::parse_subtask_specs;
use super::sanitize::{
    escape_xml_text, sanitize_subagent_text, should_skip_synthesis, truncate_utf8,
};
use super::synthesis::format_pooled_outcomes;
use super::{
    SubAgentOptions, SubAgentOutcome, DEFAULT_ALPHA_TIMEOUT_SECS, MAX_UPSTREAM_CONTEXT_CHARS,
    MAX_UPSTREAM_SNIPPET_CHARS,
};
use crate::agent::runner::tools::error::ToolExecutionError;
use crate::agent::runner::{AgentRunner, RunContext};
use crate::agent::types::{TokenUsage, ToolCall};
use crate::error::AppError;
use futures::stream::{FuturesUnordered, StreamExt};
use std::collections::HashMap;

impl AgentRunner {
    // ─────────────────────────────────────────────────────────
    //  SWARM MANAGEMENT
    // ─────────────────────────────────────────────────────────

    /// ### 🐝 Orchestration: Recruitment Cycle (spawn_subagent)
    /// Handles the `spawn_subagent` tool call: ensures specialized sub-agents are 
    /// either found or fabricated, recurses the execution, and synthesizes results.
    /// 
    /// ### 🧬 Logic: Swarm Parallelism (OML-01)
    /// 1. **Batch Detection**: Supports spawning either a single `agent_id` or 
    ///    an array of `agent_ids` in parallel.
    /// 2. **Lineage Protection**: Enforces the **Recursive Recruitment Guard** 
    ///    (SEC-01) by checking if the target agent is already in the `traceparent` 
    ///    ancestry. This prevents stack-overflow loops in the swarm.
    /// 3. **Neural Handoff**: Injects the `primary_goal` into the sub-task's 
    ///    starting context to ensure goal-alignment across the entire swarm tree.
    /// 4. **Synthesis**: Collects results from all sub-agents and uses an LLM 
    ///    synthesis pass to summarize the findings for the parent agent.
    pub(crate) fn handle_spawn_subagent<'a>(
        &'a self,
        ctx: &'a RunContext,
        fc: &'a ToolCall,
        usage: &'a mut Option<TokenUsage>,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<String, ToolExecutionError>> + Send + 'a>,
    > {
        Box::pin(async move {
        let mut specs = parse_subtask_specs(&fc.args)?;

        let max_swarm_depth = self
            .state
            .governance
            .max_swarm_depth
            .load(std::sync::atomic::Ordering::Relaxed);
        if ctx.depth >= max_swarm_depth {
            tracing::warn!(
                "🐝 [Swarm] Swarm depth limit exceeded (current: {}, max: {}). Blocking recruitment.",
                ctx.depth,
                max_swarm_depth
            );
            return Ok(format!(
                "PROTOCOL_VIOLATION: Swarm depth limit exceeded (current depth: {}). You cannot spawn more sub-agents.",
                ctx.depth
            ));
        }

        // Validate DAG topology and dependencies BEFORE recruiting or persisting sub-agents (H3).
        let mut graph = crate::agent::dag::TaskGraph::new();
        let mut spec_by_id = HashMap::with_capacity(specs.len());
        let mut task_order = HashMap::with_capacity(specs.len());
        for (idx, spec) in specs.iter().enumerate() {
            graph
                .add_node(
                    &spec.id,
                    &spec.id,
                    &spec.agent_id,
                    serde_json::json!({
                        "message": spec.message,
                        "depends_on": spec.depends_on,
                    }),
                )
                .map_err(|e| ToolExecutionError::ExecutionFailed(e.to_string()))?;
            spec_by_id.insert(spec.id.clone(), spec.clone());
            task_order.insert(spec.id.clone(), idx);
        }
        for spec in &specs {
            for dep in &spec.depends_on {
                graph.add_dependency(dep, &spec.id).map_err(|e| {
                    ToolExecutionError::ExecutionFailed(format!("PROTOCOL_VIOLATION: {}", e))
                })?;
            }
        }

        ctx.metrics.record_sub_agent();

        let requested_summary: Vec<&str> = specs.iter().map(|s| s.agent_id.as_str()).collect();
        tracing::info!(
            "🐝 [Swarm] Agent {} spawning {} sub-task(s): {:?}...",
            ctx.agent_id,
            specs.len(),
            requested_summary
        );

        let extra_skills = fc
            .args
            .get("skills")
            .and_then(|v| v.as_array())
            .map(|arr| arr.as_slice());
        let extra_workflows = fc
            .args
            .get("workflows")
            .and_then(|v| v.as_array())
            .map(|arr| arr.as_slice());
        let role_override = fc.args.get("role").and_then(|v| v.as_str());

        // Each specialist transition commits before registry publication. Keeping an
        // outer SQL transaction while waiting for lifecycle locks would invert lock order.
        for spec in &mut specs {
            if ctx.lineage.contains(&spec.agent_id) || ctx.agent_id == spec.agent_id {
                continue;
            }
            spec.agent_id = self
                .ensure_sub_agent_exists(SubAgentOptions {
                    agent_id: &spec.agent_id,
                    parent_config: &ctx.model_config,
                    extra_skills,
                    extra_workflows,
                    role_override,
                    caller_ctx: Some(ctx),
                })
                .await?;
            if let Some(entry) = spec_by_id.get_mut(&spec.id) {
                entry.agent_id = spec.agent_id.clone();
            }
        }

        let resolved_summary: Vec<String> = specs
            .iter()
            .map(|s| format!("{}:{}", s.id, s.agent_id))
            .collect();
        tracing::info!(
            "🐝 [Swarm] Resolved sub-task targets for {}: {:?}",
            ctx.agent_id,
            resolved_summary
        );

        let arbiter = self.state.resources.continuity_arbiter.clone();
        for spec in &specs {
            let id = spec.agent_id.clone();
            let arb_clone = arbiter.clone();
            let warm_span = tracing::info_span!("swarm::warm_up", agent_id = %id);
            tokio::spawn(tracing::Instrument::instrument(
                async move {
                    arb_clone.warm_up(&id).await;
                },
                warm_span,
            ));
        }

        let mut outcomes: Vec<SubAgentOutcome> = Vec::new();
        let mut completed_outputs: HashMap<String, String> = HashMap::new();

        let mut wave_guard = crate::agent::swarm_orchestrator::SpawnedChildGuard::new();
        while !graph.is_finished() {
            if ctx.cancel_handle.is_cancelled() {
                wave_guard.abort_all();
                return Err(ToolExecutionError::ExecutionFailed(
                    "Mission cancelled by parent orchestrator".to_string(),
                ));
            }
            let ready_nodes = graph.get_ready_nodes();
            if ready_nodes.is_empty() {
                if graph.is_finished() {
                    break;
                }
                tracing::warn!(
                    "⚠️ [Swarm] TaskGraph stalled with non-terminal nodes; returning collected outcomes."
                );
                break;
            }

            let mut wave_tasks = FuturesUnordered::new();
            for node in ready_nodes {
                let Some(spec) = spec_by_id.get(&node.id).cloned() else {
                    let err_msg = format!("Missing subtask spec for DAG node '{}'", node.id);
                    if let Err(e) = graph.mark_failed(&node.id, &err_msg) {
                        tracing::debug!("[swarm] mark_failed failed for '{}': {}", node.id, e);
                    }
                    outcomes.push(SubAgentOutcome::Failed {
                        task_id: node.id,
                        agent_id: node.assigned_agent_id.clone(),
                        error: err_msg,
                    });
                    continue;
                };

                // 🛡️ [Harden Phase 4: Proactive Lineage Guard]
                if ctx.lineage.contains(&spec.agent_id) || ctx.agent_id == spec.agent_id {
                    let reason = format!(
                        "CIRCULAR_RECRUITMENT - '{}' is already in your recruitment lineage. Parallel cycles are prohibited to prevent infinite loops (SEC-01).",
                        spec.agent_id
                    );
                    tracing::warn!(
                        "🛡️ [Swarm] Recursion block triggered for {} vs {:?}",
                        spec.agent_id,
                        ctx.lineage
                    );
                    if let Err(e) = graph.mark_failed(&spec.id, &reason) {
                        tracing::debug!("[swarm] mark_failed failed for '{}': {}", spec.id, e);
                    }
                    outcomes.push(SubAgentOutcome::ProtocolBlocked {
                        task_id: spec.id,
                        agent_id: spec.agent_id,
                        reason,
                    });
                    continue;
                }

                if let Err(e) = graph.mark_running(&spec.id) {
                    tracing::debug!("[swarm] mark_running failed for '{}': {}", spec.id, e);
                }

                let upstream_context = if spec.depends_on.is_empty() {
                    String::new()
                } else {
                    let mut sections = Vec::new();
                    let mut total_bytes = 0usize;
                    for dep_id in &spec.depends_on {
                        if let Some(out) = completed_outputs.get(dep_id) {
                            let sanitized = sanitize_subagent_text(out);
                            let bounded = truncate_utf8(&sanitized, MAX_UPSTREAM_SNIPPET_CHARS);
                            let section = format!(
                                "[Upstream Task '{}']\n{}",
                                escape_xml_text(dep_id),
                                bounded
                            );
                            if total_bytes.saturating_add(section.len()) > MAX_UPSTREAM_CONTEXT_CHARS
                            {
                                let remaining =
                                    MAX_UPSTREAM_CONTEXT_CHARS.saturating_sub(total_bytes);
                                if remaining > 32 {
                                    sections.push(format!(
                                        "{}\n... [truncated]",
                                        truncate_utf8(&section, remaining.saturating_sub(16))
                                    ));
                                }
                                break;
                            }
                            total_bytes = total_bytes.saturating_add(section.len());
                            sections.push(section);
                        }
                    }
                    if sections.is_empty() {
                        String::new()
                    } else {
                        format!("\n\n### UPSTREAM TASK RESULTS:\n{}", sections.join("\n\n"))
                    }
                };

                let runner = self.clone();
                let ctx_clone = ctx.clone();
                let task_id_for_join = spec.id.clone();
                let agent_id_for_join = spec.agent_id.clone();
                let (started_tx, started_rx) = tokio::sync::oneshot::channel::<()>();
                let join_handle = tokio::spawn(async move {
                    // Register AbortHandle before child execution starts (Scheduler Invariant #1).
                    let _ = started_rx.await;
                    let goal_text = ctx_clone
                        .primary_goal
                        .as_deref()
                        .unwrap_or("See mission scope for details.");
                    let mut final_instruction = if spec.message.trim().len() < 10 {
                        format!(
                            "### PRIMARY MISSION GOAL:\n{}\n\n(Please assist with the mission goal listed above.)",
                            goal_text
                        )
                    } else {
                        format!(
                            "{}\n\n--- STRATEGIC CONTEXT ---\n### PRIMARY MISSION GOAL:\n{}\n--- END CONTEXT ---",
                            spec.message, goal_text
                        )
                    };
                    if !upstream_context.is_empty() {
                        final_instruction.push_str(&upstream_context);
                    }

                    let (sub_agent_name, sub_agent_role, sub_slot, budget_cap) = {
                        let sub_agent = runner.state.registry.agents.get(&spec.agent_id);
                        (
                            sub_agent
                                .as_ref()
                                .map(|a| a.identity.name.clone())
                                .unwrap_or_else(|| spec.agent_id.clone()),
                            sub_agent
                                .as_ref()
                                .map(|a| a.identity.role.clone())
                                .unwrap_or_else(|| "specialist".to_string()),
                            sub_agent
                                .as_ref()
                                .and_then(|a| a.models.active_model_slot)
                                .map_or(2, |s| s as u8),
                            sub_agent
                                .as_ref()
                                .map(|a| a.economics.budget_usd)
                                .filter(|&b| b > 0.0),
                        )
                    };
                    let is_privacy = runner
                        .state
                        .governance
                        .privacy_mode
                        .load(std::sync::atomic::Ordering::Relaxed);
                    let socratic = crate::agent::socratic::SocraticContextEnvelope::compile(
                        &spec.agent_id,
                        &sub_agent_name,
                        &sub_agent_role,
                        ctx_clone
                            .primary_goal
                            .as_deref()
                            .unwrap_or("Task Execution"),
                        None,
                        budget_cap,
                        Some(sub_slot),
                        is_privacy,
                    );
                    let injected_instruction =
                        socratic.inject_into_prompt(&final_instruction).into_owned();

                    let payload = ctx_clone.derive_subtask_payload(injected_instruction);
                    let outcome = Box::pin(runner.run(spec.agent_id.clone(), payload)).await;
                    (spec.id, spec.agent_id, outcome)
                });
                wave_guard.register(join_handle.abort_handle());
                let _ = started_tx.send(());
                wave_tasks.push(async move {
                    match join_handle.await {
                        Ok(res) => res,
                        Err(join_err) => (
                            task_id_for_join,
                            agent_id_for_join,
                            Err(AppError::InternalServerError(format!(
                                "Sub-agent task aborted: {}",
                                join_err
                            ))),
                        ),
                    }
                });
            }

            loop {
                let next_res = tokio::select! {
                    biased;
                    _ = ctx.cancel_handle.cancelled() => {
                        wave_guard.abort_all();
                        return Err(ToolExecutionError::ExecutionFailed(
                            "Mission cancelled by parent orchestrator".to_string(),
                        ));
                    }
                    item = wave_tasks.next() => item,
                };
                let Some((task_id, agent_id, outcome)) = next_res else {
                    break;
                };
                match outcome {
                    Ok(output) => {
                        completed_outputs.insert(task_id.clone(), output.clone());
                        self.state.comms.blackboard.set(
                            ctx.blackboard_scope(),
                            &format!("task:{}#output", task_id),
                            serde_json::json!(output),
                            &agent_id,
                            ["swarm", "task_output"],
                        );
                        if let Err(e) = graph.mark_completed(
                            &task_id,
                            serde_json::json!({ "output": output }),
                        ) {
                            tracing::debug!("[swarm] mark_completed failed for '{}': {}", task_id, e);
                        }
                        outcomes.push(SubAgentOutcome::Success {
                            task_id,
                            agent_id,
                            output,
                        });
                    }
                    Err(e) => {
                        let err_str = e.to_string();
                        if let Err(me) = graph.mark_failed(&task_id, &err_str) {
                            tracing::debug!("[swarm] mark_failed failed for '{}': {}", task_id, me);
                        }
                        outcomes.push(SubAgentOutcome::Failed {
                            task_id,
                            agent_id,
                            error: err_str,
                        });
                    }
                }
            }
        }

        // Capture any downstream DAG tasks automatically skipped due to upstream failure.
        for spec in &specs {
            if let Some(node) = graph.get_node(&spec.id) {
                if node.status == crate::agent::dag::TaskNodeStatus::Skipped {
                    outcomes.push(SubAgentOutcome::Skipped {
                        task_id: spec.id.clone(),
                        agent_id: spec.agent_id.clone(),
                        reason: node
                            .error_detail
                            .clone()
                            .unwrap_or_else(|| "Skipped due to upstream failure".to_string()),
                    });
                }
            }
        }

        // Deterministic ordering matching declared `specs` order.
        outcomes.sort_by_key(|o| {
            let tid = match o {
                SubAgentOutcome::Success { task_id, .. }
                | SubAgentOutcome::ProtocolBlocked { task_id, .. }
                | SubAgentOutcome::Failed { task_id, .. }
                | SubAgentOutcome::Skipped { task_id, .. } => task_id.as_str(),
            };
            task_order.get(tid).copied().unwrap_or(usize::MAX)
        });

        let pooled_results = format_pooled_outcomes(&outcomes);

        // Only shortcut synthesis when NO sub-task succeeded (checked before allocating synthesis prompt).
        if should_skip_synthesis(&outcomes) {
            tracing::warn!(
                "⚠️ [Swarm] All sub-tasks failed, were skipped, or were blocked. Skipping synthesis."
            );
            return Ok(pooled_results);
        }

        let synthesis_prompt = format!(
            "Your swarm reported back with these pooled results:\n\n{}\n\nPlease synthesize this data and provide your final response or take next steps.",
            pooled_results
        );

        // Derive child-bounded RunContext for synthesis to prevent SEC-01 depth/lineage bypass (C1).
        let mut synth_ctx = ctx.clone();
        synth_ctx.depth = ctx.depth.saturating_add(1);
        synth_ctx.lineage.push(ctx.agent_id.clone());
        let mut swarm_tools = self.build_tools(&synth_ctx).await;
        // Because any spawned child runs at `synth_ctx.depth + 1` and `lifecycle.rs` rejects
        // `depth >= max_swarm_depth`, strip swarm tools once synthesis cannot legally spawn deeper.
        if synth_ctx.depth.saturating_add(1) >= max_swarm_depth {
            swarm_tools.function_declarations.retain(|f| {
                !matches!(
                    f.name.as_str(),
                    "spawn_subagent"
                        | "issue_alpha_directive"
                        | "recruit_specialist"
                        | "recruit"
                )
            });
        }

        match self
            .call_provider_for_synthesis(&synth_ctx, &synthesis_prompt, Some(vec![swarm_tools]))
            .await
        {
            Ok((final_text, _, final_usage)) => {
                self.accumulate_usage(usage, final_usage);
                Ok(final_text)
            }
            Err(e) => {
                if ctx.cancel_handle.is_cancelled() {
                    return Err(ToolExecutionError::ExecutionFailed(
                        "Mission cancelled by parent orchestrator".to_string(),
                    ));
                }
                tracing::warn!(
                    "⚠️ [Swarm] Synthesis provider call failed ({}); falling back to pooled sub-agent outcomes.",
                    e
                );
                Ok(pooled_results)
            }
        }
        })
    }


    /// Handles `issue_alpha_directive`: delegates to Tadpole Alpha (COO / ID: 2).
    pub(crate) async fn handle_alpha_directive(
        &self,
        ctx: &RunContext,
        fc: &ToolCall,
    ) -> Result<String, ToolExecutionError> {
        let directive = fc
            .args
            .get("directive")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                ToolExecutionError::ExecutionFailed(
                    "PROTOCOL_VIOLATION: 'directive' must be a non-empty string".to_string(),
                )
            })?;

        let max_swarm_depth = self
            .state
            .governance
            .max_swarm_depth
            .load(std::sync::atomic::Ordering::Relaxed);
        if ctx.depth >= max_swarm_depth {
            return Err(ToolExecutionError::ExecutionFailed(format!(
                "PROTOCOL_VIOLATION: Swarm depth limit exceeded (current: {}, max: {})",
                ctx.depth, max_swarm_depth
            )));
        }

        // Resolve COO deterministically (exclude orphan "alpha", avoid "coordinator" false match).
        let alpha_id = if self
            .state
            .registry
            .agents
            .contains_key(crate::agent::constants::AGENT_COO)
        {
            crate::agent::constants::AGENT_COO.to_string()
        } else {
            let mut coo_candidates: Vec<String> = self
                .state
                .registry
                .agents
                .iter()
                .filter_map(|kv| {
                    let id = kv.key();
                    if id == crate::agent::constants::AGENT_ALPHA {
                        return None;
                    }
                    let role = kv.value().identity.role.trim().to_ascii_lowercase();
                    let name = kv.value().identity.name.trim().to_ascii_lowercase();
                    if id == crate::agent::constants::AGENT_COO
                        || role == "coo"
                        || role == "coo (operations director)"
                        || name.contains("tadpole alpha")
                    {
                        Some(id.clone())
                    } else {
                        None
                    }
                })
                .collect();
            coo_candidates.sort();
            coo_candidates
                .into_iter()
                .next()
                .unwrap_or_else(|| crate::agent::constants::AGENT_COO.to_string())
        };

        if ctx.lineage.contains(&alpha_id) || ctx.agent_id == alpha_id {
            return Err(ToolExecutionError::ExecutionFailed(format!(
                "PROTOCOL_VIOLATION: CIRCULAR_RECRUITMENT - '{}' is already in recruitment lineage (SEC-01)",
                alpha_id
            )));
        }

        tracing::info!("🧬 [Sovereignty] Agent of Nine issuing directive to Tadpole Alpha ({})...", alpha_id);
        self.broadcast_agent(ctx, "🧬 Issuing directive to Tadpole Alpha...", "info");
        self.broadcast_agent_stream(ctx, "🧬 Delegating directive to Tadpole Alpha...", false);

        let goal_text = ctx
            .primary_goal
            .as_deref()
            .unwrap_or("See mission scope for details.");
        let final_directive = format!(
            "{}\n\n--- STRATEGIC CONTEXT ---\n### PRIMARY MISSION GOAL:\n{}\n--- END CONTEXT ---",
            directive, goal_text
        );

        let payload = ctx.derive_subtask_payload(final_directive);

        self.ensure_sub_agent_exists(SubAgentOptions {
            agent_id: &alpha_id,
            parent_config: &ctx.model_config,
            extra_skills: None,
            extra_workflows: None,
            role_override: Some("COO (Operations Director)"),
            caller_ctx: Some(ctx),
        })
        .await?;

        let sub_timeout = std::time::Duration::from_secs(DEFAULT_ALPHA_TIMEOUT_SECS);
        let runner = self.clone();
        let (started_tx, started_rx) = tokio::sync::oneshot::channel::<()>();
        let mut sub_task = tokio::spawn(async move {
            let _ = started_rx.await;
            runner.run(alpha_id, payload).await
        });
        let mut guard = crate::agent::swarm_orchestrator::SpawnedChildGuard::new();
        guard.register(sub_task.abort_handle());
        let _ = started_tx.send(());

        let sub_result = tokio::select! {
            biased;
            _ = ctx.cancel_handle.cancelled() => {
                sub_task.abort();
                let _ = sub_task.await;
                return Err(ToolExecutionError::ExecutionFailed(
                    "Mission cancelled by parent orchestrator".to_string(),
                ));
            }
            timed = tokio::time::timeout(sub_timeout, &mut sub_task) => match timed {
                Ok(Ok(res)) => res.map_err(|e| ToolExecutionError::ExecutionFailed(e.to_string()))?,
                Ok(Err(join_err)) => {
                    tracing::error!("🚨 [Swarm] Tadpole Alpha task panicked or was cancelled: {}", join_err);
                    return Err(ToolExecutionError::ExecutionFailed(format!(
                        "Tadpole Alpha execution failed: {}",
                        join_err
                    )));
                }
                Err(_) => {
                    sub_task.abort();
                    let _ = sub_task.await;
                    tracing::error!("🚨 [Swarm] Tadpole Alpha execution timed out after {}s", DEFAULT_ALPHA_TIMEOUT_SECS);
                    return Err(ToolExecutionError::ExecutionFailed(format!(
                        "Tadpole Alpha execution timed out after {} seconds",
                        DEFAULT_ALPHA_TIMEOUT_SECS
                    )));
                }
            }
        };

        Ok(format!(
            "Directive issued to Tadpole Alpha. Mission ID: {}\n\nResult: {}",
            ctx.mission_id, sub_result
        ))
    }
}

// Metadata: [executor]

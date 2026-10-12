//! @docs ARCHITECTURE:Runner
//!
//! ### AI Assist Note
//! **Mission Lifecycle**: Manages the setup, validation, and initialization of
//! agent missions. Enforces **SEC-01 (Recursion Guard)** and **SEC-02 (Budget Gate)**
//! before any LLM tokens are consumed. Automatically self-heals by sinking
//! registry agents into the database if missing.
//!
//! ### 🔍 Debugging & Observability
//! - **Telemetry Link**: Search `[lifecycle]` in tracing logs.
//! - **Failure Path**: Circular recursion detected, max swarm depth exceeded,
//!   budget exhausted, or agent health degradation (failure rate too high).
//! - **Trace Scope**: `server-rs::agent::runner::lifecycle`
//!

use crate::agent::types::TaskPayload;
use crate::error::AppError;

use super::AgentRunner;

impl AgentRunner {
    // ─────────────────────────────────────────────────────────
    //  SETUP & VALIDATION
    // ─────────────────────────────────────────────────────────

    /// ### 🛡️ Sector Guard: setup_and_validate
    /// Performs the critical pre-flight safety and tracing initialization for a 
    /// new agent mission branch.
    /// 
    /// ### 🧬 Logic: Security & Observability
    /// 1. **OTel Trace Propagation**: Injects the `traceparent` from the 
    ///    `TaskPayload` into the current `tracing::Span`. This ensures that 
    ///    swarm-to-swarm recruitment is perfectly visible in Jaeger/Honeycomb.
    /// 2. **Injection Sanitization**: Scans the incoming message for prompt 
    ///    injection patterns (e.g., "Ignore all previous instructions") before 
    ///    passing it to the LLM core.
    /// 3. **Validation Call**: Cascades into input constraint validation.
    pub(crate) fn setup_and_validate(
        &self,
        agent_id: &str,
        payload: &TaskPayload,
    ) -> Result<(), AppError> {
        let span = tracing::Span::current();

        // Setup OTel Trace Propagation
        if let Some(tp) = &payload.traceparent {
            use opentelemetry::global;
            use tracing_opentelemetry::OpenTelemetrySpanExt;

            let mut extractor = std::collections::HashMap::new();
            extractor.insert("traceparent".to_string(), tp.clone());
            let parent_cx = global::get_text_map_propagator(|prop| prop.extract(&extractor));
            let _ = span.set_parent(parent_cx);

            // Parse trace_id directly from traceparent (00-traceid-spanid-01)
            let parts: Vec<&str> = tp.split('-').collect();
            if parts.len() >= 4 {
                span.record("trace_id", parts[1]);
            } else {
                use opentelemetry::trace::TraceContextExt;
                let cx = span.context();
                let otel_span = cx.span();
                let otel_trace_id = otel_span.span_context().trace_id().to_string();
                span.record("trace_id", &otel_trace_id);
            }
        } else {
            use opentelemetry::trace::TraceContextExt;
            use tracing_opentelemetry::OpenTelemetrySpanExt;
            let cx = span.context();
            let otel_span = cx.span();
            let otel_trace_id = otel_span.span_context().trace_id().to_string();
            span.record("trace_id", &otel_trace_id);
        }

        // Core validation logic
        self.validate_input(agent_id, payload)?;

        // Security Sanitization
        if let crate::agent::sanitizer::SanitizationResult::Alert(msg) =
            crate::agent::sanitizer::Sanitizer::scan(&payload.message)
        {
            tracing::warn!("🛡️ [Security] Blocked potential injection: {}", msg);
            let agent_name = self
                .state
                .registry
                .agents
                .get(agent_id)
                .map(|a| a.identity.name.clone())
                .unwrap_or_else(|| agent_id.to_string());
            self.state.broadcast_agent(
                &format!("🛡️ Security: {}", msg),
                "error",
                payload.cluster_id.clone(),
                agent_id,
                &agent_name,
            );
            return Err(AppError::BadRequest(format!("Security Violation: {}", msg)));
        }

        Ok(())
    }

    /// Validates input constraints before execution begins.
    pub(crate) fn validate_input(
        &self,
        agent_id: &str,
        payload: &TaskPayload,
    ) -> Result<(), AppError> {
        let max_task_length = self
            .state
            .governance
            .max_task_length
            .load(std::sync::atomic::Ordering::Relaxed);
        if payload.message.len() > max_task_length {
            return Err(AppError::BadRequest(format!(
                "Task message too long ({} bytes, max {})",
                payload.message.len(),
                max_task_length
            )));
        }

        let lineage = payload.swarm_lineage.as_deref().unwrap_or(&[]);
        let lineage_depth = lineage.len() as u32;
        let depth = std::cmp::max(payload.swarm_depth.unwrap_or(0), lineage_depth);

        // CODE-01 FIX: Use iterator instead of to_string() allocation on hot path.
        if lineage.iter().any(|id| id == agent_id) {
            let path = lineage.join(" -> ");
            return Err(AppError::BadRequest(format!("🐝 CIRCULAR RECURSION DETECTED: Agent '{}' is already overseeing your work in this mission branch (Lineage: {} -> {}). You are PROHIBITED from recruiting your own supervisors. Please resolve the task using your own tools or recruit a DIFFERENT specialized sub-agent ID that is not in your current lineage.", agent_id, path, agent_id)));
        }

        let max_swarm_depth = self
            .state
            .governance
            .max_swarm_depth
            .load(std::sync::atomic::Ordering::Relaxed);
        if depth >= max_swarm_depth {
            return Err(AppError::BadRequest(format!("🐝 Swarm depth limit exceeded (current depth: {})! To prevent infinite recursions, this agent cannot spawn more sub-agents.", depth)));
        }

        // Check max agents limit (only for new recruitment, not for existing agents)
        if !self.state.registry.agents.contains_key(agent_id) {
            let max_agents = self
                .state
                .governance
                .max_agents
                .load(std::sync::atomic::Ordering::Relaxed);
            if self.state.registry.agents.len() as u32 >= max_agents {
                return Err(AppError::BadRequest(format!(
                    "🐝 Swarm agent limit reached (max: {}). New agent recruitment denied.",
                    max_agents
                )));
            }
        }

        Ok(())
    }

    // ─────────────────────────────────────────────────────────
    //  MISSION INITIALIZATION
    // ─────────────────────────────────────────────────────────

    /// ### 🧠 Orchestration: Mission Initialization (initialize_mission_state)
    /// Atomically registers a new mission in the persistent database and 
    /// enforces the primary budget gates.
    /// 
    /// ### 🧬 Logic: The Initialization Loop
    /// 1. **Self-Healing Registry**: Guarantees that the agent node is synced 
    ///    from the in-memory registry to the SQLite database before execution.
    /// 2. **Health Check**: Implements failure-rate throttling to prevent 
    ///    "Agent Breakdown" (degraded state where failure count > 5).
    /// 3. **Budget Gate (SEC-02)**: Enforces the enterprise budget limit 
    ///    via the `BudgetGuard`. This prevents runaway costs from recursive 
    ///    swarm recruitment.
    /// 4. **Mission Creation**: Generates the high-fidelity `Mission` record 
    ///    with an automated summary title derived from the payload message.
    pub(super) async fn initialize_mission_state(
        &self, agent_id: &str, payload: &TaskPayload, execution_id: Option<tokio::task::Id>,
    ) -> Result<crate::agent::types::Mission, AppError> {
        let _lifecycle = self.state.registry.lock_agent(agent_id).await;
        let mut agent = self.state.registry.agents.get(agent_id)
            .ok_or_else(|| AppError::NotFound(format!("Agent '{}' not found", agent_id)))?.clone();
        if agent.health.status == "suspended" {
            return Err(AppError::Forbidden("Agent is suspended".to_string()));
        }
        if self.state.comms.active_runners.get(agent_id)
            .is_some_and(|handle| Some(handle.task_id) != execution_id) {
            return Err(AppError::Conflict("Agent already has a tracked runner".to_string()));
        }
        if let Some(mission_id) = agent.state.active_mission.as_ref()
            .and_then(|mission| mission.get("id")).and_then(|id| id.as_str()) {
            let previous = self.state.comms.mission_contexts.get(mission_id).map(|ctx| ctx.clone())
                .ok_or_else(|| AppError::Conflict("Agent already has an active mission".to_string()))?;
            if previous.accounting.task_id == execution_id {
                return Err(AppError::Conflict("Execution already owns an initialized mission".to_string()));
            }
            // Drop marks the old execution stopped without taking this lock.
            // Settle its received usage before assigning a replacement mission.
            previous.accounting.wait_finished().await?;
            self.persist_terminal_state_locked(&previous, &previous.accounting.usage(), true, "Mission preempted by incoming dispatch").await?;
            agent = self.state.registry.agents.get(agent_id)
                .ok_or_else(|| AppError::NotFound(format!("Agent '{}' was deleted", agent_id)))?.clone();
        }
        // Support registry-only startup seeds explicitly, never via an update upsert.
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM agents WHERE id = ?)")
            .bind(agent_id).fetch_one(&self.state.resources.pool).await?;
        if !exists {
            agent.version = crate::agent::persistence::create_agent_db(&self.state.resources.pool, &agent).await?;
            self.state.registry.agents.insert(agent_id.to_string(), agent.clone());
        }

        let depth = std::cmp::max(
            payload.swarm_depth.unwrap_or(0),
            payload.swarm_lineage.as_ref().map(|l| l.len() as u32).unwrap_or(0),
        );
        // Record telemetry only — never mutate the enforcement limit
        self.state
            .governance
            .observed_max_depth
            .fetch_max(depth, std::sync::atomic::Ordering::Relaxed);

        let mission_title = payload.message.chars().take(50).collect::<String>() + "...";

        // 🏥 [Health Check] Failure Rate Throttling
        if let Some(agent) = self.state.registry.agents.get(agent_id) {
            if agent.value().health.failure_count >= 5 {
                if let Some(last_fail) = agent.value().health.last_failure_at {
                    let cooldown = chrono::Duration::minutes(15);
                    if chrono::Utc::now() - last_fail < cooldown {
                        tracing::warn!(
                            "🏥 [Health] Agent {} is degraded (Failure Count: {})",
                            agent_id,
                            agent.value().health.failure_count
                        );
                        let agent_name = agent.value().identity.name.clone();
                        self.state.broadcast_agent(
                            &format!(
                                "🏥 Health: in self-heal cooldown (Failure Count: {}).",
                                agent.value().health.failure_count
                            ),
                            "warning",
                            payload.cluster_id.clone(),
                            agent_id,
                            &agent_name,
                        );
                        return Err(AppError::BadRequest("Agent Degraded. Self-heal cooldown active.".to_string()));
                    }
                }
            }
        }

        // [Budget Gate] Persistent Budget Check (OpenFang Pattern)
        let has_budget = self
            .state
            .security
            .budget_guard
            .check_budget(agent_id, 0.0)
            .await?;
        if !has_budget {
            tracing::warn!(
                "🛡️ [Governance] Agent {} persistent budget exhausted",
                agent_id
            );
            let agent_name = self
                .state
                .registry
                .agents
                .get(agent_id)
                .map(|a| a.identity.name.clone())
                .unwrap_or_else(|| agent_id.to_string());
            self.state.broadcast_agent(
                "🛡️ Budget Guard: has exceeded its periodic quota.",
                "error",
                payload.cluster_id.clone(),
                agent_id,
                &agent_name,
            );
            return Err(AppError::Forbidden("Persistent Quota Exhausted. Check Security Dashboard.".to_string()));
        }

        // Keep legacy agent-local budget for backward compatibility / double gating
        let mut agent_budget = 0.0;
        let mut agent_cost = 0.0;
        if let Some(agent) = self.state.registry.agents.get(agent_id) {
            agent_budget = agent.value().economics.budget_usd;
            agent_cost = agent.value().economics.cost_usd;
        }

        if agent_budget > 0.0 && agent_cost >= agent_budget {
            tracing::warn!(
                "⚠️ [Governance] Agent {} local budget exhausted (${:.4}/${:.4})",
                agent_id,
                agent_cost,
                agent_budget
            );
            return Err(AppError::Forbidden(format!("Local Agent Budget Exhausted (${:.4}/${:.4})", agent_cost, agent_budget)));
        }

        let mission_budget = payload.budget_usd.unwrap_or_else(|| {
            if agent_budget > 0.0 {
                (agent_budget - agent_cost).max(0.001) // Ensure at least some budget if not explicitly over
            } else {
                1.0 // Default fallback
            }
        });

        // Acquire write ownership before identity/version reads; deferred
        // read-to-write upgrades can deadlock simultaneous initializations.
        let mut tx = self.state.resources.pool.begin_with("BEGIN IMMEDIATE").await?;
        let mut mission = crate::agent::mission::create_mission_in_tx(
            &mut tx,
            agent_id,
            &mission_title,
            mission_budget,
        )
        .await?;

        sqlx::query("UPDATE mission_history SET status = 'active' WHERE id = ?")
            .bind(&mission.id).execute(&mut *tx).await?;
        mission.status = crate::agent::types::MissionStatus::Active;

        agent.state.active_mission = Some(serde_json::json!({
            "id": mission.id, "title": mission.title
        }));
        agent.health.status = "busy".to_string();
        agent.health.heartbeat_at = Some(chrono::Utc::now());
        agent.version = crate::agent::persistence::save_agent_db_in_tx(&mut tx, &agent).await?;
        tx.commit().await?;
        self.state.registry.agents.insert(agent_id.to_string(), agent.clone());
        let mut recovery_ctx = agent.resolve_provider_context(self.state.base_dir.clone());
        recovery_ctx.mission_id = mission.id.clone();
        let task_id = execution_id;
        let job_run_id = self.state.comms.active_runners.get(agent_id)
            .filter(|handle| Some(handle.task_id) == task_id)
            .and_then(|handle| handle.job_run_id.clone());
        recovery_ctx.accounting = std::sync::Arc::new(super::accounting::MissionAccounting::for_execution(task_id, job_run_id));
        self.state.comms.mission_contexts.insert(mission.id.clone(), recovery_ctx);
        self.state.emit_event(serde_json::json!({
            "type": "agent:update", "agent_id": agent_id, "data": agent
        }));

        Ok(mission)
    }
}





#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use crate::agent::types::TaskPayload;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_validate_input_recursion() {
        let state = Arc::new(AppState::new_minimal_mock().await);
        let runner = AgentRunner::new(state);
        
        let agent_id = "agent-A";
        let payload = TaskPayload {
            message: "Hello".to_string(),
            swarm_lineage: Some(vec!["agent-B".to_string(), "agent-A".to_string()]),
            ..Default::default()
        };
        
        let result = runner.validate_input(agent_id, &payload);
        assert!(result.is_err());
        assert!(format!("{}", result.unwrap_err()).contains("CIRCULAR RECURSION"));
    }

    #[tokio::test]
    async fn test_validate_input_depth() {
        let state = Arc::new(AppState::new_minimal_mock().await);
        let runner = AgentRunner::new(state);
        
        let payload = TaskPayload {
            message: "Hello".to_string(),
            swarm_depth: Some(10), // Default max is usually 5 in mock
            ..Default::default()
        };
        
        let result = runner.validate_input("some-agent", &payload);
        assert!(result.is_err());
        assert!(format!("{}", result.unwrap_err()).contains("depth limit exceeded"));
    }

    #[tokio::test]
    async fn test_validate_input_length() {
        let state = Arc::new(AppState::new_minimal_mock().await);
        let runner = AgentRunner::new(state);
        
        let payload = TaskPayload {
            message: "A".repeat(100_000), // Default max is usually smaller
            ..Default::default()
        };
        
        let result = runner.validate_input("some-agent", &payload);
        assert!(result.is_err());
        assert!(format!("{}", result.unwrap_err()).contains("message too long"));
    }
}

// Metadata: [lifecycle]

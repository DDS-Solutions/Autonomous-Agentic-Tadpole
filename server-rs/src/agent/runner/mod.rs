//! @docs ARCHITECTURE:Runner
//!
//! ### AI Assist Note
//! **The Heartbeat**: This is the core `run()` loop. Orchestrates the
//! full mission lifecycle: **Setup -> Initialization -> ContextResolution
//! -> IntelligenceLoop -> Finalization**. Manages the `RunContext`
//! (state bag) and hierarchical OTel tracing for real-time "God View"
//! visualization. Supports **Deterministic Workflows** and **Recursive Swarm**
//! recruitment.
//!
//! ### 🔍 Debugging & Observability
//! - **Telemetry Link**: Search `[mod]` in tracing logs.
//! - **Failure Path**: Budget exhaustion, recursion depth limit (SEC-01),
//!   prompt injection detection, or API provider timeouts.
//! - **Trace Scope**: `server-rs::agent::runner` (Check for `AgentExecution` span)
//! - **Telemetry**: Emits `agent:status` and `agent:message` events to
//!   the global telemetry bus.

use crate::agent::backlog::MissionBacklog;
use crate::agent::types::{ModelConfig, TaskPayload, RoleAuthorityLevel};
use crate::error::AppError;
use crate::state::AppState;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

// ─────────────────────────────────────────────────────────
//  SUBMODULES
// ─────────────────────────────────────────────────────────
mod analysis;
pub mod accounting;
mod context;
mod evolution_tools;
mod external_tools;
mod finalize;
mod terminal;
mod fs_tools;
mod intelligence;
mod lifecycle;
mod metrics_tools;
mod mission_tools;
mod oversight;
pub(crate) mod parser;
pub(crate) mod prompt_renderer;
mod provider;
mod refinement;
pub(crate) mod service_traits;
mod swarm;
mod swarm_persistence;
pub mod synthesis;
mod visual_tools;
pub mod tools;
pub mod error;
pub(crate) mod execution_metrics;
pub(crate) mod turn_compactor;
pub(crate) mod loop_detector;
mod monologue;
mod workflow;

// ─────────────────────────────────────────────────────────
//  CORE TYPES
// ─────────────────────────────────────────────────────────

/// Context bag for data resolved during the setup phase of a run.
#[derive(Clone)]
pub struct RunContext {
    /// Server-owned receipt; never deserialized from a task request.
    pub accounting: Arc<accounting::MissionAccounting>,
    pub agent_id: String,
    pub name: String,
    pub role: String,
    pub department: String,
    pub description: String,
    pub model_config: ModelConfig,
    pub skills: Vec<String>,
    pub workflows: Vec<String>,
    pub mission_id: String,
    pub user_id: Option<String>,
    pub depth: u32,
    pub lineage: Vec<String>,
    pub provider_name: String,
    pub workspace_root: std::path::PathBuf,
    pub fs_adapter: crate::adapter::filesystem::FilesystemAdapter,
    pub safe_mode: bool,
    pub analysis: bool,
    pub traceparent: Option<String>,
    pub trace_id: String,
    pub last_accessed_files: std::sync::Arc<parking_lot::Mutex<Vec<String>>>,
    pub recent_findings: Option<String>,
    pub working_memory: serde_json::Value,
    pub summarized_history: Option<String>,
    pub structured_output: bool,
    pub backlog: Option<Arc<parking_lot::Mutex<MissionBacklog>>>,
    pub primary_goal: Option<String>,
    pub budget_usd: f64,
    pub budget_limit_usd: f64,
    pub current_cost_usd: f64,
    pub reasoning_depth: u32,
    pub act_threshold: f32,
    pub max_turns: u32,
    pub authority_level: RoleAuthorityLevel,
    pub resource_weights: std::collections::HashMap<String, f32>,
    pub security_policy: serde_json::Value,
    pub execution_plane: crate::agent::types::ExecutionPlane,
    pub active_node_id: std::sync::Arc<parking_lot::Mutex<Option<String>>>,
    pub conflict: std::sync::Arc<crate::security::conflict::ConflictManager>,
    pub metrics: std::sync::Arc<crate::agent::runner::execution_metrics::ExecutionMetrics>,
}

impl Default for RunContext {
    fn default() -> Self {
        Self {
            accounting: Arc::new(accounting::MissionAccounting::default()),
            agent_id: "default-agent".to_string(),
            name: "Default".to_string(),
            role: "Specialist".to_string(),
            department: "Standard".to_string(),
            description: "Default test context".to_string(),
            model_config: ModelConfig::default(),
            skills: vec![],
            workflows: vec![],
            mission_id: "default-mission".to_string(),
            user_id: None,
            depth: 0,
            lineage: vec![],
            provider_name: "mock".to_string(),
            workspace_root: std::path::PathBuf::from("."),
            fs_adapter: crate::adapter::filesystem::FilesystemAdapter::new(
                std::path::PathBuf::from("."),
            ),
            safe_mode: false,
            analysis: false,
            traceparent: None,
            trace_id: "default-trace".to_string(),
            last_accessed_files: std::sync::Arc::new(parking_lot::Mutex::new(Vec::new())),
            recent_findings: None,
            working_memory: serde_json::json!({}),
            summarized_history: None,
            structured_output: false,
            backlog: None,
            primary_goal: None,
            budget_usd: 0.0,
            budget_limit_usd: 10.0,
            current_cost_usd: 0.0,
            reasoning_depth: 1,
            act_threshold: 0.9,
            max_turns: 20,
            authority_level: RoleAuthorityLevel::Specialist,
            resource_weights: std::collections::HashMap::new(),
            security_policy: serde_json::json!({}),
            execution_plane: crate::agent::types::ExecutionPlane::default(),
            active_node_id: std::sync::Arc::new(parking_lot::Mutex::new(None)),
            conflict: std::sync::Arc::new(crate::security::conflict::ConflictManager::new()),
            metrics: std::sync::Arc::new(crate::agent::runner::execution_metrics::ExecutionMetrics::new()),
        }
    }
}

impl RunContext {

    pub fn derive_subtask_payload(&self, message: String) -> TaskPayload {
        TaskPayload {
            message,
            cluster_id: Some(self.mission_id.clone()),
            provider: Some(self.model_config.provider),
            model_id: if self.model_config.model_id.trim().is_empty() {
                Some("gemma4:e4b".to_string())
            } else {
                Some(self.model_config.model_id.clone())
            },
            api_key: self.model_config.api_key.clone(),
            base_url: self.model_config.base_url.clone(),
            rpm: self.model_config.rpm,
            tpm: self.model_config.tpm,
            rpd: self.model_config.rpd,
            tpd: self.model_config.tpd,
            context_window: self.model_config.context_window,
            input_cost_per_m: self.model_config.input_cost_per_m,
            output_cost_per_m: self.model_config.output_cost_per_m,
            swarm_depth: Some(self.depth + 1),
            swarm_lineage: Some({
                let mut l = self.lineage.clone();
                l.push(self.agent_id.clone());
                l
            }),
            external_id: self.model_config.external_id.clone(),
            safe_mode: Some(self.safe_mode),
            traceparent: self.traceparent.clone(),
            user_id: self.user_id.clone(),
            context_files: Some(self.last_accessed_files.lock().clone()),
            recent_findings: self.recent_findings.clone(),
            structured_output: Some(false),
            primary_goal: self.primary_goal.clone(),
            execution_plane: Some(self.execution_plane),
            ..Default::default()
        }
    }

    /// Creates a partial RunContext from an isolated ToolContext.
    /// Used for bridging between Zero-Trust tools and legacy AgentRunner handlers.
    pub fn from_tool_ctx(ctx: &crate::agent::runner::tools::ToolContext) -> Self {
        let agent = ctx
            .state
            .registry
            .agents
            .get(&ctx.agent_id)
            .map(|a| a.value().clone());

        let model_config = agent
            .as_ref()
            .map(|a| a.models.model.clone())
            .unwrap_or_default();

        let (name, role, department, authority_level) = if let Some(ref a) = agent {
            (
                a.identity.name.clone(),
                a.identity.role.clone(),
                a.identity.department.clone(),
                crate::agent::types::RoleAuthorityLevel::from_role(&a.identity.role),
            )
        } else {
            (
                ctx.agent_id.clone(),
                "Specialist".to_string(),
                "Operations".to_string(),
                crate::agent::types::RoleAuthorityLevel::Specialist,
            )
        };

        Self {
            agent_id: ctx.agent_id.clone(),
            name,
            role,
            department: department.clone(),
            authority_level,
            mission_id: ctx.mission_id.clone(),
            depth: ctx.depth,
            lineage: ctx.lineage.clone(),
            safe_mode: ctx.safe_mode,
            primary_goal: ctx.primary_goal.clone(),
            last_accessed_files: ctx.last_accessed_files.clone(),
            metrics: ctx.metrics.clone(),
            accounting: ctx.accounting.clone(),
            workspace_root: ctx.workspace_root.clone(),
            fs_adapter: ctx.fs_adapter.clone(),
            budget_usd: ctx.budget_usd,
            budget_limit_usd: ctx.budget_limit_usd,
            security_policy: ctx.security_policy.clone(),
            execution_plane: if department.eq_ignore_ascii_case("smb")
                || department.eq_ignore_ascii_case("business")
                || department.eq_ignore_ascii_case("digital-twin")
            {
                crate::agent::types::ExecutionPlane::SmbDigitalTwin
            } else {
                crate::agent::types::ExecutionPlane::EngineMaintenance
            },
            trace_id: ctx.trace_id.clone(),
            traceparent: Some(ctx.trace_id.clone()),
            model_config,
            active_node_id: std::sync::Arc::new(parking_lot::Mutex::new(ctx.active_node_id.clone())),
            ..Default::default()
        }
    }

    #[allow(dead_code)]
    pub fn resolve_paths(&self) -> (String, String, String) {
        let cluster_name = self
            .workspace_root
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let agent_memory_dir = format!("data/agents/{}/memory.lance", self.agent_id);
        let mission_scope_dir = format!(
            "data/workspaces/{}/missions/{}/scope.lance",
            cluster_name, self.mission_id
        );

        (cluster_name, agent_memory_dir, mission_scope_dir)
    }
}

#[derive(Clone)]
pub struct AgentRunner {
    pub state: Arc<AppState>,
}

pub(crate) struct IntelligenceOutput {
    pub text: String,
    pub usage: Option<crate::agent::types::TokenUsage>,
}

struct ActiveAgentGuard<'a> {
    counter: &'a AtomicU32,
}

impl<'a> ActiveAgentGuard<'a> {
    fn acquire(counter: &'a AtomicU32) -> Self {
        counter.fetch_add(1, Ordering::SeqCst);
        Self { counter }
    }
}

impl Drop for ActiveAgentGuard<'_> {
    fn drop(&mut self) {
        self.counter.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Guard to ensure mission cleanup on abrupt task cancellation (Drop).
struct MissionCleanupGuard {
    runner: AgentRunner,
    ctx: Option<RunContext>,
    usage: Option<crate::agent::types::TokenUsage>,
    completed: bool,
}

impl Drop for MissionCleanupGuard {
    fn drop(&mut self) {
        if let Some(ctx) = &self.ctx { ctx.accounting.finish(); }
        if !self.completed {
            if let Some(ref ctx) = self.ctx {
                let runner = self.runner.clone();
                let ctx = runner.state.comms.mission_contexts.get(&ctx.mission_id)
                    .map(|entry| entry.clone()).unwrap_or_else(|| ctx.clone());
                let usage = ctx.accounting.usage().or_else(|| self.usage.clone());
                if let Ok(handle) = tokio::runtime::Handle::try_current() {
                    handle.spawn(async move {
                        if let Err(error) = runner.persist_terminal_state(&ctx, &usage, true, "Mission aborted by kernel/user").await {
                            if !matches!(error, AppError::Conflict(_) | AppError::NotFound(_)) {
                                tracing::error!(%error, mission_id = %ctx.mission_id, "[Runner] Cancellation recovery failed");
                            }
                        }
                    });
                } else {
                    tracing::warn!(
                        "MissionCleanupGuard dropped outside active Tokio runtime; aborting async fail for mission {}",
                        ctx.mission_id
                    );
                }
            }
        }
    }
}

impl AgentRunner {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }

    /// Emits a diagnostic event to the global system terminal.
    pub(crate) fn broadcast_sys(&self, msg: &str, level: &str, mission_id: Option<String>) {
        self.state.broadcast_sys(msg, level, mission_id);
    }

    /// Emits an agent-specific personality event to the dashboard.
    /// Broadcasts partial agent text for real-time UI rendering (Streamdown).
    pub(crate) fn broadcast_agent_stream(&self, ctx: &RunContext, text: &str, is_final: bool) {
        let payload = serde_json::json!({
            "type": "agent_stream",
            "agent_id": ctx.agent_id,
            "mission_id": ctx.mission_id,
            "trace_id": ctx.trace_id,
            "text": text,
            "is_final": is_final,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });
        let _ = self.state.comms.event_tx.send(payload);
    }

    /// Emits an agent-specific personality event to the dashboard.
    pub(crate) fn broadcast_agent(&self, ctx: &RunContext, msg: &str, level: &str) {
        self.state.broadcast_agent(
            msg,
            level,
            Some(ctx.mission_id.clone()),
            &ctx.agent_id,
            &ctx.name,
        );
    }

    /// Appends a node to the multiversal mission journal via the MemoryActor.
    pub(crate) async fn append_to_journal(
        &self,
        ctx: &RunContext,
        role: &str,
        content: &str,
        parent_id: Option<String>,
        metadata: Option<serde_json::Value>,
    ) -> Result<String, AppError> {
        let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();
        let msg = crate::system::actors::SystemMessage::MemoryAppend {
            mission_id: ctx.mission_id.clone(),
            parent_id,
            role: role.to_string(),
            content: content.to_string(),
            metadata,
            resp: resp_tx,
        };

        if let Some(actors) = self.state.actors.get() {
            crate::system::actors::dispatch(&actors.memory, msg, resp_rx).await
        } else {
            Err(AppError::InternalServerError("MemoryActor not initialized".to_string()))
        }
    }

    /// Updates the heartbeat timestamp in the registry and persistence layer.
    /// Used by the `reap_stale_agents` safety valve to detect hung missions.
    pub(crate) async fn record_heartbeat(&self, agent_id: &str) {
        let now = chrono::Utc::now();
        let should_persist = if let Some(mut entry) = self.state.registry.agents.get_mut(agent_id) {
            let last = entry.health.heartbeat_at;
            entry.value_mut().health.heartbeat_at = Some(now);
            last.map(|t| (now - t).num_seconds() >= 10).unwrap_or(true)
        } else {
            true
        };

        if should_persist {
            if let Err(e) = crate::agent::persistence::update_agent_heartbeat(&self.state.resources.pool, agent_id).await {
                tracing::warn!("⚠️ [Runner] Failed to persist heartbeat for agent {}: {}", agent_id, e);
            }
        }
    }

    /// ### 🔄 Processing Pipeline: The Intelligence Heartbeat
    /// Orchestrates the full autonomous mission lifecycle for an agent identity.
    ///
    /// ### 🧬 Mission Phases
    /// 1. **Setup**: Resolves the static `RunContext` (agent name, role, department) 
    ///    and validates the incoming `TaskPayload` for circular recursion.
    /// 2. **Initialization**: Creates a persistent record in the `mission_history` 
    ///    table and clears the agent's short-term working memory.
    /// 3. **ContextResolution**: Loads the workspace path, canonicalizes the 
    ///    filesystem adapter, and injects semantic history (if enabled). Checks 
    ///    for active **Deterministic Workflows** (SOPs).
    /// 4. **IntelligenceLoop**: The primary cognitive cycle. Alternates between 
    ///    LLM inference (Reasoning) and Tool Execution (Interaction) until 
    ///    the goal is met or the budget/recursion limit is hit.
    /// 5. **Finalization**: Records total mission cost, logs the final completion 
    ///    text, and releases the `ActiveAgentGuard`.
    #[tracing::instrument(
        name = "AgentExecution",
        skip(self, payload),
        fields(
            agent_id = %agent_id,
            mission_id = %payload.cluster_id.as_deref().unwrap_or("unknown"),
            status = "running",
            swarm_depth = payload.swarm_depth.unwrap_or(0),
            trace_id = tracing::field::Empty
        )
    )]
    pub async fn run(&self, agent_id: String, mut payload: TaskPayload) -> Result<String, AppError> {
        if payload.primary_goal.is_none() {
            payload.primary_goal = Some(payload.message.clone());
        }

        let _mission_id = payload
            .cluster_id
            .clone()
            .unwrap_or_else(|| "unknown".to_string());
        self.state.yield_phase_transition(&agent_id, "Setup").await;
        self.setup_and_validate(&agent_id, &payload)?;

        self.state
            .yield_phase_transition(&agent_id, "Initialization")
            .await;
        let _active_agent_guard = ActiveAgentGuard::acquire(&self.state.governance.active_agents);
        // Initialization owns its commit/publication boundary. If this caller
        // stops, the unclaimed result drops its mission guard and recovers it.
        let initializer = self.clone();
        let initializing_agent = agent_id.clone();
        let initializing_payload = payload.clone();
        let execution_id = tokio::task::try_id();
        let (mission, mut cleanup_guard) = tokio::spawn(async move {
            let mission = initializer.initialize_mission_state(
                &initializing_agent, &initializing_payload, execution_id,
            ).await?;
            let ctx = initializer.state.comms.mission_contexts.get(&mission.id)
                .map(|ctx| ctx.clone()).ok_or_else(|| AppError::Conflict("Initialized mission was removed before ownership transfer".to_string()))?;
            let guard = MissionCleanupGuard { runner: initializer, ctx: Some(ctx), usage: None, completed: false };
            Ok::<_, AppError>((mission, guard))
        }).await.map_err(|error| AppError::InternalServerError(format!("Mission initialization task failed: {error}")))??;
        let mission_id = mission.id.clone();

        let result = self.run_initialized_mission(&agent_id, payload, &mission_id, &mut cleanup_guard).await;
        if let Err(error) = &result {
            let ctx = cleanup_guard.ctx.as_ref().expect("Initialized mission owns a cleanup context");
            let usage = ctx.accounting.usage().or_else(|| cleanup_guard.usage.clone());
            match self.fail_mission(ctx, error, &usage).await {
                Ok(()) | Err(AppError::Conflict(_)) | Err(AppError::NotFound(_)) => cleanup_guard.completed = true,
                Err(persistence_error) => {
                    tracing::error!(%persistence_error, mission_id = %mission_id, "[Runner] Mission failure could not be persisted");
                    return Err(persistence_error);
                }
            }
        } else {
            cleanup_guard.completed = true;
        }
        result
    }

    async fn run_initialized_mission(
        &self, agent_id: &str, mut payload: TaskPayload, mission_id: &str,
        cleanup_guard: &mut MissionCleanupGuard,
    ) -> Result<String, AppError> {
        // Every fallible step below is covered by the initialized mission guard.
        if self.state.actors.get().is_some() {
            let ctx = cleanup_guard.ctx.as_ref().expect("Initialized cleanup context");
            self.append_to_journal(ctx, "user", &payload.message, None, None).await?;
        }

        self.state
            .yield_phase_transition(&agent_id, "ContextResolution")
            .await;
        let agent_data = self
            .state
            .registry
            .agents
            .get(agent_id)
            .map(|a| a.value().clone())
            .ok_or_else(|| AppError::NotFound(format!("Agent {} not found", agent_id)))?;

        // Phase 2: Resume blocked first check (swapping payload message if a blocked task exists)
        if agent_data.runner_policy.resume_blocked_first {
            use sqlx::Row;
            let blocked_row = sqlx::query(
                "SELECT id, title, description FROM agent_tasks WHERE agent_id = ?1 AND status = 'needs_input' ORDER BY updated_at ASC LIMIT 1"
            )
            .bind(&agent_id)
            .fetch_optional(&self.state.resources.pool)
            .await
            .unwrap_or(None);

            if let Some(row) = blocked_row {
                let task_id: String = row.try_get("id").unwrap_or_default();
                let title: String = row.try_get("title").unwrap_or_default();
                let desc: String = row.try_get::<Option<String>, _>("description").unwrap_or(None).unwrap_or_default();

                tracing::info!("🔄 [Preflight] Resuming blocked task {} first: {}", task_id, title);
                payload.message = format!("Resuming task {}: {}\n{}", task_id, title, desc);
                payload.primary_goal = Some(title);
            }
        }

        let depth = payload.swarm_depth.unwrap_or(0);
        let lineage = payload.swarm_lineage.clone().unwrap_or_default();

        // ♻️ [SSCP] Rehydrate BEFORE context preparation to ensure prompt has state
        if let Some(restored) = self.state.resources.continuity_arbiter.rehydrate(&agent_id).await {
            if let Some(mut agent) = self.state.registry.agents.get_mut(agent_id) {
                agent.state.working_memory = restored;
                tracing::info!("♻️ [SSCP] Rehydrated working memory for agent {} before context resolution", agent_id);
            }
        }

        let ctx = self
            .prepare_run_context(&agent_id, &payload, &mission_id, depth, &lineage)
            .await?;
        cleanup_guard.ctx = Some(ctx.clone());

        // Phase 2: Preflight Checks validation
        self.execute_preflight_checks(agent_id, &agent_data).await?;

        if let Some(workflow_name) = agent_data.capabilities.workflows.first() {
            let msg_lower = payload.message.to_lowercase();
            let requested = msg_lower.contains(&workflow_name.to_lowercase().replace('_', " "))
                || msg_lower.starts_with("/workflow") || msg_lower.starts_with("/sop");
            if requested || agent_data.runner_policy.sme_auto_workflow {
                if let Ok(mut workflow) = crate::agent::workflows::load_workflow(self.state.base_dir.as_path(), workflow_name).await {
                    return self.run_deterministic_workflow(&ctx, payload, &mut workflow, &mut cleanup_guard.usage).await;
                }
            }
        }

        self.state
            .yield_phase_transition(&agent_id, "IntelligenceLoop")
            .await;
        self.record_heartbeat(&ctx.agent_id).await;
        let output_res = self.execute_intelligence_loop(&ctx, &payload).await;

        match output_res {
            Ok(output) => {
                self.state
                    .yield_phase_transition(&agent_id, "Finalization")
                    .await;
                self.record_heartbeat(&ctx.agent_id).await;
                cleanup_guard.usage = output.usage.clone();
                self.finalize_run(&ctx, &output.text, &output.usage).await
            }
            Err((e, usage)) => {
                cleanup_guard.usage = usage;
                Err(e)
            }
        }
    }

    /// Safely truncates a string to a byte limit without breaking UTF-8 boundaries.
    pub(crate) fn safe_truncate(&self, s: &str, limit: usize) -> String {
        if s.len() <= limit {
            return s.to_string();
        }
        let mut end = limit;
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}... [TRUNCATED]", &s[..end])
    }

    /// Executes preflight checks (concurrency, context version, and skill subscriptions)
    /// based on the agent's runner policy.
    pub(crate) async fn execute_preflight_checks(
        &self,
        agent_id: &str,
        agent_data: &crate::agent::types::EngineAgent,
    ) -> Result<(), AppError> {
        use sqlx::Row;
        let policy = &agent_data.runner_policy;

        // 1. Update status ledger to 'checking'
        let _ = sqlx::query(
            "UPDATE agent_status_ledger
             SET last_queue_result = 'checking', last_heartbeat = ?
             WHERE agent_id = ?"
        )
        .bind(chrono::Utc::now().timestamp())
        .bind(agent_id)
        .execute(&self.state.resources.pool)
        .await;

        // 2. Concurrency check
        if policy.max_concurrent > 0 {
            let active_claims: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM agent_tasks WHERE claimed_by = ? AND status = 'working'"
            )
            .bind(agent_id)
            .fetch_one(&self.state.resources.pool)
            .await
            .map_err(AppError::Sqlx)?;

            if active_claims >= policy.max_concurrent as i64 {
                let _ = sqlx::query(
                    "UPDATE agent_status_ledger
                     SET last_queue_result = 'blocked concurrent', last_heartbeat = ?
                     WHERE agent_id = ?"
                )
                .bind(chrono::Utc::now().timestamp())
                .bind(agent_id)
                .execute(&self.state.resources.pool)
                .await;

                return Err(AppError::Conflict(format!(
                    "Concurrency limit reached: agent '{}' has {} active claimed tasks (limit is {})",
                    agent_id, active_claims, policy.max_concurrent
                )));
            }
        }

        // 3. Context version check
        if policy.preflight_checks.iter().any(|c| c == "context_version") {
            let ledger_version: i64 = sqlx::query_scalar(
                "SELECT context_version FROM agent_status_ledger WHERE agent_id = ?"
            )
            .bind(agent_id)
            .fetch_optional(&self.state.resources.pool)
            .await
            .map_err(AppError::Sqlx)?
            .unwrap_or(1);

            if ledger_version > agent_data.version as i64 {
                let _ = sqlx::query(
                    "UPDATE agent_status_ledger
                     SET last_queue_result = 'blocked context_version', last_heartbeat = ?
                     WHERE agent_id = ?"
                )
                .bind(chrono::Utc::now().timestamp())
                .bind(agent_id)
                .execute(&self.state.resources.pool)
                .await;

                return Err(AppError::BadRequest(format!(
                    "Preflight validation failed: Agent context version ({}) is behind ledger version ({})",
                    agent_data.version, ledger_version
                )));
            }
        }

        // 4. Skill subscriptions check
        if policy.preflight_checks.iter().any(|c| c == "skill_updates") {
            let subs = sqlx::query(
                "SELECT skill_id, scope_hash, subscription_status FROM skill_subscriptions WHERE agent_id = ?"
            )
            .bind(agent_id)
            .fetch_all(&self.state.resources.pool)
            .await
            .map_err(AppError::Sqlx)?;

            for skill_id in &agent_data.capabilities.skills {
                let norm_skill = skill_id.trim().to_lowercase().replace(' ', "_").replace('-', "_");
                let sub = subs.iter().find(|row| {
                    if let Ok(db_skill) = row.try_get::<String, _>("skill_id") {
                        let norm_db = db_skill.trim().to_lowercase().replace(' ', "_").replace('-', "_");
                        norm_db == norm_skill
                    } else {
                        false
                    }
                });

                match sub {
                    None => {
                        return Err(AppError::BadRequest(format!(
                            "Preflight verification failed: Skill '{}' is not subscribed by agent '{}'",
                            skill_id, agent_id
                        )));
                    }
                    Some(row) => {
                        let status: String = row.try_get("subscription_status").unwrap_or_default();
                        if status != "approved" {
                            return Err(AppError::BadRequest(format!(
                                "Preflight verification failed: Skill '{}' subscription status is '{}' (requires 'approved')",
                                skill_id, status
                            )));
                        }

                        if let Some(manifest) = self
                            .state
                            .registry
                            .skill_registry
                            .get(skill_id)
                            .or_else(|| self.state.registry.skill_registry.get(&norm_skill))
                        {
                            let current_hash = crate::routes::agentic_engine::compute_skill_scope_hash(&manifest);
                            let approved_hash: String = row.try_get("scope_hash").unwrap_or_default();

                            if current_hash != approved_hash {
                                let _ = sqlx::query(
                                    "UPDATE skill_subscriptions
                                     SET subscription_status = 'pending_reapproval', last_updated_at = ?
                                     WHERE agent_id = ? AND skill_id = ?"
                                )
                                .bind(chrono::Utc::now().timestamp())
                                .bind(agent_id)
                                .bind(skill_id)
                                .execute(&self.state.resources.pool)
                                .await;

                                return Err(AppError::BadRequest(format!(
                                    "Preflight verification failed: Skill '{}' scope has changed and requires reapproval",
                                    skill_id
                                )));
                            }
                        }
                    }
                }
            }
        }

        let _ = sqlx::query(
            "UPDATE agent_status_ledger
             SET last_queue_result = 'executing', last_heartbeat = ?
             WHERE agent_id = ?"
        )
        .bind(chrono::Utc::now().timestamp())
        .bind(agent_id)
        .execute(&self.state.resources.pool)
        .await;

        Ok(())
    }
}



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_subtask_payload() {
        let ctx = RunContext {
            agent_id: "parent".to_string(),
            mission_id: "mission-123".to_string(),
            depth: 1,
            lineage: vec!["grandparent".to_string()],
            ..Default::default()
        };

        let payload = ctx.derive_subtask_payload("Hello".to_string());

        assert_eq!(payload.message, "Hello");
        assert_eq!(payload.cluster_id, Some("mission-123".to_string()));
        assert_eq!(payload.swarm_depth, Some(2));
        assert_eq!(payload.swarm_lineage, Some(vec!["grandparent".to_string(), "parent".to_string()]));
    }

    #[tokio::test]
    async fn test_safe_truncate() {
        let state = Arc::new(AppState::new_minimal_mock().await);
        let runner = AgentRunner::new(state);
        
        let s = "Hello World";
        assert_eq!(runner.safe_truncate(s, 5), "Hello... [TRUNCATED]");
        assert_eq!(runner.safe_truncate(s, 20), "Hello World");

        // Test UTF-8 boundary
        let emoji = "👋 Hello";
        // 👋 is 4 bytes. Truncating at 2 should back off to 0.
        let truncated = runner.safe_truncate(emoji, 2);
        assert!(truncated.contains("... [TRUNCATED]"));
    }

    #[tokio::test]
    async fn finding_regression_context_failure_finalizes_initialized_mission_before_returning() {
        let state = crate::test_support::state_with_agent("context-owner", "http://127.0.0.1:9/v1").await;
        let runner = AgentRunner::new(state.clone());
        let error = runner.run("context-owner".to_string(), TaskPayload {
            message: "Validate context".to_string(), cluster_id: Some("...".to_string()), ..Default::default()
        }).await.unwrap_err();
        assert!(matches!(error, AppError::BadRequest(message) if message.contains("workspace ID")));
        assert_eq!(sqlx::query_scalar::<_, String>("SELECT status FROM mission_history WHERE agent_id = 'context-owner'")
            .fetch_one(&state.resources.pool).await.unwrap(), "failed");
        let agent = state.registry.agents.get("context-owner").unwrap().clone();
        assert_eq!(agent.health.status, "idle");
        assert!(agent.state.active_mission.is_none());
        assert_eq!(agent.health.failure_count, 1);
        assert_eq!(state.governance.active_agents.load(Ordering::SeqCst), 0);
        assert_eq!(sqlx::query_scalar::<_, String>("SELECT status FROM agents WHERE id = 'context-owner'")
            .fetch_one(&state.resources.pool).await.unwrap(), "idle");
    }

    #[tokio::test]
    async fn finding_regression_workflow_uses_initialized_mission_and_aggregates_all_steps() {
        let provider = crate::test_support::ProviderStub::start(axum::http::StatusCode::OK, "step completed", false).await;
        let state = crate::test_support::state_with_agent("workflow-owner", &provider.base_url).await;
        let directory = state.base_dir.join("data/workflows");
        tokio::fs::create_dir_all(&directory).await.unwrap();
        tokio::fs::write(directory.join("regression.md"), "## First step\nComplete the first step.\n## Second step\nComplete the second step.\n").await.unwrap();
        state.registry.agents.get_mut("workflow-owner").unwrap().capabilities.workflows = vec!["regression".to_string()];
        let runner = AgentRunner::new(state.clone());
        let output = runner.run("workflow-owner".to_string(), TaskPayload {
            message: "/workflow regression".to_string(), cluster_id: Some("workspace-scope".to_string()), ..Default::default()
        }).await.unwrap();
        assert!(output.contains("First step") && output.contains("Second step"));
        // Each specialist step makes its initial call and a Sentinel re-prompt.
        assert_eq!(provider.calls.load(Ordering::SeqCst), 4);
        let (mission_id, status, cost): (String, String, f64) = sqlx::query_as("SELECT id, status, cost_usd FROM mission_history WHERE agent_id = 'workflow-owner'")
            .fetch_one(&state.resources.pool).await.unwrap();
        assert_ne!(mission_id, "workspace-scope");
        assert_eq!(status, "completed");
        assert!((cost - 0.00008).abs() < 1e-12);
        let agent = state.registry.agents.get("workflow-owner").unwrap();
        assert_eq!(agent.economics.tokens_used, 60);
        assert!(agent.state.active_mission.is_none());
        assert!((state.security.budget_guard.get_global_stats().1 - cost).abs() < 1e-12);
    }

    #[tokio::test]
    async fn finding_regression_initialization_write_failure_leaves_no_ghost_mission() {
        let state = crate::test_support::state_with_agent("initialization-owner", "http://127.0.0.1:9/v1").await;
        sqlx::query("CREATE TRIGGER reject_initialization BEFORE UPDATE ON agents BEGIN SELECT RAISE(ABORT, 'injected initialization failure'); END")
            .execute(&state.resources.pool).await.unwrap();
        let runner = AgentRunner::new(state.clone());
        assert!(runner.run("initialization-owner".to_string(), TaskPayload {
            message: "Validate initialization".to_string(), ..Default::default()
        }).await.is_err());
        assert_eq!(sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM mission_history WHERE agent_id = 'initialization-owner'")
            .fetch_one(&state.resources.pool).await.unwrap(), 0);
        let agent = state.registry.agents.get("initialization-owner").unwrap();
        assert_eq!(agent.health.status, "idle");
        assert_eq!(agent.version, 1);
        assert!(agent.state.active_mission.is_none());
    }

    #[tokio::test]
    async fn finding_regression_terminal_write_failure_preserves_owned_mission_for_recovery() {
        let state = crate::test_support::state_with_agent("terminal-owner", "http://127.0.0.1:9/v1").await;
        sqlx::query("CREATE TRIGGER reject_terminal_status BEFORE UPDATE ON mission_history WHEN NEW.status = 'failed' BEGIN SELECT RAISE(ABORT, 'injected terminal failure'); END")
            .execute(&state.resources.pool).await.unwrap();
        let _lifecycle = state.registry.lock_agent("terminal-owner").await;
        let task = crate::routes::agent::spawn_agent_runner(state.clone(), "terminal-owner".to_string(), TaskPayload {
            message: "Validate terminal recovery".to_string(), cluster_id: Some("...".to_string()), ..Default::default()
        });
        drop(_lifecycle);
        assert!(matches!(task.await.unwrap(), Err(AppError::Sqlx(_))));
        let lifecycle = state.registry.lock_agent("terminal-owner").await;
        assert_eq!(sqlx::query_scalar::<_, String>("SELECT status FROM mission_history WHERE agent_id = 'terminal-owner'")
            .fetch_one(&state.resources.pool).await.unwrap(), "active");
        let mut agent = state.registry.agents.get("terminal-owner").unwrap().clone();
        assert_eq!(agent.health.status, "busy");
        assert!(agent.state.active_mission.is_some());
        assert!(!state.comms.active_runners.contains_key("terminal-owner"));
        // A permanent storage failure must remain visible and retain ownership.
        // Once storage recovers, the reaper uses the ordinary terminal transaction.
        sqlx::query("DROP TRIGGER reject_terminal_status").execute(&state.resources.pool).await.unwrap();
        agent.health.heartbeat_at = Some(chrono::Utc::now() - chrono::Duration::seconds(700));
        agent.version = crate::agent::persistence::save_agent_db(&state.resources.pool, &agent).await.unwrap();
        state.registry.agents.insert("terminal-owner".to_string(), agent);
        drop(lifecycle);
        // Drop cleanup may already have recovered after the rejecting trigger was removed.
        crate::agent::persistence::reap_stale_agents(&state, 300).await.unwrap();
        assert_eq!(sqlx::query_scalar::<_, String>("SELECT status FROM mission_history WHERE agent_id = 'terminal-owner'")
            .fetch_one(&state.resources.pool).await.unwrap(), "failed");
        assert_eq!(state.registry.agents.get("terminal-owner").unwrap().health.status, "idle");
    }

    #[tokio::test]
    async fn finding_regression_cancelled_initialization_recovers_unclaimed_mission() {
        let state = crate::test_support::state_with_agent("cancel-initialization", "http://127.0.0.1:9/v1").await;
        let mut connections = Vec::new();
        for _ in 0..state.resources.pool.options().get_max_connections() {
            connections.push(state.resources.pool.acquire().await.unwrap());
        }
        let mut events = state.comms.event_tx.subscribe();
        let runner = AgentRunner::new(state.clone());
        let caller = tokio::spawn(async move {
            runner.run("cancel-initialization".to_string(), TaskPayload { message: "Initialize".to_string(), ..Default::default() }).await
        });
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                let event = events.recv().await.unwrap();
                if event["phase"] == "Initialization" { break; }
            }
            // Let initialization own its lifecycle lock and wait for SQL.
            while state.registry.try_lock_agent("cancel-initialization").is_ok() {
                tokio::task::yield_now().await;
            }
        }).await.unwrap();
        caller.abort();
        assert!(caller.await.unwrap_err().is_cancelled());
        drop(connections);
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                let statuses: Vec<String> = sqlx::query_scalar("SELECT status FROM mission_history WHERE agent_id = 'cancel-initialization'")
                    .fetch_all(&state.resources.pool).await.unwrap();
                if statuses == ["failed"] && state.comms.mission_contexts.is_empty() { break; }
                tokio::task::yield_now().await;
            }
        }).await.unwrap();
        let agent = state.registry.agents.get("cancel-initialization").unwrap().clone();
        assert_eq!(agent.health.status, "idle");
        assert!(agent.state.active_mission.is_none());
        assert_eq!(state.governance.active_agents.load(Ordering::SeqCst), 0);
    }
}





// Metadata: [mod]

//! @docs ARCHITECTURE:Agent
//!
//! ### AI Assist Note
//! **EngineAgent & Runtime Policy Schemas**: Defines EngineAgent composition (identity, economics, health, models, capabilities, state), ToolContext, and wire serialization.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Malformed agent payload deserialization or wire-format mismatch.
//! - **Telemetry Link**: Search `[agent]` in tracing logs.

use super::*;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashMap;

#[derive(Debug, Clone, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentIdentity {
    pub id: String,
    pub name: String,
    pub role: String,
    pub department: String,
    pub description: String,
    pub category: String,
    #[serde(alias = "theme_color")]
    pub theme_color: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentEconomics {
    #[serde(alias = "budget_usd")]
    pub budget_usd: f64,
    #[serde(alias = "cost_usd")]
    pub cost_usd: f64,
    #[serde(alias = "tokens_used")]
    pub tokens_used: u32,
    #[serde(alias = "token_usage")]
    pub token_usage: TokenUsage,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentHealth {
    pub status: String,
    #[serde(alias = "failure_count")]
    pub failure_count: u32,
    #[serde(alias = "last_failure_at")]
    pub last_failure_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(alias = "heartbeat_at")]
    pub heartbeat_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl Default for AgentHealth {
    fn default() -> Self {
        Self {
            status: "idle".to_string(),
            failure_count: 0,
            last_failure_at: None,
            heartbeat_at: None,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentModels {
    #[serde(alias = "model_id")]
    pub model_id: Option<String>,
    pub model: ModelConfig,
    #[serde(alias = "model_2")]
    pub model_2: Option<String>,
    #[serde(alias = "model_3")]
    pub model_3: Option<String>,
    #[serde(alias = "model_config2")]
    pub model_config2: Option<ModelConfig>,
    #[serde(alias = "model_config3")]
    pub model_config3: Option<ModelConfig>,
    #[serde(alias = "active_model_slot")]
    pub active_model_slot: Option<i32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentCapabilities {
    pub skills: Vec<String>,
    pub workflows: Vec<String>,
    #[serde(alias = "mcp_tools")]
    pub mcp_tools: Vec<String>,
    #[serde(alias = "skill_manifest")]
    pub skill_manifest: Option<crate::agent::skill_manifest::SkillManifest>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentState {
    #[serde(alias = "active_mission")]
    pub active_mission: Option<serde_json::Value>,
    #[serde(alias = "current_task")]
    pub current_task: Option<String>,
    #[serde(alias = "working_memory")]
    pub working_memory: serde_json::Value,
    #[serde(alias = "current_reasoning_turn")]
    pub current_reasoning_turn: u32,
}

/// ### 📡 Protocol: ToolContext
/// A lightweight, isolated view of the application state for tool execution.
/// Prevents tools from accessing unrelated runner internals while providing
/// necessary governance metadata.
pub struct ToolContext {
    pub mission_id: String,
    pub agent_id: String,
    pub workspace_root: std::path::PathBuf,
    pub fs_adapter: crate::adapter::filesystem::FilesystemAdapter,
    pub state: std::sync::Arc<crate::state::AppState>,
    pub trace_id: String,
    pub budget_usd: f64,
    pub budget_limit_usd: f64,
    pub security_policy: serde_json::Value,
    pub active_node_id: Option<String>,
    pub depth: u32,
    pub lineage: Vec<String>,
    pub safe_mode: bool,
    pub primary_goal: Option<String>,
    pub last_accessed_files: std::sync::Arc<parking_lot::Mutex<Vec<String>>>,
    pub metrics: std::sync::Arc<crate::agent::runner::execution_metrics::ExecutionMetrics>,
    pub accounting: std::sync::Arc<crate::agent::runner::accounting::MissionAccounting>,
    pub execution_plane: ExecutionPlane,
    pub accept_mode: AcceptMode,
    pub mandate: Mandate,
}

impl ToolContext {
    /// Creates an ephemeral ToolContext with default metrics and accounting.
    pub fn ephemeral(
        state: std::sync::Arc<crate::state::AppState>,
        workspace_root: std::path::PathBuf,
        mission_id: String,
        agent_id: String,
    ) -> Self {
        Self {
            fs_adapter: crate::adapter::filesystem::FilesystemAdapter::new(workspace_root.clone()),
            workspace_root,
            mission_id,
            agent_id,
            state,
            trace_id: uuid::Uuid::new_v4().to_string(),
            budget_usd: 0.0,
            budget_limit_usd: 10.0,
            security_policy: serde_json::json!({}),
            active_node_id: None,
            depth: 0,
            lineage: Vec::new(),
            safe_mode: false,
            primary_goal: None,
            last_accessed_files: std::sync::Arc::new(parking_lot::Mutex::new(Vec::new())),
            metrics: std::sync::Arc::new(crate::agent::runner::execution_metrics::ExecutionMetrics::default()),
            accounting: std::sync::Arc::new(crate::agent::runner::accounting::MissionAccounting::default()),
            execution_plane: ExecutionPlane::default(),
            accept_mode: AcceptMode::default(),
            mandate: Mandate::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RunnerPolicy {
    pub max_concurrent: u32,
    pub resume_blocked_first: bool,
    pub preflight_checks: Vec<String>,
    #[serde(default)]
    pub sme_auto_workflow: bool,
    #[serde(default)]
    pub cascade_routing: bool,
}

impl Default for RunnerPolicy {
    fn default() -> Self {
        Self {
            max_concurrent: 1,
            resume_blocked_first: true,
            preflight_checks: vec!["context_version".to_string(), "skill_updates".to_string()],
            sme_auto_workflow: false,
            cascade_routing: false,
        }
    }
}

fn default_runner_policy() -> RunnerPolicy {
    RunnerPolicy::default()
}

#[derive(Debug, Clone, Default, specta::Type)]
pub struct EngineAgent {
    pub identity: AgentIdentity,
    pub models: AgentModels,
    pub economics: AgentEconomics,
    pub health: AgentHealth,
    pub capabilities: AgentCapabilities,
    pub state: AgentState,
    pub metadata: std::collections::HashMap<String, serde_json::Value>,
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
    pub requires_oversight: bool,
    pub voice_id: Option<String>,
    pub voice_engine: Option<String>,
    pub stt_engine: Option<String>,
    pub connector_configs: Vec<ConnectorConfig>,
    pub version: u32,
    pub runner_policy: RunnerPolicy,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AgentResponse<'a> {
    #[serde(flatten)]
    identity: &'a AgentIdentity,
    #[serde(flatten)]
    models_raw: AgentModelsResponse<'a>,
    #[serde(flatten)]
    economics: &'a AgentEconomics,
    #[serde(flatten)]
    health: AgentHealthResponse<'a>,
    #[serde(flatten)]
    capabilities: &'a AgentCapabilities,
    #[serde(flatten)]
    state: &'a AgentState,
    metadata: &'a HashMap<String, serde_json::Value>,
    created_at: &'a Option<DateTime<Utc>>,
    requires_oversight: bool,
    voice_id: &'a Option<String>,
    voice_engine: &'a Option<String>,
    stt_engine: &'a Option<String>,
    connector_configs: &'a [ConnectorConfig],
    version: u32,
    runner_policy: &'a RunnerPolicy,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AgentModelsResponse<'a> {
    model_id: &'a Option<String>,
    model: &'a str,
    model_config: &'a ModelConfig,
    model_2: &'a Option<String>,
    model_config2: &'a Option<ModelConfig>,
    model_3: &'a Option<String>,
    model_config3: &'a Option<ModelConfig>,
    active_model_slot: &'a Option<i32>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AgentHealthResponse<'a> {
    status: &'a str,
    failure_count: u32,
    last_failure_at: &'a Option<DateTime<Utc>>,
    #[serde(rename = "lastPulse")]
    last_pulse: &'a Option<DateTime<Utc>>,
}

impl Serialize for EngineAgent {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let model_name = if self.models.model.model_id.trim().is_empty() {
            self.models.model_id.as_deref().unwrap_or_default()
        } else {
            self.models.model.model_id.as_str()
        };

        let response = AgentResponse {
            identity: &self.identity,
            models_raw: AgentModelsResponse {
                model_id: &self.models.model_id,
                model: model_name,
                model_config: &self.models.model,
                model_2: &self.models.model_2,
                model_config2: &self.models.model_config2,
                model_3: &self.models.model_3,
                model_config3: &self.models.model_config3,
                active_model_slot: &self.models.active_model_slot,
            },
            economics: &self.economics,
            health: AgentHealthResponse {
                status: &self.health.status,
                failure_count: self.health.failure_count,
                last_failure_at: &self.health.last_failure_at,
                last_pulse: &self.health.heartbeat_at,
            },
            capabilities: &self.capabilities,
            state: &self.state,
            metadata: &self.metadata,
            created_at: &self.created_at,
            requires_oversight: self.requires_oversight,
            voice_id: &self.voice_id,
            voice_engine: &self.voice_engine,
            stt_engine: &self.stt_engine,
            connector_configs: &self.connector_configs,
            version: self.version,
            runner_policy: &self.runner_policy,
        };
        response.serialize(serializer)
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum EngineAgentModelInput {
    ModelId(String),
    Config(Box<ModelConfig>),
}

#[derive(Deserialize, Default)]
struct EngineAgentWire {
    id: String,
    name: String,
    role: String,
    department: String,
    description: String,
    #[serde(default, alias = "modelId", alias = "primary_model")]
    model_id: Option<String>,
    #[serde(default)]
    model: Option<EngineAgentModelInput>,
    #[serde(default, alias = "model_config", alias = "modelConfig")]
    model_config: Option<ModelConfig>,
    #[serde(default, alias = "model2")]
    model_2: Option<String>,
    #[serde(default, alias = "model3")]
    model_3: Option<String>,
    #[serde(default, alias = "model_config2", alias = "modelConfig2")]
    model_config2: Option<ModelConfig>,
    #[serde(default, alias = "model_config3", alias = "modelConfig3")]
    model_config3: Option<ModelConfig>,
    #[serde(default, alias = "activeModelSlot")]
    active_model_slot: Option<i32>,
    #[serde(default, alias = "system_prompt", alias = "systemPrompt")]
    system_prompt: Option<String>,
    #[serde(default, alias = "activeMission")]
    active_mission: Option<serde_json::Value>,
    status: String,
    #[serde(default, alias = "currentTask")]
    current_task: Option<String>,
    #[serde(default, alias = "tokensUsed")]
    tokens_used: u32,
    #[serde(default, alias = "tokenUsage")]
    token_usage: TokenUsage,
    #[serde(default)]
    skills: Vec<String>,
    #[serde(default)]
    workflows: Vec<String>,
    #[serde(default, alias = "mcpTools")]
    mcp_tools: Vec<String>,
    #[serde(default, alias = "skillManifest")]
    skill_manifest: Option<crate::agent::skill_manifest::SkillManifest>,
    #[serde(default)]
    metadata: HashMap<String, serde_json::Value>,
    #[serde(default, alias = "themeColor")]
    theme_color: Option<String>,
    #[serde(default, alias = "budgetUsd")]
    budget_usd: f64,
    #[serde(default, alias = "costUsd")]
    cost_usd: f64,
    #[serde(default, alias = "voiceId")]
    voice_id: Option<String>,
    #[serde(default, alias = "voiceEngine")]
    voice_engine: Option<String>,
    #[serde(default, alias = "sttEngine", alias = "stt_engine")]
    stt_engine: Option<String>,
    #[serde(default = "default_category")]
    category: String,
    #[serde(default, alias = "failureCount")]
    failure_count: u32,
    #[serde(default, alias = "lastFailureAt")]
    last_failure_at: Option<DateTime<Utc>>,
    #[serde(default, alias = "createdAt")]
    created_at: Option<DateTime<Utc>>,
    #[serde(default, alias = "heartbeatAt")]
    heartbeat_at: Option<DateTime<Utc>>,
    #[serde(default, alias = "lastPulse", alias = "last_pulse")]
    last_pulse: Option<DateTime<Utc>>,
    #[serde(default, alias = "requiresOversight")]
    requires_oversight: bool,
    #[serde(default, alias = "workingMemory")]
    working_memory: serde_json::Value,
    #[serde(default, alias = "connectorConfigs")]
    connector_configs: Vec<ConnectorConfig>,
    #[serde(default, alias = "currentReasoningTurn")]
    current_reasoning_turn: u32,
    #[serde(default = "default_version")]
    version: u32,
    #[serde(default = "default_runner_policy", alias = "runnerPolicy")]
    runner_policy: RunnerPolicy,
}

impl<'de> Deserialize<'de> for EngineAgent {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = EngineAgentWire::deserialize(deserializer)?;

        let mut model = match (wire.model_config, wire.model) {
            (Some(config), _) => config,
            (None, Some(EngineAgentModelInput::Config(config))) => *config,
            (None, Some(EngineAgentModelInput::ModelId(model_id))) => ModelConfig {
                provider: ModelProvider::from_str(&model_id).unwrap_or_default(),
                model_id,
                ..ModelConfig::default()
            },
            (None, None) => ModelConfig::default(),
        };

        if model.system_prompt.is_none() {
            model.system_prompt = wire.system_prompt;
        }

        let model_id = match wire.model_id {
            Some(mid) => {
                if model.model_id.is_empty() {
                    model.model_id = mid.clone();
                }
                Some(mid)
            }
            None if !model.model_id.is_empty() => Some(model.model_id.clone()),
            None => None,
        };

        Ok(Self {
            identity: AgentIdentity {
                id: wire.id,
                name: wire.name,
                role: wire.role,
                department: wire.department,
                description: wire.description,
                category: wire.category,
                theme_color: wire.theme_color,
            },
            models: AgentModels {
                model_id,
                model,
                model_2: wire.model_2,
                model_3: wire.model_3,
                model_config2: wire.model_config2,
                model_config3: wire.model_config3,
                active_model_slot: wire.active_model_slot,
            },
            economics: AgentEconomics {
                budget_usd: wire.budget_usd,
                cost_usd: wire.cost_usd,
                tokens_used: wire.tokens_used,
                token_usage: wire.token_usage,
            },
            health: AgentHealth {
                status: wire.status,
                failure_count: wire.failure_count,
                last_failure_at: wire.last_failure_at,
                heartbeat_at: wire.last_pulse.or(wire.heartbeat_at),
            },
            capabilities: AgentCapabilities {
                skills: wire.skills,
                workflows: wire.workflows,
                mcp_tools: wire.mcp_tools,
                skill_manifest: wire.skill_manifest,
            },
            state: AgentState {
                active_mission: wire.active_mission,
                current_task: wire.current_task,
                working_memory: wire.working_memory,
                current_reasoning_turn: wire.current_reasoning_turn,
            },
            metadata: wire.metadata,
            created_at: wire.created_at,
            requires_oversight: wire.requires_oversight,
            voice_id: wire.voice_id,
            voice_engine: wire.voice_engine,
            stt_engine: wire.stt_engine,
            connector_configs: wire.connector_configs,
            version: wire.version,
            runner_policy: wire.runner_policy,
        })
    }
}

fn default_version() -> u32 {
    1
}

pub(super) fn default_category() -> String {
    "user".to_string()
}

impl EngineAgent {
    #[allow(dead_code)]
    pub fn is_suspended(&self) -> bool {
        self.health.status == "suspended"
    }

    #[allow(dead_code)]
    pub fn resolve_provider_context(&self, base_dir: std::path::PathBuf) -> crate::agent::runner::RunContext {
        let workspace_root = base_dir.join("data/workspaces/default");
        crate::agent::runner::RunContext {
            accounting: Default::default(),
            agent_id: self.identity.id.clone(),
            name: self.identity.name.clone(),
            role: self.identity.role.clone(),
            department: self.identity.department.clone(),
            description: self.identity.description.clone(),
            model_config: self.models.model.clone(),
            skills: self.capabilities.skills.clone(),
            workflows: self.capabilities.workflows.clone(),
            mission_id: "system-internal".to_string(),
            depth: 0,
            lineage: vec![],
            provider_name: self.models.model.provider.to_string(),
            workspace_root: workspace_root.clone(),
            fs_adapter: crate::adapter::filesystem::FilesystemAdapter::new(workspace_root),
            safe_mode: false,
            execution_mode: crate::agent::types::ExecutionMode::Swarm,
            analysis: false,
            traceparent: None,
            trace_id: uuid::Uuid::new_v4().to_string(),
            user_id: None,
            last_accessed_files: std::sync::Arc::new(parking_lot::Mutex::new(Vec::new())),
            recent_findings: None,
            working_memory: self.state.working_memory.clone(),
            summarized_history: None,
            structured_output: false,
            backlog: None,
            primary_goal: self.state.current_task.clone(),
            budget_usd: self.economics.budget_usd,
            budget_limit_usd: 10.0,
            current_cost_usd: self.economics.cost_usd,
            reasoning_depth: self.models.model.reasoning_depth.unwrap_or(1),
            act_threshold: self.models.model.act_threshold.unwrap_or(0.95),
            max_turns: self.models.model.max_turns.unwrap_or(20),
            authority_level: crate::agent::types::RoleAuthorityLevel::from_role(&self.identity.role),
            resource_weights: std::collections::HashMap::new(),
            security_policy: serde_json::json!({}),
            execution_plane: ExecutionPlane::default(),
            accept_mode: AcceptMode::default(),
            mandate: Mandate::default(),
            active_node_id: std::sync::Arc::new(parking_lot::Mutex::new(None)),
            conflict: std::sync::Arc::new(crate::security::conflict::ConflictManager::new()),
            metrics: std::sync::Arc::new(crate::agent::runner::execution_metrics::ExecutionMetrics::new()),
            root_mission_id: "system-internal".to_string(),
            cascade_routing: self.runner_policy.cascade_routing,
            cancel_handle: crate::agent::swarm_orchestrator::MissionHandle::new("system-internal"),
        }
    }
}


// Metadata: [agent]

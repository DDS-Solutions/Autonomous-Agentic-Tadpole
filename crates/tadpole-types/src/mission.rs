//! @docs ARCHITECTURE:Agent
//!
//! ### AI Assist Note
//! **Mission, Chat, Oversight & Graph Schemas**: Defines TaskPayload, ChatMessage, ToolCall, Mission/MissionLog, OversightEntry, and SwarmGraph DTOs.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Schema mismatch on mission dispatch or graph telemetry payloads.
//! - **Telemetry Link**: Search `[mission]` in tracing logs.

use super::*;

fn default_category() -> String {
    "user".to_string()
}
use serde::{Deserialize, Serialize};

/// Three-tier execution authority mode for Neural Chat Command and Voice Standups.
/// - `Chat`: Lightweight conversational brainstorming (zero tools, no mission record).
/// - `Plan`: Read-only codebase/knowledge research (read-only tools only, no mutations, no mission record).
/// - `Swarm`: Full tracked Mission lifecycle with planning, delegation, and Oversight Gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    Chat,
    Plan,
    #[default]
    Swarm,
}

impl ExecutionMode {
    /// Returns true only when this mode creates and persists a tracked `mission_history` record.
    pub fn creates_mission(self) -> bool {
        matches!(self, Self::Swarm)
    }

    /// Returns true when tool declarations may be exposed to the model.
    pub fn allows_tools(self) -> bool {
        !matches!(self, Self::Chat)
    }

    /// Returns true when mutating/delegation tools must be blocked (`Chat` and `Plan`).
    pub fn is_safe_mode(self) -> bool {
        !matches!(self, Self::Swarm)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, specta::Type)]
pub struct TaskPayload {
    #[serde(alias = "task")]
    pub message: String,
    pub cluster_id: Option<String>,
    pub department: Option<String>,
    pub provider: Option<ModelProvider>,
    pub model_id: Option<String>,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub rpm: Option<u32>,
    pub tpm: Option<u32>,
    pub rpd: Option<u32>,
    pub tpd: Option<u32>,
    pub budget_usd: Option<f64>,
    #[serde(default, alias = "context_window", alias = "contextWindow")]
    pub context_window: Option<u32>,
    #[serde(default, alias = "input_cost_per_m", alias = "inputCostPerM")]
    pub input_cost_per_m: Option<f64>,
    #[serde(default, alias = "output_cost_per_m", alias = "outputCostPerM")]
    pub output_cost_per_m: Option<f64>,
    pub swarm_depth: Option<u32>,
    pub swarm_lineage: Option<Vec<String>>,
    pub external_id: Option<String>,
    pub safe_mode: Option<bool>,
    #[serde(default, alias = "executionMode")]
    pub execution_mode: Option<ExecutionMode>,
    pub analysis: Option<bool>,
    pub traceparent: Option<String>,
    #[serde(default, alias = "userId")]
    pub user_id: Option<String>,
    #[serde(default)]
    pub context_files: Option<Vec<String>>,
    #[serde(default)]
    pub recent_findings: Option<String>,
    #[serde(default)]
    pub structured_output: Option<bool>,
    #[serde(default, alias = "primaryGoal")]
    pub primary_goal: Option<String>,
    #[serde(default, alias = "enabledSkills")]
    pub enabled_skills: Option<Vec<String>>,
    #[serde(default, alias = "autoResume")]
    pub auto_resume: Option<bool>,
    #[serde(default, alias = "executionPlane")]
    pub execution_plane: Option<ExecutionPlane>,
    #[serde(default, alias = "acceptMode")]
    pub accept_mode: Option<AcceptMode>,
    #[serde(default)]
    pub mandate: Option<Mandate>,
    #[serde(default, alias = "workflowId")]
    pub workflow_id: Option<String>,
    #[serde(default, alias = "cascadeRouting")]
    pub cascade_routing: Option<bool>,
    #[serde(default, alias = "rootMissionId")]
    pub root_mission_id: Option<String>,
}

impl TaskPayload {
    /// Resolves the effective `ExecutionMode` with full backward compatibility for legacy `safe_mode`.
    pub fn resolved_execution_mode(&self) -> ExecutionMode {
        if let Some(mode) = self.execution_mode {
            return mode;
        }
        if self
            .safe_mode
            .unwrap_or_else(|| self.analysis.unwrap_or(false))
        {
            ExecutionMode::Plan
        } else {
            ExecutionMode::Swarm
        }
    }

    /// Resolves an immutable Mandate from explicit parameters or synthesizes one safely.
    pub fn resolved_mandate(&self) -> Mandate {
        if let Some(ref m) = self.mandate {
            m.clone()
        } else {
            Mandate {
                execution_plane: self.execution_plane.unwrap_or_default(),
                accept_mode: self.accept_mode.unwrap_or_default(),
                allowed_tools: self.enabled_skills.clone(),
                path_boundary: None,
                budget_usd: self.budget_usd.unwrap_or(0.10),
                delivery_target: self.primary_goal.clone(),
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, specta::Type)]
pub struct ToolCallAudit {
    pub id: String,
    pub mission_id: Option<String>,
    #[serde(rename = "agent_id")]
    pub agent_id: String,
    pub skill: String,
    pub params: serde_json::Value,
    pub department: String,
    pub description: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum SkillType {
    Skill,
    Workflow,
    Hook,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct SkillProposal {
    pub r#type: SkillType,
    pub name: String,
    pub description: String,
    pub execution_command: Option<String>,
    pub schema: Option<serde_json::Value>,
    pub content: Option<String>,
    pub full_instructions: Option<String>,
    pub negative_constraints: Option<Vec<String>>,
    pub verification_script: Option<String>,
    #[serde(default = "default_category")]
    pub category: String,
}



#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct OversightEntry {
    pub id: String,
    pub mission_id: Option<String>,
    pub tool_call: Option<ToolCallAudit>,
    #[serde(alias = "capability_proposal")]
    pub skill_proposal: Option<SkillProposal>,
    pub status: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, specta::Type)]
pub struct Mission {
    pub id: String,
    pub agent_id: String,
    pub title: String,
    pub status: MissionStatus,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub budget_usd: f64,
    pub cost_usd: f64,
    pub is_degraded: Option<bool>,
    pub is_pinned: bool,
    pub active_node_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, specta::Type)]
pub struct MissionLog {
    pub id: String,
    pub mission_id: String,
    pub agent_id: String,
    pub source: String,
    pub text: String,
    pub severity: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub metadata: Option<serde_json::Value>,
    pub hash: Option<String>,
    pub prev_hash: Option<String>,
}

/// Enhanced memory entry with scoring metadata for Advanced RAG.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MemoryEntryDetailed {
    pub id: String,
    pub text: String,
    pub mission_id: String,
    pub timestamp: i64,
    /// Raw semantic distance from vector search.
    pub distance: f32,
    /// Final calculated Multi-Factor Score.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<crate::rag_scoring::RagScore>,
}
/// ### 📡 Protocol: ChatMessage
/// Structured, multi-role message for provider message generation.
/// Decouples conversational turns from flat strings, enabling native
/// prompt caching (Anthropic/Gemini) and exact role-fidelity.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "role", rename_all = "snake_case")]
pub enum ChatMessage {
    System {
        content: String,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        cache_control: bool,
    },
    User {
        content: String,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        cache_control: bool,
    },
    Assistant {
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        tool_calls: Option<Vec<ToolCall>>,
    },
    Tool {
        tool_call_id: String,
        name: String,
        content: String,
    },
}

impl ChatMessage {
    pub fn user(content: impl Into<String>) -> Self {
        Self::User {
            content: content.into(),
            cache_control: false,
        }
    }

    pub fn system(content: impl Into<String>) -> Self {
        Self::System {
            content: content.into(),
            cache_control: false,
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self::Assistant {
            content: content.into(),
            tool_calls: None,
        }
    }

    pub fn tool(tool_call_id: impl Into<String>, name: impl Into<String>, content: impl Into<String>) -> Self {
        Self::Tool {
            tool_call_id: tool_call_id.into(),
            name: name.into(),
            content: content.into(),
        }
    }

    pub fn role_str(&self) -> &str {
        match self {
            Self::System { .. } => "system",
            Self::User { .. } => "user",
            Self::Assistant { .. } => "assistant",
            Self::Tool { .. } => "tool",
        }
    }

    pub fn content_str(&self) -> &str {
        match self {
            Self::System { content, .. } => content.as_str(),
            Self::User { content, .. } => content.as_str(),
            Self::Assistant { content, .. } => content.as_str(),
            Self::Tool { content, .. } => content.as_str(),
        }
    }

    pub fn to_flattened_string(&self) -> String {
        match self {
            Self::System { content, .. } => format!("SYSTEM: {}", content),
            Self::User { content, .. } => format!("USER: {}", content),
            Self::Assistant { content, .. } => format!("ASSISTANT: {}", content),
            Self::Tool { name, content, .. } => format!("OBSERVATION: Tool {} Result: {}", name, content),
        }
    }

    /// Converts a legacy flattened conversation history line into a ChatMessage.
    pub fn from_legacy_turn(turn: &str) -> Self {
        let trimmed = turn.trim();
        if let Some(rest) = trimmed.strip_prefix("USER: ") {
            Self::user(rest)
        } else if let Some(rest) = trimmed.strip_prefix("ASSISTANT: ") {
            Self::assistant(rest)
        } else if let Some(rest) = trimmed.strip_prefix("SYSTEM: ") {
            Self::system(rest)
        } else if let Some(rest) = trimmed.strip_prefix("OBSERVATION: ") {
            let rest_trimmed = rest.trim();
            if let Some(after_tool) = rest_trimmed.strip_prefix("Tool ") {
                if let Some((tool_name, tool_output)) = after_tool.split_once(" Result: ") {
                    Self::tool("legacy", tool_name.trim(), tool_output)
                } else {
                    Self::tool("legacy", "observation", rest_trimmed)
                }
            } else {
                Self::tool("legacy", "observation", rest_trimmed)
            }
        } else {
            Self::user(trimmed)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct ToolCall {
    pub name: String,
    pub args: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ToolDefinition {
    pub function_declarations: Vec<FunctionDeclaration>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FunctionDeclaration {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmNode {
    pub id: String,
    pub name: String,
    pub address: String,
    pub status: String,
    pub last_seen: chrono::DateTime<chrono::Utc>,
    pub metadata: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OversightDecision {
    pub decision: String,
    #[serde(default)]
    pub answer: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmGraph {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    pub r#type: String,
    pub status: String,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    pub id: String,
    pub source: String,
    pub target: String,
    pub label: String,
    pub metadata: serde_json::Value,
}


// Metadata: [mission]

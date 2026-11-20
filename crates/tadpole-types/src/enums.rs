//! @docs ARCHITECTURE:Agent
//!
//! ### AI Assist Note
//! **Core Agent Enumerations & Mandates**: Defines compile-time safe enums for providers, modalities, mission states, authority levels, execution planes, and mandates.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Invalid enum variant deserialization or mandate mismatch.
//! - **Telemetry Link**: Search `[enums]` in tracing logs.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// =============================================================================
// ENUMERATIONS (For Compile-Time Safety)
// =============================================================================

/// ### 📡 Protocol: ModelProvider
/// Defines the set of supported LLM backend protocols for the Tadpole OS engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type, Default, specta::Type)]
#[sqlx(rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum ModelProvider {
    Openai,
    Anthropic,
    Google,
    Gemini, // Alias for Google
    #[default]
    Ollama,
    Groq,
    Mistral,
    Perplexity,
    Fireworks,
    Together,
    Deepseek,
    Xai,
    Inception,
    Openrouter,
    Cerebras,
    Sambanova,
    Meta,
    Alibaba,
    Local,
}

impl ModelProvider {
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "openai" | "open-ai" => Some(Self::Openai),
            "anthropic" | "claude" | "claude-3" => Some(Self::Anthropic),
            "google" | "gemini" | "google-ai-studio" | "google-vertex" => Some(Self::Google),
            "ollama" => Some(Self::Ollama),
            "groq" => Some(Self::Groq),
            "mistral" | "mistral-ai" | "mistralai" => Some(Self::Mistral),
            "perplexity" | "pplx" => Some(Self::Perplexity),
            "fireworks" | "fireworks-ai" => Some(Self::Fireworks),
            "together" | "together-ai" => Some(Self::Together),
            "deepseek" | "deep-seek" => Some(Self::Deepseek),
            "xai" | "grok" | "x-ai" => Some(Self::Xai),
            "inception" | "mercury" => Some(Self::Inception),
            "openrouter" | "open-router" => Some(Self::Openrouter),
            "cerebras" => Some(Self::Cerebras),
            "sambanova" | "samba-nova" => Some(Self::Sambanova),
            "meta" | "llama" => Some(Self::Meta),
            "alibaba" | "qwen" => Some(Self::Alibaba),
            "local" | "custom" => Some(Self::Local),
            _ => None,
        }
    }
}

impl std::fmt::Display for ModelProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Openai => "openai",
            Self::Anthropic => "anthropic",
            Self::Google | Self::Gemini => "google",
            Self::Ollama => "ollama",
            Self::Groq => "groq",
            Self::Mistral => "mistral",
            Self::Perplexity => "perplexity",
            Self::Fireworks => "fireworks",
            Self::Together => "together",
            Self::Deepseek => "deepseek",
            Self::Xai => "xai",
            Self::Inception => "inception",
            Self::Openrouter => "openrouter",
            Self::Cerebras => "cerebras",
            Self::Sambanova => "sambanova",
            Self::Meta => "meta",
            Self::Alibaba => "alibaba",
            Self::Local => "local",
        };
        write!(f, "{}", s)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum Modality {
    #[default]
    Llm,
    Vision,
    Voice,
    Audio,
    Reasoning,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, sqlx::Type, specta::Type)]
#[sqlx(rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum MissionStatus {
    Pending,
    Active,
    Completed,
    Failed,
    Paused,
    Suspended,
    Dropped,
}

/// ### 📡 Protocol: RoleAuthorityLevel
/// Defines the authority level of an agent in the swarm.
/// Strict ordering: Observer (lowest) < Specialist < Management < Executive (highest).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum RoleAuthorityLevel {
    /// Observer level - Read-only oversight.
    Observer,
    /// Specialist level - Task execution.
    Specialist,
    /// Management level (Alpha Node) - Tactical coordination.
    Management,
    /// Executive level (CEO, COO) - Strategic oversight and delegation.
    Executive,
}

impl RoleAuthorityLevel {
    pub fn from_role(role: &str) -> Self {
        let r = role.to_lowercase();
        if r.contains("ceo") || r.contains("overlord") || r.contains("executive") {
            Self::Executive
        } else if r.contains("coo") || r.contains("orchestrator") || r.contains("commander") || r.contains("alpha") {
            Self::Management
        } else if r.contains("specialist") || r.contains("coder") || r.contains("engineer") || r.contains("developer") || r.contains("agent") {
            Self::Specialist
        } else {
            Self::Observer
        }
    }
}

/// ### 📡 Protocol: ProviderStatus
/// Represents the health state of an LLM provider.
/// - Green: Healthy, all models functional.
/// - Amber: Degraded, high failure rate or rate limited. Diverts to secondary.
/// - Red: Down, critical failures. Fails over to fallback or NullProvider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum ProviderStatus {
    Green,
    Amber,
    Red,
}

/// ### Protocol: ExecutionPlane
/// Represents the dual-plane sovereign execution hierarchy governing swarm autonomy.
/// - Plane 1 (EngineMaintenance): Recursively operates, diagnoses, fixes, and improves the codebase.
/// - Plane 2 (SmbDigitalTwin): Company-agnostic SMB business twin runtime governed by FR-11 permission firewall.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "kebab-case")]
pub enum ExecutionPlane {
    #[default]
    #[serde(alias = "plane-1", alias = "engine_maintenance", alias = "EngineMaintenance")]
    EngineMaintenance,
    #[serde(alias = "plane-2", alias = "smb_digital_twin", alias = "SmbDigitalTwin")]
    SmbDigitalTwin,
}

/// ### 📜 Protocol: AcceptMode
/// Governs operational side-effect permissions:
/// - Auto: Fully autonomous mutation within Mandate boundaries.
/// - Observe: Shadow execution mode. Plans, simulates blast radius, and writes audit receipts without mutating external systems.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum AcceptMode {
    #[default]
    #[serde(alias = "auto")]
    Auto,
    #[serde(alias = "observe")]
    Observe,
}

/// ### 📜 Protocol: Mandate
/// Immutable execution envelope decoupling caller authorization ("who may ask")
/// from runtime execution scope ("what may run").
/// Strictly bounds Plane 1 (EngineMaintenance) and Plane 2 (SmbDigitalTwin) execution.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Mandate {
    pub execution_plane: ExecutionPlane,
    pub accept_mode: AcceptMode,
    #[serde(default)]
    pub allowed_tools: Option<Vec<String>>,
    #[serde(default)]
    pub path_boundary: Option<String>,
    pub budget_usd: f64,
    #[serde(default)]
    pub delivery_target: Option<String>,
}

impl Default for Mandate {
    fn default() -> Self {
        Self {
            execution_plane: ExecutionPlane::default(),
            accept_mode: AcceptMode::default(),
            allowed_tools: None,
            path_boundary: None,
            budget_usd: 0.10,
            delivery_target: None,
        }
    }
}

pub trait Validatable {
    fn validate(&self) -> Result<(), String>;
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TokenUsage {
    #[serde(default, alias = "input_tokens")]
    pub input_tokens: u32,
    #[serde(default, alias = "output_tokens")]
    pub output_tokens: u32,
    #[serde(default, alias = "total_tokens")]
    pub total_tokens: u32,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, Default, specta::Type)]
pub struct SyncManifest {
    pub id: String,
    pub agent_id: String,
    pub source_type: String,
    pub source_uri: String,
    pub status: String,
    pub last_sync_at: Option<DateTime<Utc>>,
}


// Metadata: [enums]

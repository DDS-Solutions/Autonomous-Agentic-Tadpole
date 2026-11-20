//! @docs ARCHITECTURE:Agent
//!
//! ### AI Assist Note
//! **Model Registry & Blueprint Schemas**: Defines IMR-01 model capability matrices, provider configs, model entries, and role blueprints with validation.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Invalid provider/model configuration or missing capability defaults.
//! - **Telemetry Link**: Search `[model]` in tracing logs.

use super::*;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModelCapabilities {
    #[serde(default, alias = "supports_tools")]
    pub supports_tools: bool,
    #[serde(default, alias = "supports_vision")]
    pub supports_vision: bool,
    #[serde(default, alias = "supports_structured_output")]
    pub supports_structured_output: bool,
    #[serde(default, alias = "supports_reasoning")]
    pub supports_reasoning: bool,
    #[serde(default, alias = "supports_halting_tool")]
    pub supports_halting_tool: bool,
    #[serde(default, alias = "context_window")]
    pub context_window: Option<u32>,
    #[serde(default, alias = "max_output_tokens")]
    pub max_output_tokens: Option<u32>,
    #[serde(default, alias = "is_embedding_model")]
    pub is_embedding_model: bool,
    #[serde(default, alias = "is_agentic_recommended")]
    pub is_agentic_recommended: bool,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default = "default_source")]
    pub source: String,
}

fn default_source() -> String {
    "matrix".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct CapabilityPattern {
    pub slugs: Vec<String>,
    pub supports_tools: Option<bool>,
    pub supports_vision: Option<bool>,
    pub supports_structured_output: Option<bool>,
    pub supports_reasoning: Option<bool>,
    pub supports_halting_tool: Option<bool>,
    pub context_window: Option<u32>,
    pub max_output_tokens: Option<u32>,
    pub is_embedding_model: Option<bool>,
    pub is_agentic_recommended: Option<bool>,
    pub tags: Option<Vec<String>>,
}

impl CapabilityPattern {
    pub fn update(&self, caps: &mut ModelCapabilities) {
        if let Some(val) = self.supports_tools {
            caps.supports_tools = val;
        }
        if let Some(val) = self.supports_vision {
            caps.supports_vision = val;
        }
        if let Some(val) = self.supports_structured_output {
            caps.supports_structured_output = val;
        }
        if let Some(val) = self.supports_reasoning {
            caps.supports_reasoning = val;
        }
        if let Some(val) = self.supports_halting_tool {
            caps.supports_halting_tool = val;
        }
        if let Some(val) = self.context_window {
            caps.context_window = Some(val);
        }
        if let Some(val) = self.max_output_tokens {
            caps.max_output_tokens = Some(val);
        }
        if let Some(val) = self.is_embedding_model {
            caps.is_embedding_model = val;
        }
        if let Some(val) = self.is_agentic_recommended {
            caps.is_agentic_recommended = val;
        }
        if let Some(ref val) = self.tags {
            caps.tags = val.clone();
        }
    }
}

pub static CAPABILITY_PATTERNS: once_cell::sync::Lazy<Vec<CapabilityPattern>> = once_cell::sync::Lazy::new(|| {
    let raw = include_str!("capability_patterns.json");
    serde_json::from_str(raw).unwrap_or_else(|e| {
        tracing::error!("🚨 Failed to parse capability_patterns.json: {}", e);
        Vec::new()
    })
});

impl ModelCapabilities {
    /// ### IMR-01: Intelligent Inference
    /// Automatically infers capabilities based on the Model ID.
    pub fn infer_from_id(model_id: &str) -> Self {
        let id = model_id.to_lowercase();
        let mut caps = Self {
            context_window: Some(32_768),
            max_output_tokens: Some(4_096),
            supports_tools: true,
            supports_halting_tool: true,
            source: "matrix".to_string(),
            ..Self::default()
        };

        for pattern in &*CAPABILITY_PATTERNS {
            if pattern.slugs.iter().any(|s| id.contains(s)) {
                pattern.update(&mut caps);
            }
        }

        // Final Edge Case logic not easily captured by slugs
        if id.contains("gemini-1.5-pro") {
            caps.context_window = Some(2_000_000);
        }
        if id.contains("gemma-4") && (id.contains("26b") || id.contains("moe") || id.contains("31b")) {
            caps.context_window = Some(256_000);
        }

        caps
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModelConfig {
    #[serde(default, alias = "model_id")]
    pub model_id: String,
    #[serde(default)]
    pub provider: ModelProvider,
    #[serde(default, alias = "system_prompt")]
    pub system_prompt: Option<String>,
    #[serde(default, skip_serializing)]
    pub api_key: Option<String>,
    #[serde(default, alias = "base_url")]
    pub base_url: Option<String>,
    #[serde(default)]
    pub temperature: Option<f32>,
    #[serde(default, alias = "max_tokens")]
    pub max_tokens: Option<u32>,
    #[serde(default, alias = "top_p")]
    pub top_p: Option<f32>,
    #[serde(default, alias = "external_id")]
    pub external_id: Option<String>,
    #[serde(default)]
    pub rpm: Option<u32>,
    #[serde(default)]
    pub rpd: Option<u32>,
    #[serde(default)]
    pub tpm: Option<u32>,
    #[serde(default)]
    pub tpd: Option<u32>,
    #[serde(default)]
    pub skills: Option<Vec<String>>,
    #[serde(default)]
    pub workflows: Option<Vec<String>>,
    #[serde(default, alias = "mcp_tools")]
    pub mcp_tools: Option<Vec<String>>,
    #[serde(default, alias = "steering_vectors")]
    pub steering_vectors: Option<Vec<String>>,
    #[serde(default, alias = "reasoning_depth")]
    pub reasoning_depth: Option<u32>,
    #[serde(default, alias = "act_threshold")]
    pub act_threshold: Option<f32>,
    #[serde(default, alias = "max_turns")]
    pub max_turns: Option<u32>,
    #[serde(default, alias = "connector_configs")]
    pub connector_configs: Option<Vec<ConnectorConfig>>,
    #[serde(default, alias = "extra_parameters")]
    pub extra_parameters: Option<std::collections::HashMap<String, serde_json::Value>>,
    #[serde(default, alias = "context_window")]
    pub context_window: Option<u32>,
    #[serde(default, alias = "input_cost_per_m")]
    pub input_cost_per_m: Option<f64>,
    #[serde(default, alias = "output_cost_per_m")]
    pub output_cost_per_m: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorConfig {
    pub r#type: String, 
    pub uri: String,    
}

impl ModelConfig {
    pub fn supports_native_tools(&self) -> bool {
        let mid = self.model_id.to_lowercase();
        if mid.contains("phi3") || mid.contains("phi-3") || mid.contains("gemma") {
            return false;
        }
        true
    }

    pub fn merge(&self, other: &Self) -> Self {
        let mut merged = self.clone();

        macro_rules! merge_option {
            ($field:ident) => {
                if merged.$field.is_none() {
                    merged.$field = other.$field.clone();
                }
            };
        }

        merge_option!(system_prompt);
        merge_option!(temperature);
        merge_option!(max_tokens);
        merge_option!(top_p);
        merge_option!(base_url);
        merge_option!(external_id);
        merge_option!(rpm);
        merge_option!(rpd);
        merge_option!(tpm);
        merge_option!(tpd);
        merge_option!(max_turns);
        merge_option!(steering_vectors);
        merge_option!(reasoning_depth);
        merge_option!(act_threshold);

        if let Some(other_extras) = &other.extra_parameters {
            let mut extras = merged.extra_parameters.unwrap_or_default();
            for (k, v) in other_extras {
                extras.entry(k.clone()).or_insert_with(|| v.clone());
            }
            merged.extra_parameters = Some(extras);
        }

        merged
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfig {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
    #[serde(default, skip_serializing, alias = "api_key")]
    pub api_key: Option<String>,
    #[serde(default, alias = "base_url")]
    pub base_url: Option<String>,
    pub protocol: ModelProvider,
    #[serde(default, alias = "external_id")]
    pub external_id: Option<String>,
    #[serde(default, skip_serializing, alias = "custom_headers")]
    pub custom_headers: Option<std::collections::HashMap<String, String>>,
    #[serde(default, alias = "default_config")]
    pub default_config: Option<ModelConfig>,
    #[serde(default, alias = "supports_steering_vectors")]
    pub supports_steering_vectors: bool,
    #[serde(default, alias = "audio_model")]
    pub audio_model: Option<String>,
}

impl Validatable for ProviderConfig {
    fn validate(&self) -> Result<(), String> {
        let name = self.name.trim();
        if name.is_empty() {
            return Err("Provider name cannot be empty".to_string());
        }
        if let Some(url) = &self.base_url {
            if !url.trim().is_empty() && !url.starts_with("http") {
                return Err(format!("Invalid base_url: '{}'. Must start with http:// or https://", url));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModelEntry {
    pub id: String,
    pub name: String,
    #[serde(alias = "provider_id")]
    pub provider_id: String,
    #[serde(default)]
    pub provider: Option<ModelProvider>,
    #[serde(default)]
    pub rpm: Option<u32>,
    #[serde(default)]
    pub tpm: Option<u32>,
    #[serde(default)]
    pub rpd: Option<u32>,
    #[serde(default)]
    pub tpd: Option<u32>,
    #[serde(default)]
    pub modality: Modality,
    #[serde(default)]
    pub capabilities: ModelCapabilities,
    #[serde(default, alias = "last_synced_at")]
    pub last_synced_at: Option<DateTime<Utc>>,
    #[serde(default, alias = "context_window")]
    pub context_window: Option<u32>,
    #[serde(default, alias = "input_cost_per_m")]
    pub input_cost_per_m: Option<f64>,
    #[serde(default, alias = "output_cost_per_m")]
    pub output_cost_per_m: Option<f64>,
}

impl Validatable for ModelEntry {
    fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("Model name cannot be empty".to_string());
        }
        if self.provider_id.trim().is_empty() {
            return Err("Model must be assigned to a Provider ID".to_string());
        }
        if let Some(cost) = self.input_cost_per_m {
            if cost < 0.0 || cost.is_nan() {
                return Err("input_cost_per_m cannot be negative or NaN".to_string());
            }
        }
        if let Some(cost) = self.output_cost_per_m {
            if cost < 0.0 || cost.is_nan() {
                return Err("output_cost_per_m cannot be negative or NaN".to_string());
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, Default, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RoleBlueprint {
    pub id: String,
    pub name: String,
    pub department: String,
    pub description: String,
    #[serde(default)]
    pub skills: String, 
    #[serde(default)]
    pub workflows: String, 
    #[serde(default, alias = "mcpTools")]
    pub mcp_tools: String, 
    #[serde(default, alias = "requiresOversight")]
    pub requires_oversight: bool,
    #[serde(default, alias = "modelId")]
    pub model_id: Option<String>,
    #[serde(default, alias = "createdAt")]
    pub created_at: Option<DateTime<Utc>>,
}

impl Validatable for RoleBlueprint {
    fn validate(&self) -> Result<(), String> {
        if self.id.trim().is_empty() {
            return Err("Blueprint ID cannot be empty".to_string());
        }
        if self.name.trim().is_empty() {
            return Err("Blueprint name cannot be empty".to_string());
        }
        Ok(())
    }
}


// Metadata: [model]

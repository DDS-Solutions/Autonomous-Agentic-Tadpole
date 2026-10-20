//! @docs ARCHITECTURE:Persistence
//!
//! ### AI Assist Note
//! Provider and model disk catalogs preserve credential redaction and environment precedence.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Storage failure, invalid input, or ownership conflict.
//! - **Telemetry Link**: Search `[Persistence]` in server logs.

use crate::agent::types::{ModelEntry, ProviderConfig};
use crate::error::AppError;

const PROVIDERS_FILE: &str = "data/infra_providers.json";
const MODELS_FILE: &str = "data/infra_models.json";

async fn read_json_file<T: serde::de::DeserializeOwned>(path: &std::path::Path) -> Option<T> {
    let content = tokio::fs::read_to_string(path).await.ok()?;
    serde_json::from_str(&content).ok()
}

/// ### 🔒 SEC-02: Credential Polarization Helper
/// Maps a protocol-specific provider ID to its canonical environment variable.
/// 
/// Returns `None` for providers that do not require secret keys or utilize
/// alternative authentication mechanisms (e.g., local Ollama instance).
fn provider_env_var(provider_id: &str) -> Option<&'static str> {
    match provider_id {
        "google" | "gemini" => Some("GOOGLE_API_KEY"),
        "groq" => Some("GROQ_API_KEY"),
        "openai" => Some("OPENAI_API_KEY"),
        "anthropic" => Some("ANTHROPIC_API_KEY"),
        "inception" => Some("INCEPTION_API_KEY"),
        "deepseek" => Some("DEEPSEEK_API_KEY"),
        _ => None,
    }
}

/// Loads provider configurations from disk and overlays security context.
/// 
/// ### 🔒 SEC-02: Credential Polarization
/// API keys are NEVER loaded from the raw JSON file if a corresponding environment 
/// variable (e.g., `GOOGLE_API_KEY`) is present. Environment variables are treated 
/// as the Sovereign Root of Truth for credentials.
pub async fn load_providers(base_dir: &std::path::Path) -> Vec<ProviderConfig> {
    let providers_file = match crate::utils::security::validate_path(base_dir, PROVIDERS_FILE) {
        Ok(path) => path,
        Err(err) => {
            tracing::error!("⚠️ [Persistence] Security boundary violation validating providers file: {}", err);
            return crate::agent::registry::get_default_providers();
        }
    };
    let mut providers = if providers_file.exists() {
        read_json_file::<Vec<ProviderConfig>>(&providers_file).await.unwrap_or_else(|| {
            tracing::error!(
                file = ?providers_file,
                "❌ [Persistence] Provider JSON parse failure — falling back to defaults"
            );
            crate::agent::registry::get_default_providers()
        })
    } else {
        // Fallback: Check RESOURCE_ROOT (e.g. bundled data)
        let resource_root = std::env::var("RESOURCE_ROOT").unwrap_or_else(|_| ".".to_string());
        let bundled_file = std::path::Path::new(&resource_root).join(PROVIDERS_FILE);
        read_json_file::<Vec<ProviderConfig>>(&bundled_file).await.unwrap_or_else(crate::agent::registry::get_default_providers)
    };

    // SEC-02: Override api_key from environment variables.
    for provider in &mut providers {
        if let Some(env_var) = provider_env_var(&provider.id) {
            if let Ok(key) = std::env::var(env_var) {
                if !key.trim().is_empty() {
                    provider.api_key = Some(key);
                }
            }
        }
    }

    providers
}

/// ### 🔒 SEC-02: Credential Redaction Pass
/// Persists provider configurations to disk after sanitizing sensitive tokens.
/// 
/// ### 🛰️ Security Note: Identity Leakage Prevention
/// API keys are stripped and replaced with `serde_json::Value::Null` before disk write.
/// This ensures that repo-wide exports or cloud backups do not accidentally contain
/// the `NEURAL_TOKEN` or cloud provider secrets.
pub async fn save_providers(
    base_dir: &std::path::Path,
    providers: Vec<ProviderConfig>,
) -> Result<(), AppError> {
    let providers_file = crate::utils::security::validate_path(base_dir, PROVIDERS_FILE)?;
    let sanitized: Vec<serde_json::Value> = providers
        .iter()
        .map(|p| {
            let mut val = serde_json::to_value(p).unwrap_or_default();
            if let Some(obj) = val.as_object_mut() {
                obj.insert("api_key".to_string(), serde_json::Value::Null);
                obj.remove("apiKey");
            }
            val
        })
        .collect();
    let content = serde_json::to_string_pretty(&sanitized)?;
    tokio::fs::write(providers_file, content)
        .await
        .map_err(AppError::Io)?;
    Ok(())
}

/// Computes a capability score for a model to establish prioritization.
/// Priority weights align with Tadpole OS autonomous agentic requirements:
/// - Tools support (1,000,000)
/// - Agentic recommended (500,000)
/// - Structured output (250,000)
/// - Reasoning capability (200,000)
/// - Multimodal vision (50,000)
/// - Context window size (1 point per 1k tokens)
pub fn model_capability_score(m: &ModelEntry) -> u64 {
    let mut score = 0u64;
    if m.capabilities.supports_tools {
        score += 1_000_000;
    }
    if m.capabilities.is_agentic_recommended {
        score += 500_000;
    }
    if m.capabilities.supports_structured_output {
        score += 250_000;
    }
    if m.capabilities.supports_reasoning {
        score += 200_000;
    }
    if m.capabilities.supports_vision {
        score += 50_000;
    }
    score += (m.capabilities.context_window.unwrap_or(32_768) as u64) / 1000;
    score
}

/// Sorts models with Local (Ollama) providers first, ranked by capability requirements,
/// followed by Cloud providers, ranked by capability requirements.
pub fn sort_models_local_first(models: &mut [ModelEntry]) {
    models.sort_by(|a, b| {
        let is_a_local = a.provider_id.eq_ignore_ascii_case("ollama")
            || matches!(a.provider, Some(crate::agent::types::ModelProvider::Ollama));
        let is_b_local = b.provider_id.eq_ignore_ascii_case("ollama")
            || matches!(b.provider, Some(crate::agent::types::ModelProvider::Ollama));

        // 1. Local first
        if is_a_local != is_b_local {
            return is_b_local.cmp(&is_a_local); // true before false
        }

        // 2. Capability score comparison (higher score first)
        let score_a = model_capability_score(a);
        let score_b = model_capability_score(b);
        if score_a != score_b {
            return score_b.cmp(&score_a);
        }

        // 3. Stable tie-breaker on model id
        a.id.cmp(&b.id)
    });
}

/// Loads the model registry from disk.
pub async fn load_models(base_dir: &std::path::Path) -> Vec<ModelEntry> {
    let models_file = match crate::utils::security::validate_path(base_dir, MODELS_FILE) {
        Ok(path) => path,
        Err(err) => {
            tracing::error!("⚠️ [Persistence] Security boundary violation validating models file: {}", err);
            return crate::agent::registry::get_default_models();
        }
    };
    let mut models = if models_file.exists() {
        if let Some(models) = read_json_file::<Vec<ModelEntry>>(&models_file).await {
            models
        } else {
            tracing::error!(file = ?models_file, "❌ [Persistence] Model JSON parse failure — falling back to defaults");
            crate::agent::registry::get_default_models()
        }
    } else {
        // Fallback: Check RESOURCE_ROOT
        let resource_root = std::env::var("RESOURCE_ROOT").unwrap_or_else(|_| ".".to_string());
        let bundled_file = std::path::Path::new(&resource_root).join(MODELS_FILE);
        if let Some(models) = read_json_file::<Vec<ModelEntry>>(&bundled_file).await {
            models
        } else {
            crate::agent::registry::get_default_models()
        }
    };

    sort_models_local_first(&mut models);
    models
}

/// Persists all model entries to disk with deterministic local-first capability ordering.
pub async fn save_models(base_dir: &std::path::Path, mut models: Vec<ModelEntry>) -> Result<(), AppError> {
    let models_file = crate::utils::security::validate_path(base_dir, MODELS_FILE)?;
    sort_models_local_first(&mut models);
    let content = serde_json::to_string_pretty(&models)?;
    tokio::fs::write(models_file, content)
        .await
        .map_err(AppError::Io)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::types::{ModelCapabilities, ModelEntry, ModelProvider};

    #[test]
    fn test_sort_models_local_first() {
        let mut models = vec![
            ModelEntry {
                id: "gpt-4o".to_string(),
                provider_id: "openai".to_string(),
                provider: Some(ModelProvider::Openai),
                capabilities: ModelCapabilities {
                    supports_tools: true,
                    is_agentic_recommended: true,
                    supports_structured_output: true,
                    context_window: Some(128_000),
                    ..Default::default()
                },
                ..Default::default()
            },
            ModelEntry {
                id: "phi3.5:latest".to_string(),
                provider_id: "ollama".to_string(),
                provider: Some(ModelProvider::Ollama),
                capabilities: ModelCapabilities {
                    supports_tools: false,
                    context_window: Some(32_768),
                    ..Default::default()
                },
                ..Default::default()
            },
            ModelEntry {
                id: "gemma4:26b".to_string(),
                provider_id: "ollama".to_string(),
                provider: Some(ModelProvider::Ollama),
                capabilities: ModelCapabilities {
                    supports_tools: true,
                    is_agentic_recommended: true,
                    supports_structured_output: true,
                    context_window: Some(128_000),
                    ..Default::default()
                },
                ..Default::default()
            },
        ];

        sort_models_local_first(&mut models);

        // Assert Local comes first
        assert_eq!(models[0].id, "gemma4:26b");
        assert_eq!(models[1].id, "phi3.5:latest");
        // Cloud comes last
        assert_eq!(models[2].id, "gpt-4o");
    }
}

// Metadata: [catalog]

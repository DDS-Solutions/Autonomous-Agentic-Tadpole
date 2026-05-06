//! @docs ARCHITECTURE:Agent
//!
//! ### AI Assist Note
//! **Discovery Hub**: Provides the primary interface for infrastructure
//! discovery and baseline configuration. While production models are
//! externalized to `data/*.json`, this module remains the logic-anchor
//! for **Infrastructure Fallbacks**.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Missing or empty default provider lists, causing
//!   agent initialization to stall.
//! - **Trace Scope**: `server-rs::agent::registry`

use crate::agent::types::{ModelEntry, ProviderConfig};

/// Returns the exhaustive list of supported LLM providers.
///
/// # Note
/// This is now a legacy fallback. System providers should be loaded from
/// `data/infra_providers.json` via the persistence module.
pub fn get_default_providers() -> Vec<ProviderConfig> {
    vec![
        ProviderConfig {
            id: "ollama".to_string(),
            name: "Ollama (Local)".to_string(),
            icon: Some("🦙".to_string()),
            base_url: Some("http://127.0.0.1:11434".to_string()),
            protocol: crate::agent::types::ModelProvider::Ollama,
            ..ProviderConfig::default()
        }
    ]
}


/// Returns the exhaustive list of supported LLM models.
///
/// # Note
/// This is now a legacy fallback. System models should be loaded from
/// `data/infra_models.json` via the persistence module.
pub fn get_default_models() -> Vec<ModelEntry> {
    vec![
        ModelEntry {
            id: "gemma4:e4b".to_string(),
            name: "Gemma 4 (e4b)".to_string(),
            provider_id: "ollama".to_string(),
            provider: Some(crate::agent::types::ModelProvider::Ollama),
            modality: crate::agent::types::Modality::Llm,
            ..ModelEntry::default()
        },
        ModelEntry {
            id: "llama3:8b".to_string(),
            name: "Llama 3 (8B)".to_string(),
            provider_id: "ollama".to_string(),
            provider: Some(crate::agent::types::ModelProvider::Ollama),
            modality: crate::agent::types::Modality::Llm,
            ..ModelEntry::default()
        },
        ModelEntry {
            id: "gpt-4o".to_string(),
            name: "GPT-4o (Standard)".to_string(),
            provider_id: "openai".to_string(),
            provider: Some(crate::agent::types::ModelProvider::Openai),
            modality: crate::agent::types::Modality::Llm,
            ..ModelEntry::default()
        },
        ModelEntry {
            id: "gemini-1.5-pro".to_string(),
            name: "Gemini 1.5 Pro".to_string(),
            provider_id: "google".to_string(),
            provider: Some(crate::agent::types::ModelProvider::Google),
            modality: crate::agent::types::Modality::Llm,
            ..ModelEntry::default()
        },
    ]
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_providers() {
        let providers = get_default_providers();
        assert!(!providers.is_empty());
        assert!(providers.iter().any(|p| p.id == "ollama"));
    }

    #[test]
    fn test_default_models() {
        let models = get_default_models();
        assert!(!models.is_empty());
        assert!(models.iter().any(|m| m.id == "gemma4:e4b"));
        assert!(models.iter().any(|m| m.id == "gpt-4o"));
    }
}

// Metadata: [registry]

// Metadata: [registry]

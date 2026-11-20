//! @docs ARCHITECTURE:Agent
//!
//! ### AI Assist Note
//! **Model Manager (LLM Orchestrator)**: Orchestrates the AI provider
//! configurations and model lifecycle for the Tadpole OS engine.
//! Features **IMR-01 (Intelligent Model Registry)**: provides automated
//! discovery and synchronization of models from remote providers (Ollama,
//! OpenAI, Groq, etc.).
//! Features **Provider Handshake Validation**: provides a
//! connectivity test mechanism (`test_provider`) to verify API keys
//! and endpoint reactivity across diverse protocols.
//! Implements **Secure Secret Redaction**: ensures that `api_key`
//! values are stripped from frontend responses while maintaining
//! functional status indicators (`has_api_key`).
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: 401 Unauthorized during provider handshakes
//!   due to malformed keys, 400 Bad Request for insecure HTTP
//!   transmissions to external endpoints (SEC-01), or dependency
//!   orphaning if models are not decommissioned alongside providers.
//! - **Telemetry Link**: Search `[model_manager]` in tracing logs.
//!   in `tracing` logs for infrastructure audit events.
//! - **Trace Scope**: `server-rs::routes::model_manager`

use crate::{
    agent::{
        capability_matrix::CapabilityMatrix,
        types::{ModelEntry, ProviderConfig, Validatable},
    },
    error::AppError,
    state::AppState,
};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use std::sync::Arc;

const ANTHROPIC_FALLBACK_MODELS: &[&str] = &[
    "claude-3-7-sonnet-20250219",
    "claude-3-5-sonnet-20241022",
    "claude-3-5-haiku-20241022",
    "claude-3-opus-20240229",
];

fn anthropic_fallback_models() -> Vec<String> {
    ANTHROPIC_FALLBACK_MODELS
        .iter()
        .map(|s| (*s).to_string())
        .collect()
}

#[derive(serde::Serialize)]
pub struct ProviderResponse {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
    pub base_url: Option<String>,
    pub protocol: crate::agent::types::ModelProvider,
    pub external_id: Option<String>,
    pub supports_steering_vectors: bool,
    pub audio_model: Option<String>,
    pub api_key: Option<String>,
    pub has_api_key: bool,
}

impl From<&ProviderConfig> for ProviderResponse {
    fn from(config: &ProviderConfig) -> Self {
        let has_key = config
            .api_key
            .as_ref()
            .is_some_and(|s| !s.trim().is_empty());
        Self {
            id: config.id.clone(),
            name: config.name.clone(),
            icon: config.icon.clone(),
            base_url: config.base_url.clone(),
            protocol: config.protocol,
            external_id: config.external_id.clone(),
            supports_steering_vectors: config.supports_steering_vectors,
            audio_model: config.audio_model.clone(),
            api_key: None,
            has_api_key: has_key,
        }
    }
}

pub fn get_ollama_host() -> String {
    std::env::var("OLLAMA_HOST").unwrap_or_else(|_| "http://127.0.0.1:11434".to_string())
}

pub fn is_secure_local(url_str: &str) -> bool {
    let Ok(parsed) = reqwest::Url::parse(url_str) else {
        return false;
    };
    if !matches!(parsed.scheme(), "http" | "https") {
        return false;
    }
    if let Some(host) = parsed.host_str() {
        let stripped = host.trim_start_matches('[').trim_end_matches(']');
        if stripped.eq_ignore_ascii_case("localhost")
            || stripped == "127.0.0.1"
            || stripped == "::1"
            || stripped.eq_ignore_ascii_case("host.docker.internal")
        {
            return true;
        }
        if let Ok(ip) = stripped.parse::<std::net::IpAddr>() {
            return match ip {
                std::net::IpAddr::V4(v4) => v4.is_loopback() || v4.is_private(),
                std::net::IpAddr::V6(v6) => v6.is_loopback(),
            };
        }
    }
    false
}

/// SEC-01: Validates a provider endpoint URL and blocks transmission of credentials over unencrypted HTTP
/// to external (non-loopback/non-RFC1918) hosts unless `TADPOLE_ALLOW_LOCAL_HTTP` is explicitly set.
pub fn validate_provider_transport_url(url: &str, enforce_tls_guard: bool) -> Result<(), AppError> {
    let parsed_url = reqwest::Url::parse(url)
        .map_err(|e| AppError::BadRequest(format!("Malformed provider URL: {}", e)))?;
    if !enforce_tls_guard {
        return Ok(());
    }
    let secure_local = is_secure_local(url);
    let allow_insecure = std::env::var("TADPOLE_ALLOW_LOCAL_HTTP").is_ok();

    if parsed_url.scheme() == "http" && !secure_local && !allow_insecure {
        return Err(AppError::BadRequest(
            "Insecure transmission blocked: API keys cannot be sent over HTTP to external providers. Use HTTPS."
                .to_string(),
        ));
    }
    Ok(())
}

/// Enriches a `ModelEntry` with inferred capabilities and normalizes cloud pricing safeguards.
pub(crate) fn enrich_and_normalize_model_entry(entry: &mut ModelEntry, id: &str) {
    entry.id = id.to_string();
    if let Some(cw) = entry.context_window {
        entry.capabilities.context_window = Some(cw);
    } else if entry.capabilities.context_window.unwrap_or(0) == 0 {
        entry.capabilities = CapabilityMatrix::infer_capabilities(id);
    }

    // Safeguard: Normalize 0.0 custom pricing on cloud models to None
    // so catalog pricing is preserved and paid models are never inadvertently zeroed out.
    let lower_id = id.to_ascii_lowercase();
    let is_local_provider = entry.provider_id.eq_ignore_ascii_case("ollama")
        || entry.provider_id.eq_ignore_ascii_case("local")
        || lower_id.starts_with("ollama")
        || lower_id.starts_with("local/");
    if !is_local_provider {
        if entry.input_cost_per_m == Some(0.0) {
            entry.input_cost_per_m = None;
        }
        if entry.output_cost_per_m == Some(0.0) {
            entry.output_cost_per_m = None;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProtocolRouter {
    Anthropic,
    Google,
    OpenAI,
    Ollama,
}

impl ProtocolRouter {
    fn from_protocol(protocol: &str) -> Self {
        match protocol {
            "anthropic" => Self::Anthropic,
            "google" | "gemini" => Self::Google,
            "ollama" => Self::Ollama,
            _ => Self::OpenAI, // Default for standard open-compatible endpoints
        }
    }

    fn build_models_url(&self, protocol: &str, base_url: &str, is_sync: bool) -> String {
        match self {
            Self::Anthropic => format!(
                "{}/v1/models",
                if base_url.is_empty() {
                    "https://api.anthropic.com"
                } else {
                    base_url.trim_end_matches('/')
                }
            ),
            Self::Google => {
                let base = if base_url.is_empty() {
                    "https://generativelanguage.googleapis.com".to_string()
                } else {
                    base_url.trim_end_matches('/').to_string()
                };
                if is_sync {
                    format!("{}/v1/models", base)
                } else {
                    let mut b = base;
                    if !b.contains("/v1") && !b.contains("/beta") {
                        b.push_str("/v1");
                    }
                    format!("{}/models", b)
                }
            }
            Self::Ollama => {
                let base = if base_url.is_empty() {
                    get_ollama_host()
                } else {
                    base_url.trim_end_matches('/').to_string()
                };
                let base = if !base.contains("/api") && !base.contains("/v1") {
                    format!("{}/api", base)
                } else {
                    base
                };
                if is_sync && !base.contains("/v1") {
                    format!("{}/tags", base)
                } else {
                    format!("{}/models", base)
                }
            }
            Self::OpenAI => format!(
                "{}/models",
                if base_url.is_empty() {
                    match protocol {
                        "groq" => "https://api.groq.com/openai/v1",
                        _ => "https://api.openai.com/v1",
                    }
                } else {
                    base_url.trim_end_matches('/')
                }
            ),
        }
    }

    fn authenticate_request(
        &self,
        req: reqwest::RequestBuilder,
        api_key: &str,
    ) -> reqwest::RequestBuilder {
        match self {
            Self::Google => req.header("x-goog-api-key", api_key),
            Self::Anthropic => req
                .header("x-api-key", api_key)
                .header("anthropic-version", "2023-06-01"),
            Self::Ollama => {
                if !api_key.is_empty() {
                    req.header("Authorization", format!("Bearer {}", api_key))
                } else {
                    req
                }
            }
            Self::OpenAI => req.header("Authorization", format!("Bearer {}", api_key)),
        }
    }

    fn parse_discovery_response(&self, body: &serde_json::Value, url: &str) -> Vec<String> {
        let mut discovered_ids = Vec::new();
        match self {
            Self::Ollama if url.contains("/tags") => {
                if let Some(models) = body.get("models").and_then(|m| m.as_array()) {
                    for m in models {
                        if let Some(name) = m.get("name").and_then(|n| n.as_str()) {
                            discovered_ids.push(name.to_string());
                        }
                    }
                }
            }
            Self::Google => {
                if let Some(models) = body.get("models").and_then(|m| m.as_array()) {
                    for m in models {
                        if let Some(name) = m.get("name").and_then(|n| n.as_str()) {
                            discovered_ids.push(name.replace("models/", ""));
                        }
                    }
                }
            }
            _ => {
                // OpenAI standard
                if let Some(data) = body.get("data").and_then(|d| d.as_array()) {
                    for m in data {
                        if let Some(id) = m.get("id").and_then(|i| i.as_str()) {
                            discovered_ids.push(id.to_string());
                        }
                    }
                }
            }
        }
        discovered_ids
    }
}

/// Returns all configured AI providers.
/// SEC-02: Strips `api_key` from the response so secrets are never sent to the frontend.
#[tracing::instrument(skip(state), name = "infra_providers::list")]
pub async fn get_providers(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    let sanitized: Vec<ProviderResponse> = state
        .registry
        .providers
        .iter()
        .map(|kv| ProviderResponse::from(kv.value()))
        .collect();
    Ok(Json(sanitized))
}

/// Updates or creates a provider configuration.
#[tracing::instrument(skip(state, config), fields(provider_id = %id), name = "infra_providers::update")]
pub async fn update_provider(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(config): Json<ProviderConfig>,
) -> Result<impl IntoResponse, AppError> {
    if let Err(e) = config.validate() {
        return Err(AppError::BadRequest(e));
    }
    state.registry.providers.insert(id.clone(), config);
    state.save_providers().await;
    Ok((
        StatusCode::OK,
        Json(serde_json::json!({ "status": "updated", "id": id })),
    ))
}

/// Deletes a provider configuration.
#[tracing::instrument(skip(state), fields(provider_id = %id), name = "infra_providers::delete")]
pub async fn delete_provider(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    tracing::info!("🗑️ [infra-trace] Deleting provider: {}", id);
    let removed = state.registry.providers.remove(&id);
    if removed.is_some() {
        tracing::info!("✅ [infra-trace] Successfully removed provider: {}", id);

        // Also remove all models associated with this provider
        let initial_count = state.registry.models.len();
        state
            .registry
            .models
            .retain(|_, model| model.provider_id != id);
        let removed_count = initial_count - state.registry.models.len();
        if removed_count > 0 {
            tracing::info!(
                "🗑️ [infra-trace] Decommissioned {} associated models for provider: {}",
                removed_count,
                id
            );
        }

        // Save both registries
        state.save_providers().await;
        state.save_models().await;

        Ok((
            StatusCode::OK,
            Json(serde_json::json!({ "status": "deleted", "id": id })),
        ))
    } else {
        tracing::warn!("⚠️ [infra-trace] Provider not found for deletion: {}", id);
        Err(AppError::NotFound(format!("Provider '{}' not found", id)))
    }
}

/// Returns all available agent models.
#[tracing::instrument(skip(state), name = "infra_models::list")]
pub async fn get_models(State(state): State<Arc<AppState>>) -> Result<impl IntoResponse, AppError> {
    let models: Vec<ModelEntry> = state
        .registry
        .models
        .iter()
        .map(|kv| {
            let mut entry = kv.value().clone();
            if entry.capabilities.context_window.unwrap_or(0) == 0 {
                entry.capabilities = CapabilityMatrix::infer_capabilities(&entry.id);
            }
            if entry.context_window.is_none() {
                entry.context_window = entry.capabilities.context_window;
            }
            entry
        })
        .collect();
    Ok(Json(models))
}

/// Updates or creates a model entry.
#[tracing::instrument(skip(state, entry), fields(model_id = %id), name = "infra_models::update")]
pub async fn update_model(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(mut entry): Json<ModelEntry>,
) -> Result<impl IntoResponse, AppError> {
    if let Err(e) = entry.validate() {
        return Err(AppError::BadRequest(e));
    }

    // Verify provider existence for swarm consistency
    if !state.registry.providers.contains_key(&entry.provider_id) {
        return Err(AppError::BadRequest(format!(
            "Provider '{}' is not registered in the neural infrastructure.",
            entry.provider_id
        )));
    }

    enrich_and_normalize_model_entry(&mut entry, &id);

    tracing::info!(
        "🔄 [infra-trace] Model updated: id={}, context_window={:?}, input_cost_per_m={:?}, output_cost_per_m={:?}",
        id, entry.context_window, entry.input_cost_per_m, entry.output_cost_per_m
    );

    state.registry.models.insert(id.clone(), entry);
    state.save_models().await;
    Ok((
        StatusCode::OK,
        Json(serde_json::json!({ "status": "updated", "id": id })),
    ))
}

/// Deletes a model from the registry.
#[tracing::instrument(skip(state), fields(model_id = %id), name = "infra_models::delete")]
pub async fn delete_model(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    tracing::info!("🗑️ [infra-trace] Deleting model: {}", id);
    let removed = state.registry.models.remove(&id);
    if removed.is_some() {
        tracing::info!("✅ [infra-trace] Successfully removed model: {}", id);
        state.save_models().await;
        Ok((
            StatusCode::OK,
            Json(serde_json::json!({ "status": "deleted", "id": id })),
        ))
    } else {
        tracing::warn!("⚠️ [infra-trace] Model not found for deletion: {}", id);
        Err(AppError::NotFound(format!("Model '{}' not found", id)))
    }
}

/// Tests a provider configuration by performing a dummy model list or balance check.
#[tracing::instrument(skip(state, config), fields(provider_id = %id), name = "infra_providers::test")]
pub async fn test_provider(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(config): Json<ProviderConfig>,
) -> Result<impl IntoResponse, AppError> {
    tracing::info!("🔍 [Test Trace] Initiating handshake for provider: {}", id);

    let stored_provider = state.registry.providers.get(&id).map(|p| p.clone());
    let supplied_key = config.api_key.filter(|k| !k.is_empty());
    let using_stored_key = supplied_key.is_none();
    let api_key = supplied_key
        .or_else(|| stored_provider.as_ref().and_then(|p| p.api_key.clone()))
        .unwrap_or_default();
    let stored_base = stored_provider
        .as_ref()
        .and_then(|p| p.base_url.as_deref())
        .unwrap_or("")
        .trim()
        .trim_end_matches('/');
    let requested_base = config
        .base_url
        .as_deref()
        .unwrap_or("")
        .trim()
        .trim_end_matches('/');
    let protocol = config.protocol.to_string().to_lowercase();

    if api_key.is_empty() {
        return Err(AppError::BadRequest(
            "Missing Secure API Key. handshakes require authentication.".to_string(),
        ));
    }

    // SEC-EXFIL: Never transmit a stored server-side API key to a caller-modified base_url.
    // When testing with a stored key and an omitted base_url, fall back to the stored provider's
    // configured base_url so self-hosted/custom keys are never redirected to a public default host.
    let effective_base_url = if using_stored_key {
        if !requested_base.is_empty() && !requested_base.eq_ignore_ascii_case(stored_base) {
            return Err(AppError::Forbidden(
                "Refusing to send stored provider credentials to a modified base_url. Supply an explicit api_key or update the provider configuration first.".to_string(),
            ));
        }
        if requested_base.is_empty() {
            stored_base
        } else {
            requested_base
        }
    } else {
        requested_base
    };

    let router = ProtocolRouter::from_protocol(&protocol);
    let url = router.build_models_url(&protocol, effective_base_url, false);

    // SEC-01: Prevent transmission of API keys over unencrypted HTTP for external endpoints.
    validate_provider_transport_url(&url, true)?;

    // SEC-PRIVACY: Respect cluster privacy_mode using parsed URL host validation (not raw prefix matching).
    if state
        .governance
        .privacy_mode
        .load(std::sync::atomic::Ordering::SeqCst)
        && !is_secure_local(&url)
    {
        return Err(AppError::Forbidden(
            "Privacy Mode is active: outbound external provider handshakes are blocked."
                .to_string(),
        ));
    }

    let request = state.resources.http_client.get(&url);
    let request = router.authenticate_request(request, &api_key);

    match request.send().await {
        Ok(res) => {
            if res.status().is_success() {
                tracing::info!("✅ [Test Trace] Handshake successful for {}", id);
                Ok((
                    StatusCode::OK,
                    Json(serde_json::json!({
                        "status": "success",
                        "message": format!("Handshake successful: {} endpoint is reactive.", config.name)
                    })),
                ))
            } else {
                let err_text = res
                    .text()
                    .await
                    .unwrap_or_else(|_| "Unknown error".to_string());
                let safe_err = state.security.secret_redactor.redact(&err_text);
                tracing::warn!("❌ [Test Trace] Handshake failed for {}: {}", id, safe_err);
                Err(AppError::Unauthorized(format!(
                    "Handshake failed: {}",
                    safe_err
                )))
            }
        }
        Err(e) => {
            let safe_err = state.security.secret_redactor.redact(&format!("{}", e));
            tracing::error!("🚨 [Test Trace] Network error for {}: {}", id, safe_err);
            Err(AppError::InternalServerError(format!(
                "Network Error: {}",
                safe_err
            )))
        }
    }
}

/// Dynamic synchronization of models from a provider's discovery endpoint.
/// Part of IMR-01.
#[tracing::instrument(skip(state), fields(provider_id = %id), name = "infra_providers::sync")]
pub async fn sync_provider_models(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    tracing::info!("🔄 [IMR] Synchronizing models for provider: {}", id);

    // Scope DashMap read guard so it is dropped before any network or disk .await
    let (api_key, base_url, provider_protocol, protocol) = {
        let provider = state
            .registry
            .providers
            .get(&id)
            .ok_or_else(|| AppError::NotFound(format!("Provider '{}' not found", id)))?;
        (
            provider.api_key.as_deref().unwrap_or("").to_string(),
            provider.base_url.as_deref().unwrap_or("").to_string(),
            provider.protocol,
            provider.protocol.to_string().to_lowercase(),
        )
    };

    // 1. Resolve URL and Authenticate via Router
    let router = ProtocolRouter::from_protocol(&protocol);
    let url = router.build_models_url(&protocol, &base_url, true);

    // SEC-01: Prevent transmission of API keys over unencrypted HTTP for external endpoints.
    validate_provider_transport_url(&url, !api_key.is_empty())?;

    let request = state
        .resources
        .http_client
        .get(&url)
        .timeout(std::time::Duration::from_secs(10));
    let request = router.authenticate_request(request, &api_key);

    let discovered_ids = match request.send().await {
        Ok(resp) if resp.status().is_success() => {
            if let Ok(body) = resp.json::<serde_json::Value>().await {
                let ids = router.parse_discovery_response(&body, &url);
                if ids.is_empty() && protocol == "anthropic" {
                    anthropic_fallback_models()
                } else {
                    ids
                }
            } else if protocol == "anthropic" {
                anthropic_fallback_models()
            } else {
                return Err(AppError::InternalServerError(
                    "Failed to parse discovery response.".to_string(),
                ));
            }
        }
        _ if protocol == "anthropic" => anthropic_fallback_models(),
        Ok(resp) => {
            return Err(AppError::Unauthorized(format!(
                "Discovery failed ({}): Provider rejected credentials or endpoint unreachable.",
                resp.status()
            )));
        }
        Err(e) => {
            return Err(AppError::InternalServerError(format!(
                "Handshake failed during discovery: {}",
                e
            )));
        }
    };

    let count = discovered_ids.len();
    let mut added = 0;
    let mut modified = false;

    // 3. Register and Enrich
    for model_id in discovered_ids {
        let entry_id = format!("{}:{}", id, model_id.replace('/', "-"));
        let capabilities = CapabilityMatrix::infer_capabilities(&model_id);

        if let Some(mut existing) = state.registry.models.get_mut(&entry_id) {
            existing.last_synced_at = Some(chrono::Utc::now());
            if existing.capabilities.source != "manual" {
                existing.capabilities = capabilities;
            }
            modified = true;
        } else {
            let entry = ModelEntry {
                id: model_id.clone(),
                name: model_id,
                provider_id: id.clone(),
                provider: Some(provider_protocol),
                modality: if capabilities.supports_vision {
                    crate::agent::types::Modality::Vision
                } else {
                    crate::agent::types::Modality::Llm
                },
                capabilities,
                last_synced_at: Some(chrono::Utc::now()),
                ..ModelEntry::default()
            };
            state.registry.models.insert(entry_id, entry);
            added += 1;
            modified = true;
        }
    }

    if modified {
        state.save_models().await;
    }

    tracing::info!(
        "🔄 [ModelManager] Synced provider '{}': {} discovered, {} added.",
        id,
        count,
        added
    );

    Ok(Json(serde_json::json!({
        "status": "ok",
        "provider_id": id,
        "discovered": count,
        "added": added
    })))
}

/// Returns a curated catalog of recommended models for the local swarm.
pub async fn get_model_catalog(
    State(_state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    let catalog = serde_json::json!([
        {
            "id": "llama3.3:70b",
            "name": "Llama 3.3 (70B)",
            "provider": "ollama",
            "description": "Meta's flagship open model. Exceptional reasoning and coding.",
            "size": "42GB",
            "vram": "48GB",
            "tags": ["Flagship", "Reasoning", "Coding"]
        },
        {
            "id": "qwen2.5-coder:7b",
            "name": "Qwen 2.5 Coder (7B)",
            "provider": "ollama",
            "description": "Alibaba's specialized code synthesis and refactoring model.",
            "size": "4.7GB",
            "vram": "8GB",
            "tags": ["Coding", "Fast"]
        },
        {
            "id": "llama3:8b",
            "name": "Llama 3 (8B)",
            "provider": "ollama",
            "description": "Meta's balanced 8B model for lightweight local inference.",
            "size": "4.7GB",
            "vram": "8GB",
            "tags": ["General", "Logic"]
        },
        {
            "id": "phi3:latest",
            "name": "Phi-3 Mini",
            "provider": "ollama",
            "description": "Microsoft's high-performance 3.8B small language model.",
            "size": "2.3GB",
            "vram": "4GB",
            "tags": ["Fast", "Efficiency"]
        },
        {
            "id": "nomic-embed-text:latest",
            "name": "Nomic Embed",
            "provider": "ollama",
            "description": "High-fidelity text embeddings for RAG and Vector Memory.",
            "size": "274MB",
            "vram": "1GB",
            "tags": ["RAG", "Embeddings"]
        }
    ]);

    Ok(Json(catalog))
}

#[derive(serde::Deserialize)]
pub struct PullModelPayload {
    pub node_id: String,
    pub tag: String,
}

/// Validates whether a peer URL is trusted to receive Admin-scoped cluster pull credentials.
/// Only loopback targets (`localhost`, `127.0.0.1`, `[::1]`, or IPv4/IPv6 loopback) are trusted
/// by default; non-loopback peers (including unauthenticated mDNS-discovered or simulated LAN nodes)
/// MUST be explicitly allowlisted in `SWARM_TRUSTED_PEERS` and satisfy either `https` or `is_secure_local`.
pub fn is_trusted_pull_peer_url(url_str: &str, trusted_peers_csv: Option<&str>) -> bool {
    let Ok(parsed) = reqwest::Url::parse(url_str) else {
        return false;
    };
    if !matches!(parsed.scheme(), "http" | "https") {
        return false;
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return false;
    }
    let Some(raw_host) = parsed.host_str() else {
        return false;
    };
    let stripped_host = raw_host.trim_start_matches('[').trim_end_matches(']');

    if stripped_host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    if let Ok(ip) = stripped_host.parse::<std::net::IpAddr>() {
        if ip.is_loopback() {
            return true;
        }
    }

    let Some(csv) = trusted_peers_csv else {
        return false;
    };
    let host_port = match parsed.port() {
        Some(port) => format!("{}:{}", raw_host, port),
        None => raw_host.to_string(),
    };
    let host_port_stripped = match parsed.port() {
        Some(port) => format!("{}:{}", stripped_host, port),
        None => stripped_host.to_string(),
    };
    let explicitly_trusted = csv
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .any(|entry| {
            let normalized = entry
                .trim_start_matches("http://")
                .trim_start_matches("https://")
                .trim_end_matches('/');
            normalized.eq_ignore_ascii_case(raw_host)
                || normalized.eq_ignore_ascii_case(stripped_host)
                || normalized.eq_ignore_ascii_case(&host_port)
                || normalized.eq_ignore_ascii_case(&host_port_stripped)
        });

    if !explicitly_trusted {
        return false;
    }

    parsed.scheme() == "https" || is_secure_local(url_str)
}

/// Proxies a model pull request to a specific swarm node.
pub async fn pull_model(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<PullModelPayload>,
) -> Result<impl IntoResponse, AppError> {
    tracing::info!(
        "📥 [ModelStore] Requesting pull of {} on node {}",
        payload.tag,
        payload.node_id
    );

    // 1. Locate the node and drop the DashMap read guard prior to network .await
    let (node_name, node_address) = {
        let node = state.registry.nodes.get(&payload.node_id).ok_or_else(|| {
            AppError::NotFound(format!("Node '{}' not found in swarm", payload.node_id))
        })?;
        (node.name.clone(), node.address.clone())
    };

    // 2. Prepare and validate the peer pull URL before attaching any credentials
    let trimmed_addr = node_address.trim().trim_end_matches('/');
    let ollama_url = if trimmed_addr.starts_with("http://") || trimmed_addr.starts_with("https://") {
        format!("{}/v1/api/pull", trimmed_addr)
    } else {
        format!("http://{}/v1/api/pull", trimmed_addr)
    };

    let trusted_peers = std::env::var("SWARM_TRUSTED_PEERS").ok();
    if !is_trusted_pull_peer_url(&ollama_url, trusted_peers.as_deref()) {
        return Err(AppError::Forbidden(format!(
            "Refusing to forward Admin pull credentials to untrusted peer node '{}' ({}). Only loopback peers or hosts explicitly allowlisted in SWARM_TRUSTED_PEERS are permitted.",
            node_name, node_address
        )));
    }

    let ollama_payload = serde_json::json!({
        "name": payload.tag,
        "stream": false
    });

    // 3. Dispatch using Admin-capable token (matches POST /v1/api/pull Admin scope under tiered RBAC)
    let peer_token = state
        .security
        .admin_token
        .as_deref()
        .unwrap_or(&state.security.deploy_token);

    let resp = state
        .resources
        .http_client
        .post(&ollama_url)
        .header("Authorization", format!("Bearer {}", peer_token))
        .json(&ollama_payload)
        .send()
        .await
        .map_err(|e| {
            AppError::InternalServerError(format!("Failed to reach Bunker node: {}", e))
        })?;

    if resp.status().is_success() {
        tracing::info!(
            "✅ [ModelStore] Successfully initiated pull on {}",
            node_name
        );
        Ok((
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "success",
                "message": format!("Model {} is being pulled to {}", payload.tag, node_name)
            })),
        ))
    } else {
        let status = resp.status();
        let err = resp
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        tracing::error!(
            "❌ [ModelStore] Pull failed on {} ({}): {}",
            node_name,
            status,
            err
        );
        Err(AppError::InternalServerError(format!(
            "Bunker node '{}' reported error ({}): {}",
            node_name, status, err
        )))
    }
}

/// A lightweight proxy handler that forwards local model pull requests
/// from the Engine port (8000) to the local Ollama instance (11434).
/// This ensures a unified "Swarm Gateway" for all node operations.
pub async fn ollama_proxy_pull(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, AppError> {
    let ollama_host =
        std::env::var("OLLAMA_HOST").unwrap_or_else(|_| "http://127.0.0.1:11434".to_string());
    let ollama_url = format!("{}/api/pull", ollama_host);

    tracing::info!(
        "🔄 [ModelStore] Proxying pull request to local Ollama: {}",
        ollama_url
    );

    let resp = state
        .resources
        .http_client
        .post(&ollama_url)
        .json(&payload)
        .send()
        .await
        .map_err(|e| {
            AppError::InternalServerError(format!("Failed to reach local Ollama: {}", e))
        })?;

    let status = resp.status();
    let body = resp.text().await.unwrap_or_else(|_| "{}".to_string());

    if status.is_success() {
        Ok((status, body))
    } else {
        tracing::error!(
            "❌ [ModelStore] Local Ollama pull failed ({}): {}",
            status,
            body
        );
        Err(AppError::InternalServerError(format!(
            "Ollama reported error: {}",
            body
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_manager_router_and_transport_happy_failure_and_edge() {
        // (a) Happy Path: ProtocolRouter URL construction, discovery parsing, and HTTPS/local transport validation
        let google_router = ProtocolRouter::from_protocol("google");
        assert_eq!(
            google_router.build_models_url("google", "", true),
            "https://generativelanguage.googleapis.com/v1/models"
        );
        let google_body = serde_json::json!({
            "models": [
                {"name": "models/gemini-2.5-pro"},
                {"name": "models/gemini-2.5-flash"}
            ]
        });
        assert_eq!(
            google_router.parse_discovery_response(
                &google_body,
                "https://generativelanguage.googleapis.com/v1/models"
            ),
            vec!["gemini-2.5-pro".to_string(), "gemini-2.5-flash".to_string()]
        );

        let ollama_router = ProtocolRouter::from_protocol("ollama");
        assert_eq!(
            ollama_router.build_models_url("ollama", "http://127.0.0.1:11434", true),
            "http://127.0.0.1:11434/api/tags"
        );
        assert!(validate_provider_transport_url("https://api.openai.com/v1/models", true).is_ok());
        assert!(validate_provider_transport_url("http://127.0.0.1:11434/api/tags", true).is_ok());
        assert!(
            validate_provider_transport_url("http://192.168.1.50:11434/api/tags", true).is_ok()
        );

        // (b) Failure Path (Input Validation & SEC-01): External unencrypted HTTP with API key and malformed URLs
        let insecure_err = validate_provider_transport_url("http://api.openai.com/v1/models", true)
            .expect_err("External HTTP with API key must be blocked by SEC-01");
        assert!(
            matches!(insecure_err, AppError::BadRequest(msg) if msg.contains("Insecure transmission blocked"))
        );

        let malformed_err = validate_provider_transport_url("not a valid url", true)
            .expect_err("Malformed URL must return BadRequest");
        assert!(
            matches!(malformed_err, AppError::BadRequest(msg) if msg.contains("Malformed provider URL"))
        );

        // (c) Edge Case Path: Trailing slashes, cloud vs. local 0.0 pricing normalization, and Anthropic fallback list
        let openai_router = ProtocolRouter::from_protocol("openai");
        assert_eq!(
            openai_router.build_models_url("openai", "https://custom.endpoint.io/v1///", false),
            "https://custom.endpoint.io/v1/models"
        );
        assert_eq!(anthropic_fallback_models().len(), 4);

        let mut cloud_entry = ModelEntry {
            provider_id: "openai".to_string(),
            input_cost_per_m: Some(0.0),
            output_cost_per_m: Some(0.0),
            ..ModelEntry::default()
        };
        enrich_and_normalize_model_entry(&mut cloud_entry, "gpt-4o");
        assert_eq!(cloud_entry.input_cost_per_m, None);
        assert_eq!(cloud_entry.output_cost_per_m, None);

        let mut local_entry = ModelEntry {
            provider_id: "ollama".to_string(),
            input_cost_per_m: Some(0.0),
            output_cost_per_m: Some(0.0),
            ..ModelEntry::default()
        };
        enrich_and_normalize_model_entry(&mut local_entry, "qwen2.5-coder:7b");
        assert_eq!(local_entry.input_cost_per_m, Some(0.0));
        assert_eq!(local_entry.output_cost_per_m, Some(0.0));
    }

    #[tokio::test]
    async fn test_provider_privacy_mode_and_stored_key_exfil_guards() {
        // URL-parsed local host validation rejects prefix/userinfo spoof domains and accepts IPv4/IPv6 loopback
        assert!(is_secure_local("http://127.0.0.1:11434/api/tags"));
        assert!(is_secure_local("http://localhost:11434/api/tags"));
        assert!(is_secure_local("http://[::1]:11434/api/tags"));
        assert!(is_secure_local("https://[::1]:8443/v1/models"));
        assert!(!is_secure_local(
            "https://localhost.attacker.example/v1/models"
        ));
        assert!(!is_secure_local(
            "https://localhost@attacker.example/v1/models"
        ));
        assert!(!is_secure_local(
            "http://127.0.0.1.attacker.example/v1/models"
        ));
        assert!(!is_secure_local("file://localhost/etc/passwd"));

        let state = Arc::new(AppState::new_minimal_mock().await);
        state
            .governance
            .privacy_mode
            .store(true, std::sync::atomic::Ordering::SeqCst);

        // Register a stored custom-endpoint provider with a server-side secret key
        let stored = ProviderConfig {
            id: "custom-openai".to_string(),
            name: "Custom OpenAI".to_string(),
            icon: None,
            base_url: Some("https://internal.corp.example/v1".to_string()),
            protocol: crate::agent::types::ModelProvider::Openai,
            external_id: None,
            custom_headers: None,
            default_config: None,
            supports_steering_vectors: false,
            audio_model: None,
            api_key: Some("sk-stored-secret-do-not-exfil".to_string()),
        };
        state
            .registry
            .providers
            .insert(stored.id.clone(), stored.clone());

        // (1) Privacy Mode MUST block spoofed https://localhost.attacker.example prefix bypass
        let spoof_cfg = ProviderConfig {
            id: "spoof".to_string(),
            name: "Spoof".to_string(),
            icon: None,
            base_url: Some("https://localhost.attacker.example/v1".to_string()),
            protocol: crate::agent::types::ModelProvider::Openai,
            external_id: None,
            custom_headers: None,
            default_config: None,
            supports_steering_vectors: false,
            audio_model: None,
            api_key: Some("sk-explicit-key".to_string()),
        };
        let spoof_res = test_provider(
            axum::extract::State(state.clone()),
            axum::extract::Path("spoof".to_string()),
            Json(spoof_cfg),
        )
        .await;
        assert!(
            matches!(spoof_res, Err(AppError::Forbidden(ref msg)) if msg.contains("Privacy Mode")),
            "Spoofed localhost.attacker.example must be blocked by Privacy Mode"
        );

        // (2) Stored-key exfiltration guard: caller-modified base_url MUST be rejected with 403
        let exfil_cfg = ProviderConfig {
            api_key: None,
            base_url: Some("https://attacker.example/v1".to_string()),
            ..stored.clone()
        };
        let exfil_res = test_provider(
            axum::extract::State(state.clone()),
            axum::extract::Path(stored.id.clone()),
            Json(exfil_cfg),
        )
        .await;
        assert!(
            matches!(exfil_res, Err(AppError::Forbidden(ref msg)) if msg.contains("modified base_url")),
            "Modified base_url with stored API key must return Forbidden"
        );

        // (3) Stored-key omitted base_url falls back to stored_provider.base_url (never public default host)
        // Here stored_provider has a non-local HTTPS URL, so Privacy Mode blocks it deterministically.
        let omitted_base_cfg = ProviderConfig {
            api_key: None,
            base_url: None,
            ..stored.clone()
        };
        let omitted_res = test_provider(
            axum::extract::State(state.clone()),
            axum::extract::Path(stored.id.clone()),
            Json(omitted_base_cfg),
        )
        .await;
        assert!(
            matches!(omitted_res, Err(AppError::Forbidden(ref msg)) if msg.contains("Privacy Mode")),
            "Omitted base_url with stored key must fall back to stored base_url and obey Privacy Mode"
        );
    }

    #[tokio::test]
    async fn test_pull_model_peer_trust_and_tiered_admin_auth() {
        use axum::{middleware::from_fn_with_state, routing::post, Router};

        // Pure peer-trust validation across loopback, IPv6, userinfo spoof, unverified LAN, and explicit allowlist
        assert!(is_trusted_pull_peer_url("http://127.0.0.1:8000/v1/api/pull", None));
        assert!(is_trusted_pull_peer_url("http://localhost:8000/v1/api/pull", None));
        assert!(is_trusted_pull_peer_url("http://[::1]:8000/v1/api/pull", None));
        assert!(!is_trusted_pull_peer_url("http://192.168.50.42:8000/v1/api/pull", None));
        assert!(!is_trusted_pull_peer_url("http://attacker.example:8000/v1/api/pull", None));
        assert!(!is_trusted_pull_peer_url("http://127.0.0.1@attacker.example:8000/v1/api/pull", None));
        assert!(is_trusted_pull_peer_url(
            "http://192.168.50.42:8000/v1/api/pull",
            Some("192.168.50.42:8000")
        ));
        assert!(!is_trusted_pull_peer_url(
            "http://192.168.50.99:8000/v1/api/pull",
            Some("192.168.50.42:8000")
        ));
        // External allowlisted peer MUST use HTTPS (never cleartext HTTP)
        assert!(!is_trusted_pull_peer_url(
            "http://bunker.corp.example/v1/api/pull",
            Some("bunker.corp.example")
        ));
        assert!(is_trusted_pull_peer_url(
            "https://bunker.corp.example/v1/api/pull",
            Some("bunker.corp.example")
        ));

        // Build AppState with tiered RBAC: Operator deploy_token != Admin admin_token
        let mut base_state = AppState::new_minimal_mock().await;
        base_state.security = Arc::new(crate::state::hubs::sec::SecurityHub {
            audit_trail: base_state.security.audit_trail.clone(),
            budget_guard: base_state.security.budget_guard.clone(),
            shell_scanner: base_state.security.shell_scanner.clone(),
            secret_redactor: base_state.security.secret_redactor.clone(),
            system_monitor: base_state.security.system_monitor.clone(),
            permission_policy: base_state.security.permission_policy.clone(),
            verification_gate: base_state.security.verification_gate.clone(),
            deploy_token: "operator-only-token-123456".to_string(),
            admin_token: Some("admin-super-token-987654".to_string()),
            readonly_token: None,
            deploy_token_old: None,
            deploy_token_new: None,
            token_rotated_at: None,
            token_grace_secs: None,
            conflict: base_state.security.conflict.clone(),
        });
        let state = Arc::new(base_state);

        // (a) Happy Path: loopback peer enforcing real validate_token middleware on POST /v1/api/pull
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let peer_addr = listener.local_addr().unwrap();
        let peer_app = Router::new()
            .route(
                "/v1/api/pull",
                post(|| async { (StatusCode::OK, Json(serde_json::json!({ "status": "pulling" }))) }),
            )
            .layer(from_fn_with_state(
                state.clone(),
                crate::middleware::auth::validate_token,
            ));
        let server_handle = tokio::spawn(async move {
            let _ = axum::serve(listener, peer_app).await;
        });

        state.registry.nodes.insert(
            "local-loopback".to_string(),
            crate::agent::types::SwarmNode {
                id: "local-loopback".to_string(),
                name: "Local Bunker".to_string(),
                address: peer_addr.to_string(),
                status: "online".to_string(),
                last_seen: chrono::Utc::now(),
                metadata: std::collections::HashMap::new(),
            },
        );
        let happy_res = pull_model(
            axum::extract::State(state.clone()),
            Json(PullModelPayload {
                node_id: "local-loopback".to_string(),
                tag: "qwen2.5-coder:7b".to_string(),
            }),
        )
        .await;
        server_handle.abort();
        assert!(
            happy_res.is_ok(),
            "Loopback peer pull_model must succeed under tiered RBAC using Admin token, got: {:?}",
            happy_res.err()
        );

        // (b) Failure Path: unverified LAN/mDNS node MUST fail closed with 403 Forbidden (zero credentials leaked)
        state.registry.nodes.insert(
            "rogue-mdns".to_string(),
            crate::agent::types::SwarmNode {
                id: "rogue-mdns".to_string(),
                name: "Unverified Edge".to_string(),
                address: "192.168.50.42:8000".to_string(),
                status: "online".to_string(),
                last_seen: chrono::Utc::now(),
                metadata: std::collections::HashMap::new(),
            },
        );
        let rogue_res = pull_model(
            axum::extract::State(state.clone()),
            Json(PullModelPayload {
                node_id: "rogue-mdns".to_string(),
                tag: "qwen2.5-coder:7b".to_string(),
            }),
        )
        .await;
        assert!(
            matches!(rogue_res, Err(AppError::Forbidden(ref msg)) if msg.contains("SWARM_TRUSTED_PEERS")),
            "Unverified LAN peer must be rejected with 403 Forbidden"
        );

        // (c) Edge Case Path: nonexistent node_id returns 404 NotFound cleanly
        let missing_res = pull_model(
            axum::extract::State(state),
            Json(PullModelPayload {
                node_id: "missing-node-xyz".to_string(),
                tag: "qwen2.5-coder:7b".to_string(),
            }),
        )
        .await;
        assert!(matches!(missing_res, Err(AppError::NotFound(_))));
    }
}

// Metadata: [model_manager]

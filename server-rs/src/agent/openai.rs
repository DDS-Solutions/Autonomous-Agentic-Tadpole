//! @docs ARCHITECTURE:Agent
//!
//! ### AI Assist Note
//! **Universal OpenAI Bridge**: Implements the `LlmProvider` trait for
//! OpenAI-compatible endpoints, including **GPT-4o**, **O1**, and **Groq**.
//! Optimized for **Hallucination Recovery**: features a regex-based
//! extraction engine (`FUNCTION_REGEX`) to repair malformed tool calls
//! (common in Llama 3 models). Intercepts 400 errors to perform
//! **In-Flight JSON Correction** before failing.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: 429 Rate Limit on bursts (RPM/TPM), 400 Bad Request
//!   on tool schema mismatch, or regex bypass on multi-line JSON blocks
//!   exceeding the capture window.
//! - **Trace Scope**: `server-rs::agent::openai`

use crate::agent::runner::parser::PolyglotParser;
use crate::agent::types::{ModelConfig, TokenUsage, ToolCall, ToolDefinition};
use crate::error::AppError;
use once_cell::sync::Lazy;
use regex::Regex;
use reqwest::{header, Client};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
struct OpenAIMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
}

#[derive(Debug, Serialize)]
struct OpenAITool {
    #[serde(rename = "type")]
    tool_type: String,
    function: OpenAIFunctionDefinition,
}

#[derive(Debug, Serialize)]
struct OpenAIFunctionDefinition {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct OpenAIRequest {
    model: String,
    messages: Vec<OpenAIMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OpenAITool>>,
    #[serde(flatten)]
    #[serde(skip_serializing_if = "Option::is_none")]
    extra_parameters: Option<std::collections::HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Deserialize)]
struct OpenAIChoice {
    message: OpenAIResponseMessage,
}

#[derive(Debug, Deserialize)]
struct OpenAIResponseMessage {
    content: Option<String>,
    #[serde(rename = "tool_calls")]
    tool_calls: Option<Vec<OpenAIToolCall>>,
}

#[derive(Debug, Deserialize)]
struct OpenAIToolCall {
    function: OpenAIFunctionCall,
}

#[derive(Debug, Deserialize)]
struct OpenAIFunctionCall {
    name: String,
    arguments: String,
}

#[derive(Debug, Deserialize)]
struct OpenAIUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct OpenAIResponse {
    choices: Vec<OpenAIChoice>,
    usage: Option<OpenAIUsage>,
}

pub struct OpenAIProvider {
    client: Client,
    config: ModelConfig,
    api_key: String,
}

/// Regex for extracting tool calls from raw text (Groq/Llama 3 style)
/// Enhanced to handle hallucinated '=' or '(' after the function name, and extra closing tags.
#[allow(dead_code)]
static FUNCTION_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?s)<function=([a-zA-Z0-9_-]+)[^\{]*(\{.*?\})[^>]*>?").expect("Static tool-call parser regex MUST be valid."));

impl OpenAIProvider {
    pub fn new(client: Client, api_key: String, config: ModelConfig) -> Self {
        Self {
            client,
            config,
            api_key,
        }
    }

    pub async fn generate(
        &self,
        system_prompt: &str,
        user_message: &str,
        tools: Option<Vec<ToolDefinition>>,
    ) -> Result<(String, Vec<ToolCall>, Option<TokenUsage>), AppError> {
        self.generate_internal(system_prompt, user_message, tools, None)
            .await
    }

    /// Main generation loop for OpenAI-compatible models.
    ///
    /// ### 🛠️ Resiliency Layers
    /// 1. **Tool Mapping**: Translating Gemini-style tools to OpenAI's tool-call schema.
    /// 2. **Retry Logic**: Injecting error-correction instructions if previous attempts failed.
    /// 3. **Native vs Bridge**: Detecting if the model supports native tools or needs tag parsing.
    async fn generate_internal(
        &self,
        system_prompt: &str,
        user_message: &str,
        tools: Option<Vec<ToolDefinition>>,
        retry_msg: Option<String>,
    ) -> Result<(String, Vec<ToolCall>, Option<TokenUsage>), AppError> {
        self.generate_with_fallback(system_prompt, user_message, tools, retry_msg, false).await
    }

    /// ### 📡 Intelligence Orchestration: OpenAI Native/Bridge (generate_with_fallback)
    /// Orchestrates the primary generative heartbeat for OpenAI-compatible providers.
    /// Handles native tool calls, tag extraction, and recursive failure recovery.
    /// 
    /// ### 🧬 Logic: Resilient Execution (OML-01)
    /// 1. **IPv4 Loopback Enforcement**: Resolves IPv4/IPv6 localhost conflicts 
    ///    by forcing `127.0.0.1` for local sidecars.
    /// 2. **Tool Schema Mapping**: Translates engine-standard tools into 
    ///    OpenAI's `tool_type: function` format.
    /// 3. **Dynamic Quantization Fallback**: If a local provider (Ollama) 
    ///    returns an OOM (Out Of Memory) error, the engine automatically 
    ///    downgrades the model to a `Q4_K_M` quantization to prevent mission failure.
    /// 4. **In-Flight JSON Correction**: Intercepts `400` errors related to 
    ///    malformed tool use and performs a manual regex extraction pass.
    /// 5. **Hallucination Cleanup**: Manually extracts and strips raw function 
    ///    tags leaked into the content by models like Llama-3 or Phi-3.
    async fn generate_with_fallback(
        &self,
        system_prompt: &str,
        user_message: &str,
        tools: Option<Vec<ToolDefinition>>,
        retry_msg: Option<String>,
        is_fallback: bool,
    ) -> Result<(String, Vec<ToolCall>, Option<TokenUsage>), AppError> {
        let mut url = self
            .config
            .base_url
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "https://api.openai.com/v1".to_string())
            .trim()
            .to_string();

        // ### 🛡️ Sector Defense: Localhost Conflict Resolution
        // Resolve IPv4/IPv6 localhost conflict on Windows by forcing 127.0.0.1.
        // This prevents 500 errors when local sidecars (Ollama/Mercury) only 
        // bind to the IPv4 loopback.
        if url.contains("localhost") {
            url = url.replace("localhost", "127.0.0.1");
        }

        // Canonicalize the completions endpoint
        if !url.ends_with("/chat/completions") && !url.contains("/v1/audio") {
            if !url.ends_with('/') {
                url.push('/');
            }
            url.push_str("chat/completions");
        }

        // Map Gemini tools to OpenAI tools
        let openai_tools = tools.as_ref().map(|ts| {
            ts.iter()
                .flat_map(|t| {
                    t.function_declarations.iter().map(|f| OpenAITool {
                        tool_type: "function".to_string(),
                        function: OpenAIFunctionDefinition {
                            name: f.name.clone(),
                            description: f.description.clone(),
                            parameters: f.parameters.clone(),
                        },
                    })
                })
                .collect::<Vec<OpenAITool>>()
        });

        let mut messages = Vec::new();
        
        let has_vectors = self.config.steering_vectors.as_ref().is_some_and(|v| !v.is_empty());
        
        if !has_vectors {
            messages.push(OpenAIMessage {
                role: "system".to_string(),
                content: Some(system_prompt.to_string()),
            });
        } else {
            tracing::info!("🧠 [OBLITERATUS] Steering vectors active. Stripping bulky system prompt.");
        }

        messages.push(OpenAIMessage {
            role: "user".to_string(),
            content: Some(user_message.to_string()),
        });

        // If this is a retry, append the failed generation and correction instruction
        if let Some(ref r) = retry_msg {
            messages.push(OpenAIMessage {
                role: "assistant".to_string(),
                content: Some(r.clone()),
            });
            messages.push(OpenAIMessage {
                role: "user".to_string(),
                content: Some("CRITICAL ERROR: Your previous tool call was malformed. Please fix the JSON syntax and try again. Ensure all arguments are inside the brackets and there are no stray characters.".to_string()),
            });
        }

        let request_body = OpenAIRequest {
            model: self.config.model_id.to_lowercase(),
            messages,
            temperature: self.config.temperature,
            user: self.config.external_id.clone(),
            tools: if openai_tools.as_ref().is_none_or(|t| t.is_empty()) {
                None
            } else if !self.config.supports_native_tools() {
                tracing::warn!(
                    "🛡️ [Provider] Suppressing tools for '{}' (Incompatible with bridge)",
                    self.config.model_id
                );
                None
            } else {
                openai_tools
            },
            extra_parameters: {
                let mut p = self.config.extra_parameters.clone().unwrap_or_default();
                if let Some(ref vectors) = self.config.steering_vectors {
                    if !vectors.is_empty() {
                        p.insert("steering_vectors".to_string(), serde_json::json!(vectors));
                    }
                }
                if p.is_empty() { None } else { Some(p) }
            },
        };

        let mut request = self
            .client
            .post(&url)
            .header(header::AUTHORIZATION, format!("Bearer {}", self.api_key.trim()))
            .json(&request_body);

        // ### 🛡️ Sector Defense: Extended Timeout for Local Models
        // Local models (Ollama/Mercury) can be slow on consumer hardware during heavy inference. 
        // We extend the timeout to 300s to prevent premature mission failure.
        // If memory pressure is high (>92%), we extend further to 600s to allow for swapping.
        if url.contains("127.0.0.1") || url.contains("localhost") {
            let timeout_secs = if let Some(stats) = self.config.extra_parameters.as_ref().and_then(|p| p.get("memory_pressure")).and_then(|v| v.as_f64()) {
                if stats >= 0.92 { 600 } else { 300 }
            } else {
                300
            };
            request = request.timeout(std::time::Duration::from_secs(timeout_secs));
        }

        let res_result = request.send().await;
        if let Err(ref e) = res_result {
            tracing::error!("🚨 [CRITICAL DEBUG] Reqwest send failed! Error: {:?}", e);
            if e.is_builder() {
                tracing::error!("🚨 [CRITICAL DEBUG] It is a BUILDER error! URL: {}, Body: {}", url, serde_json::to_string(&request_body).unwrap_or_else(|_| "Failed to serialize body for debug".to_string()));
            }
        }

        let res = res_result.map_err(|e| {
            if e.is_timeout() {
                tracing::error!("🕒 [OpenAI] Request timed out for URL: {}. Model might be too slow for current hardware.", url);
            } else if e.is_connect() {
                tracing::error!("🔌 [OpenAI] Failed to connect to URL: {}. Is the provider service (Ollama/Mercury) running?", url);
            }
            AppError::from(e)
        })?;

        if !res.status().is_success() {
            let status = res.status();
            let error_text = res.text().await?;
            tracing::error!("📡 [OpenAI] Request failed with status {}: {}", status, error_text);
            
            // ### 🧠 Resilience: Dynamic Quantization Fallback (OML-01)
            // Intercept OOM (Out Of Memory) from local providers (Ollama, vLLM, Mercury).
            // Logic: If the primary high-precision model (e.g. Llama-3-70b) fails 
            // due to VRAM exhaustion, we down-rank to a Q4_K_M quantization 
            // to ensure mission continuity without user intervention.
            let is_local = self.config.provider == crate::agent::types::ModelProvider::Ollama 
                || self.config.base_url.as_ref().is_some_and(|u| u.contains("127.0.0.1") || u.contains("localhost"));
            
            if is_local && is_oom_error(&error_text) && !is_fallback {
                let current_model = &self.config.model_id;
                // Avoid infinite fallback loops
                if !current_model.contains(":q") && !current_model.contains("-q") {
                    let fallback_model = format!("{}:q4_K_M", current_model);
                    tracing::warn!("🚨 [OML-01] OOM detected for '{}'. Attempting Dynamic Quantization Fallback to '{}'...", current_model, fallback_model);
                    
                    let mut fallback_config = self.config.clone();
                    fallback_config.model_id = fallback_model;
                    
                    let fallback_provider = OpenAIProvider {
                        client: self.client.clone(),
                        config: fallback_config,
                        api_key: self.api_key.clone(),
                    };

                    return Box::pin(fallback_provider.generate_with_fallback(
                        system_prompt,
                        user_message,
                        tools,
                        retry_msg,
                        true
                    )).await;
                }
            }

            // In-Flight JSON Correction
            if status == 400 && error_text.contains("tool_use_failed") {
                if let Ok(err_json) = serde_json::from_str::<serde_json::Value>(&error_text) {
                    if let Some(failed_gen) = err_json["error"]["failed_generation"].as_str() {
                        tracing::info!(
                            "🛠️ [OpenAI] Native tool failure detected. Generation: {}",
                            failed_gen
                        );
                        // 1. Attempt Polyglot extraction from the failed generation
                        match PolyglotParser::extract(failed_gen) {
                            Ok(recovered_calls) => {
                                tracing::info!("🛠️ [OpenAI] Successfully recovered {} tool calls via PolyglotParser.", recovered_calls.len());
                                let recovered_text = PolyglotParser::scrub_tool_calls(failed_gen);
                                return Ok((
                                    recovered_text,
                                    recovered_calls,
                                    None,
                                ));
                            }
                            Err(crate::agent::runner::parser::ParserError::NoCallsFound) => {
                                tracing::warn!("🛠️ [OpenAI] Recovery failed. No calls found in failed generation.");
                            }
                            Err(e) => {
                                tracing::warn!("🛠️ [OpenAI] Extraction error during recovery: {}. Requiring annealing...", e);
                                return Err(AppError::AnnealingRequired(e.to_string()));
                            }
                        }
                    }
                }
            }

            // Map status codes to AppError
            match status.as_u16() {
                429 => return Err(AppError::RateLimit(error_text)),
                400 => return Err(AppError::BadRequest(error_text)),
                401 | 403 => return Err(AppError::Unauthorized(error_text)),
                _ => return Err(AppError::InfrastructureError {
                    provider_id: "openai".to_string(),
                    detail: error_text,
                    help_link: None,
                }),
            }
        }

        let parsed: OpenAIResponse = res.json().await.map_err(|e| AppError::InternalServerError(e.to_string()))?;

        let choice = parsed
            .choices
            .first()
            .ok_or_else(|| AppError::InternalServerError("No completion return from OpenAI".to_string()))?;

        let mut function_calls = Vec::new();
        if let Some(tool_calls) = &choice.message.tool_calls {
            for tc in tool_calls {
                match serde_json::from_str::<serde_json::Value>(&tc.function.arguments) {
                    Ok(args) => {
                        function_calls.push(ToolCall {
                            name: tc.function.name.clone(),
                            args,
                        });
                    }
                    Err(e) => {
                        // Attempt heuristic repair first
                        let repaired = PolyglotParser::repair_json(&tc.function.arguments);
                        match serde_json::from_str::<serde_json::Value>(&repaired) {
                            Ok(args) => {
                                tracing::info!("🛠️ [OpenAI] Repaired native tool call JSON for '{}'.", tc.function.name);
                                function_calls.push(ToolCall {
                                    name: tc.function.name.clone(),
                                    args,
                                });
                            }
                            Err(_) => {
                                tracing::warn!("🛠️ [OpenAI] Native tool arguments JSON parse failed: {}. Requiring annealing...", e);
                                return Err(AppError::AnnealingRequired(format!("Tool '{}' arguments JSON parse failed: {}", tc.function.name, e)));
                            }
                        }
                    }
                }
            }
        }

        let mut output_text = choice.message.content.clone().unwrap_or_default();
        if output_text.is_empty() && function_calls.is_empty() {
            tracing::warn!("⚠️ [OpenAI] Provider returned 200 OK but EMPTY content and NO tool calls for model '{}'.", self.config.model_id);
        }

        // ### 🛠️ Recovery & Cleanup: Polyglot Tool Extraction
        match PolyglotParser::extract(&output_text) {
            Ok(poly_calls) => {
                tracing::info!("🛠️ [Polyglot] Extracted {} additional tool calls.", poly_calls.len());
                function_calls.extend(poly_calls);
                output_text = PolyglotParser::scrub_tool_calls(&output_text);
            }
            Err(crate::agent::runner::parser::ParserError::NoCallsFound) => {} // Ignore narrative-only
            Err(e) => {
                tracing::warn!("🛠️ [Polyglot] Extraction failed: {}. Requiring annealing...", e);
                return Err(AppError::AnnealingRequired(e.to_string()));
            }
        }

        let token_usage = parsed.usage.map(|u| TokenUsage {
            input_tokens: u.prompt_tokens,
            output_tokens: u.completion_tokens,
            total_tokens: u.total_tokens,
        });

        Ok((output_text, function_calls, token_usage))
    }
}

// ─────────────────────────────────────────────────────────
//  LlmProvider trait implementation
// ─────────────────────────────────────────────────────────

#[async_trait::async_trait]
impl crate::agent::provider_trait::LlmProvider for OpenAIProvider {
    async fn generate(
        &self,
        system_prompt: &str,
        user_message: &str,
        tools: Option<Vec<ToolDefinition>>,
    ) -> Result<(
        String,
        Vec<ToolCall>,
        Option<TokenUsage>,
    ), AppError> {
        OpenAIProvider::generate(self, system_prompt, user_message, tools).await
    }

    async fn embed(&self, text: &str) -> Result<Vec<f32>, AppError> {
        let mut url = self
            .config
            .base_url
            .clone()
            .unwrap_or_else(|| "https://api.openai.com/v1".to_string())
            .trim()
            .to_string();
        if !url.ends_with("/embeddings") {
            if !url.ends_with('/') {
                url.push('/');
            }
            url.push_str("embeddings");
        }

        #[derive(Debug, Serialize)]
        struct EmbedRequest {
            model: String,
            input: String,
        }

        let request_body = EmbedRequest {
            model: self.config.model_id.clone(),
            input: text.to_string(),
        };

        let res = self
            .client
            .post(&url)
            .header(header::AUTHORIZATION, format!("Bearer {}", self.api_key.trim()))
            .json(&request_body)
            .send()
            .await?;

        if !res.status().is_success() {
            let error_text = res.text().await?;
            return Err(AppError::InfrastructureError {
                provider_id: "openai_embedding".to_string(),
                detail: error_text,
                help_link: None,
            });
        }

        #[derive(Debug, Deserialize)]
        struct EmbedData {
            embedding: Vec<f32>,
        }
        #[derive(Debug, Deserialize)]
        struct EmbedResponse {
            data: Vec<EmbedData>,
        }

        let parsed: EmbedResponse = res.json().await.map_err(|e| AppError::InternalServerError(e.to_string()))?;
        Ok(parsed
            .data
            .first()
            .map(|d| d.embedding.clone())
            .unwrap_or_default())
    }
}

/// Detects if an error message indicates an Out-Of-Memory condition.
fn is_oom_error(text: &str) -> bool {
    let oom_keywords = ["out of memory", "insufficient vram", "not enough space", "oom"];
    let lower = text.to_lowercase();
    oom_keywords.iter().any(|k| lower.contains(k))
}

// Metadata: [openai]

// Metadata: [openai]

//! @docs ARCHITECTURE:Runner
//!
//! ### AI Assist Note
//! **Provider Abstraction**: Decouples the engine from specific LLM vendors
//! (Gemini, Groq, OpenAI). Uses a concrete `ProviderVariant` enum to avoid
//! async trait object overhead. Implements **Privacy Guard** (SEC-04) to block
//! external traffic when local-only mode is active.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Missing API keys, unknown provider protocol, or rate limit
//!   (RPM/TPM) breach. Falling back to `NullProvider` ensures missions degrade
//!   gracefully instead of crashing.
//! - **Trace Scope**: `server-rs::agent::runner::provider`

use super::{AgentRunner, RunContext};
use crate::agent::null_provider::{NullProvider, NullReason};
use crate::agent::provider_trait::LlmProvider;
use crate::agent::types::{ModelProvider, TokenUsage, ToolCall, ToolDefinition};
use crate::error::AppError;
use std::sync::Arc;

// ─────────────────────────────────────────────────────────
//  PROVIDER VARIANT ENUM
// ─────────────────────────────────────────────────────────

/// Concrete enum representing all supported LLM provider backends.
pub(crate) enum ProviderVariant {
    /// Google Gemini API (Native tool-calling).
    Gemini(crate::agent::gemini::GeminiProvider),
    /// Groq high-speed inference (Llama/Whisper).
    Groq(crate::agent::groq::GroqProvider),
    /// OpenAI and compatible proxies (Ollama, Inception).
    OpenAI(crate::agent::openai::OpenAIProvider),
    /// Anthropic Claude 3.5 Sonnet (Native Messages API).
    Anthropic(crate::agent::anthropic::AnthropicProvider),
    /// Fallback "Degraded" provider for missing keys or unknown protocols.
    Null(NullProvider),
}

impl ProviderVariant {
    pub(crate) async fn generate(
        &self,
        system_prompt: &str,
        user_message: &str,
        tools: Option<Vec<ToolDefinition>>,
    ) -> Result<(String, Vec<ToolCall>, Option<TokenUsage>), AppError> {
        match self {
            ProviderVariant::Gemini(p) => {
                p.generate(system_prompt, user_message, tools).await
            }
            ProviderVariant::Groq(p) => p.generate(system_prompt, user_message, tools).await,
            ProviderVariant::OpenAI(p) => p.generate(system_prompt, user_message, tools).await,
            ProviderVariant::Anthropic(p) => p.generate(system_prompt, user_message, tools).await,
            ProviderVariant::Null(p) => {
                p.generate(system_prompt, user_message, tools).await
            }
        }
    }

    #[allow(dead_code)]
    pub(crate) async fn embed(&self, text: &str) -> Result<Vec<f32>, AppError> {
        match self {
            ProviderVariant::Gemini(p) => p.embed(text).await,
            ProviderVariant::Groq(p) => p.embed(text).await,
            ProviderVariant::OpenAI(p) => p.embed(text).await,
            ProviderVariant::Anthropic(p) => p.embed(text).await,
            ProviderVariant::Null(p) => p.embed(text).await,
        }
    }
}

/// Resolves an API key: prefers the per-agent config override, then falls
/// back to the named environment variable.
fn resolve_api_key(config: &crate::agent::types::ModelConfig, env_var: &str) -> Option<String> {
    config
        .api_key
        .as_ref()
        .filter(|s| !s.trim().is_empty())
        .cloned()
        .or_else(|| std::env::var(env_var).ok())
}

impl AgentRunner {
    // ─────────────────────────────────────────────────────────
    //  PROVIDER DISPATCH
    // ─────────────────────────────────────────────────────────

    /// Accumulates token usage from a tool call into the mission total.
    pub(crate) fn accumulate_usage(
        &self,
        total: &mut Option<TokenUsage>,
        local: Option<TokenUsage>,
    ) {
        if let Some(loc) = local {
            if let Some(tot) = total {
                tot.input_tokens += loc.input_tokens;
                tot.output_tokens += loc.output_tokens;
                tot.total_tokens += loc.total_tokens;
            } else {
                *total = Some(loc);
            }
        }
    }

    /// Routes the generation request to the correct LLM provider.
    pub(crate) async fn call_provider(
        &self,
        ctx: &RunContext,
        system_prompt: &str,
        user_message: &str,
        tools: Option<Vec<ToolDefinition>>,
    ) -> Result<(String, Vec<ToolCall>, Option<TokenUsage>), AppError> {
        self.dispatch_to_provider(ctx, system_prompt, user_message, tools)
            .await
    }

    /// Calls the provider for a synthesis/follow-up step. Supporting tools here
    /// allows specialists to 'Self-Heal' from sub-agent failures.
    pub(crate) async fn call_provider_for_synthesis(
        &self,
        ctx: &RunContext,
        prompt: &str,
        tools: Option<Vec<ToolDefinition>>,
    ) -> Result<(String, Vec<ToolCall>, Option<TokenUsage>), AppError> {
        let synthesis_prompt = format!("{}\n\nCRITICAL INSTRUCTION: You MUST provide a deterministic resolution. If the sub-agent result is unsatisfactory, use your tools to find an alternative or call 'complete_mission' with the findings so far.", prompt);
        self.dispatch_to_provider(ctx, &synthesis_prompt, "", tools)
            .await
    }

    /// Resolves the correct `ProviderVariant` for the given context.
    pub(crate) fn resolve_provider(
        &self,
        ctx: &RunContext,
        client: reqwest::Client,
    ) -> ProviderVariant {
        use crate::agent::types::ModelProvider;

        tracing::info!(
            "🔍 [Provider] Resolving provider '{}' for agent '{}'",
            ctx.provider_name,
            ctx.agent_id
        );

        if std::env::var("TADPOLE_NULL_PROVIDERS").as_deref() == Ok("true") {
            return ProviderVariant::Null(NullProvider::new(&ctx.agent_id, NullReason::TestMode));
        }

        // SEC-04: Privacy Mode Enforcement
        if self
            .state
            .governance
            .privacy_mode
            .load(std::sync::atomic::Ordering::Relaxed)
            && ctx.model_config.provider != ModelProvider::Ollama
        {
            return ProviderVariant::Null(NullProvider::new(
                &ctx.agent_id,
                NullReason::PrivacyModeEnforced,
            ));
        }

        match ctx.model_config.provider {
            ModelProvider::Google | ModelProvider::Gemini => {
                match resolve_api_key(&ctx.model_config, "GOOGLE_API_KEY") {
                    Some(key) => {
                        ProviderVariant::Gemini(crate::agent::gemini::GeminiProvider::new(
                            client,
                            key,
                            ctx.model_config.clone(),
                        ))
                    }
                    None => ProviderVariant::Null(NullProvider::new(
                        &ctx.agent_id,
                        NullReason::MissingApiKey {
                            env_var: "GOOGLE_API_KEY",
                        },
                    )),
                }
            }
            ModelProvider::Groq => match resolve_api_key(&ctx.model_config, "GROQ_API_KEY") {
                Some(key) => ProviderVariant::Groq(crate::agent::groq::GroqProvider::new(
                    client,
                    key,
                    ctx.model_config.clone(),
                )),
                None => ProviderVariant::Null(NullProvider::new(
                    &ctx.agent_id,
                    NullReason::MissingApiKey {
                        env_var: "GROQ_API_KEY",
                    },
                )),
            },
            ModelProvider::Openai | ModelProvider::Xai | ModelProvider::Openrouter | 
            ModelProvider::Mistral | ModelProvider::Perplexity | ModelProvider::Fireworks | 
            ModelProvider::Together | ModelProvider::Cerebras | ModelProvider::Sambanova |
            ModelProvider::Meta | ModelProvider::Alibaba => {
                let (env_var, default_url) = match ctx.model_config.provider {
                    ModelProvider::Xai => ("XAI_API_KEY", "https://api.x.ai/v1"),
                    ModelProvider::Openrouter => ("OPENROUTER_API_KEY", "https://openrouter.ai/api/v1"),
                    ModelProvider::Mistral => ("MISTRAL_API_KEY", "https://api.mistral.ai/v1"),
                    ModelProvider::Perplexity => ("PERPLEXITY_API_KEY", "https://api.perplexity.ai"),
                    ModelProvider::Fireworks => ("FIREWORKS_API_KEY", "https://api.fireworks.ai/inference/v1"),
                    ModelProvider::Together => ("TOGETHER_API_KEY", "https://api.together.xyz/v1"),
                    ModelProvider::Cerebras => ("CEREBRAS_API_KEY", "https://api.cerebras.ai/v1"),
                    ModelProvider::Sambanova => ("SAMBANOVA_API_KEY", "https://api.sambanova.ai/v1"),
                    ModelProvider::Meta => ("META_API_KEY", "https://api.meta.com/v1"),
                    ModelProvider::Alibaba => ("ALIBABA_API_KEY", "https://api.alibaba.com/v1"),
                    _ => ("OPENAI_API_KEY", "https://api.openai.com/v1"),
                };

                match resolve_api_key(&ctx.model_config, env_var) {
                    Some(key) => {
                        let mut config = ctx.model_config.clone();
                        if config.base_url.is_none() {
                            config.base_url = Some(default_url.to_string());
                        }
                        ProviderVariant::OpenAI(crate::agent::openai::OpenAIProvider::new(
                            client,
                            key,
                            config,
                        ))
                    }
                    None => ProviderVariant::Null(NullProvider::new(
                        &ctx.agent_id,
                        NullReason::MissingApiKey { env_var },
                    )),
                }
            }
            ModelProvider::Inception => {
                match resolve_api_key(&ctx.model_config, "INCEPTION_API_KEY") {
                    Some(key) => {
                        ProviderVariant::OpenAI(crate::agent::openai::OpenAIProvider::new(
                            client,
                            key,
                            ctx.model_config.clone(),
                        ))
                    }
                    None => ProviderVariant::Null(NullProvider::new(
                        &ctx.agent_id,
                        NullReason::MissingApiKey {
                            env_var: "INCEPTION_API_KEY",
                        },
                    )),
                }
            }
            ModelProvider::Deepseek => {
                let api_key = resolve_api_key(&ctx.model_config, "DEEPSEEK_API_KEY")
                    .or_else(|| std::env::var("OPENAI_API_KEY").ok());
                match api_key {
                    Some(key) => {
                        ProviderVariant::OpenAI(crate::agent::openai::OpenAIProvider::new(
                            client,
                            key,
                            ctx.model_config.clone(),
                        ))
                    }
                    None => ProviderVariant::Null(NullProvider::new(
                        &ctx.agent_id,
                        NullReason::MissingApiKey {
                            env_var: "DEEPSEEK_API_KEY",
                        },
                    )),
                }
            }
            ModelProvider::Ollama => {
                let api_key = ctx
                    .model_config
                    .api_key
                    .clone()
                    .unwrap_or_else(|| "ollama".to_string());
                let mut config = ctx.model_config.clone();
                if config.base_url.as_deref().unwrap_or("").trim().is_empty() {
                    let mut host = std::env::var("OLLAMA_HOST")
                        .unwrap_or_else(|_| "http://127.0.0.1:11434".to_string())
                        .trim()
                        .to_string();
                    if !host.ends_with("/v1") && !host.ends_with("/v1/") {
                        if host.ends_with('/') { host.pop(); }
                        host.push_str("/v1");
                    }
                    config.base_url = Some(host);
                }
                ProviderVariant::OpenAI(crate::agent::openai::OpenAIProvider::new(
                    client,
                    api_key,
                    config,
                ))
            }
            ModelProvider::Local => {
                let api_key = ctx
                    .model_config
                    .api_key
                    .clone()
                    .unwrap_or_else(|| "local".to_string());
                let mut config = ctx.model_config.clone();
                if config.base_url.is_none() {
                    config.base_url = Some("http://localhost:8080/v1".to_string());
                }
                ProviderVariant::OpenAI(crate::agent::openai::OpenAIProvider::new(
                    client,
                    api_key,
                    config,
                ))
            }
            ModelProvider::Anthropic => {
                match resolve_api_key(&ctx.model_config, "ANTHROPIC_API_KEY") {
                    Some(key) => {
                        ProviderVariant::Anthropic(crate::agent::anthropic::AnthropicProvider::new(
                            client,
                            key,
                            ctx.model_config.clone(),
                        ))
                    }
                    None => ProviderVariant::Null(NullProvider::new(
                        &ctx.agent_id,
                        NullReason::MissingApiKey {
                            env_var: "ANTHROPIC_API_KEY",
                        },
                    )),
                }
            }
        }
    }

    fn dispatch_to_provider<'a>(
        &'a self,
        ctx: &'a RunContext,
        system_prompt: &'a str,
        user_message: &'a str,
        tools: Option<Vec<ToolDefinition>>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(String, Vec<ToolCall>, Option<TokenUsage>), AppError>> + Send + 'a>> {
        Box::pin(async move {
        use crate::agent::types::ProviderStatus;
        use std::sync::atomic::Ordering;

        let provider_id = ctx.provider_name.clone();
        let health = self.state.registry.provider_health.get(&provider_id)
            .map(|h| *h.value())
            .unwrap_or(ProviderStatus::Green);

        // Pre-emptive Failover for RED status
        if health == ProviderStatus::Red {
            tracing::error!("🚨 [Provider] {} is in RED state. Attempting failover...", provider_id);
        }

        let client = (*self.state.resources.http_client).clone();
        let limiter_key = format!("{}:{}", ctx.provider_name, ctx.model_config.model_id);
        let limiter = self
            .state
            .resources
            .rate_limiters
            .entry(limiter_key.clone())
            .or_insert_with(|| {
                Arc::new(crate::agent::rate_limiter::RateLimiter::new(
                    ctx.model_config.rpm,
                    ctx.model_config.tpm,
                ))
            })
            .value()
            .clone();

        if limiter.is_active() {
            let estimated_tokens = ((system_prompt.len() + user_message.len()) as f64 / 3.5) as u32;
            limiter.acquire(estimated_tokens).await;
        }

        // SSCP: Cloud providers bypass RAM guard intentionally — inference runs remotely.
        // Only local providers (Ollama/127.0.0.1) can cause host OOM.
        let is_local = ctx.model_config.provider == ModelProvider::Ollama 
            || ctx.model_config.base_url.as_ref().is_some_and(|u| u.contains("127.0.0.1") || u.contains("localhost"));
        
        if is_local {
            let arbiter = self.state.resources.continuity_arbiter.clone();
            
            // 1. Update hot registry with current agent's estimated footprint
            let estimated_tokens = ((system_prompt.len() + user_message.len()) as f64 / 3.5) as usize;
            arbiter.update_agent_load(&ctx.agent_id, estimated_tokens);

            // 2. Check for VRAM/RAM pressure
            if arbiter.check_vram_pressure() {
                if let Some(target_id) = arbiter.select_eviction_target() {
                    if target_id != ctx.agent_id {
                        // Retrieve the eviction target's working memory from the registry
                        let working_memory = self.state.registry.agents.get(&target_id)
                            .map(|a| a.state.working_memory.clone());
                        tracing::warn!("❄️ [SSCP] High memory pressure. Evicting agent {} to SSD to prioritize {}", target_id, ctx.agent_id);
                        let _ = arbiter.evict_to_ssd(&target_id, working_memory.as_ref()).await;

                        // ✨ CLEAR FROM REGISTRY TO FREE MEMORY ✨
                        if let Some(mut target_agent) = self.state.registry.agents.get_mut(&target_id) {
                            target_agent.state.working_memory = serde_json::json!({});
                            tracing::info!("♻️ [SSCP] Cleared registry memory for evicted agent {}", target_id);
                        }
                    }
                }
            }

            // 3. Early failure for extreme pressure (SSCP Hard Limit)
            // NOTE: Threshold is 90% (not 98%) — at 98% the OS is in OOM territory
            // and the eviction spawns above may already be failing silently.
            let stats = self.state.security.system_monitor.get_system_defense_stats();
            if stats.memory_pressure >= 0.80 {
                tracing::warn!(
                    "⚠️ [SSCP] High memory pressure ({:.1}%). Degraded inference mode — eviction prioritized.",
                    stats.memory_pressure * 100.0
                );
            }
            if stats.memory_pressure >= 0.90 {
                tracing::error!("🚨 [SSCP] CRITICAL memory pressure ({:.1}%). Swarm execution suspended.", stats.memory_pressure * 100.0);
                return Err(AppError::InfrastructureError { 
                    provider_id: "sscp_guard".to_string(), 
                    detail: "CRITICAL: System memory pressure is too high for safe inference.".to_string(), 
                    help_link: None 
                });
            }
        }

        // Unified provider dispatch (local and cloud share the same failure tracking path)
        let provider = self.resolve_provider(ctx, client);
        let start = std::time::Instant::now();
        let result = provider.generate(system_prompt, user_message, tools.clone()).await;
        let duration = start.elapsed().as_secs_f32() * 1000.0;
        
        // Record latency if successful (we don't record failures to avoid skewing the performance baseline)
        if result.is_ok() {
            self.state.resources.hardware_profiler.record_inference_latency(duration);
        }

        match result {
            Ok((text, tool_calls, usage)) => {
                // Success: Reset failures and restore Green status
                self.state.registry.provider_failures.remove(&provider_id);
                self.state.registry.provider_health.insert(provider_id, ProviderStatus::Green);

                if limiter.is_active() {
                    if let Some(ref u) = usage {
                        limiter.record_usage(u.total_tokens);
                        self.state.governance.tpm_accumulator.fetch_add(
                            u.total_tokens as usize,
                            Ordering::Relaxed,
                        );
                    }
                }
                Ok((text, tool_calls, usage))
            }
            Err(e) => {
                // Failure: Increment count and update status
                let failures = self.state.registry.provider_failures.entry(provider_id.clone())
                    .or_insert_with(|| std::sync::atomic::AtomicU32::new(0));
                let count = failures.fetch_add(1, Ordering::Relaxed) + 1;

                let new_status = if count >= 5 {
                    ProviderStatus::Red
                } else if count >= 3 {
                    ProviderStatus::Amber
                } else {
                    ProviderStatus::Green
                };
                
                self.state.registry.provider_health.insert(provider_id.clone(), new_status);

                tracing::warn!(
                    "⚠️ [Provider] {} failed (count: {}). New status: {:?}",
                    provider_id, count, new_status
                );

                // 🔄 Sovereign Failover: Attempt model-slot failover for connection errors
                let err_str = e.to_string();
                let is_conn_fail = err_str.contains("error sending request")
                    || err_str.contains("Connection refused")
                    || err_str.contains("timed out")
                    || err_str.contains("connection reset")
                    || err_str.contains("status: 429")
                    || err_str.contains("429 Too Many")
                    || err_str.contains("status: 503")
                    || err_str.contains("status: 401")
                    || err_str.contains("Unauthorized");

                if is_conn_fail {
                    if let Some(failover_ctx) = self.try_resolve_failover_context(ctx).await {
                        tracing::warn!(
                            "🔄 [Sovereign Failover] {} unreachable for agent {}. Failing over to {} ({})",
                            provider_id, ctx.agent_id, failover_ctx.provider_name, failover_ctx.model_config.model_id
                        );
                        self.broadcast_sys(
                            &format!(
                                "🔄 Sovereign Failover: {} unreachable. Switching {} to {} ({})...",
                                provider_id.to_uppercase(), ctx.agent_id, 
                                failover_ctx.provider_name.to_uppercase(), failover_ctx.model_config.model_id
                            ),
                            "warning",
                            Some(ctx.mission_id.clone()),
                        );
                        // Recursive dispatch with the failover context (no infinite loop: 
                        // try_resolve_failover_context returns None if no more slots are available)
                        return self.dispatch_to_provider(&failover_ctx, system_prompt, user_message, tools).await;
                    }
                }

                Err(e)
            }
        }
        }) // close Box::pin(async move { ... })
    }

    /// Attempts to resolve a failover RunContext by stepping to the next available model slot.
    /// Returns None if no viable failover slot exists.
    async fn try_resolve_failover_context(&self, ctx: &RunContext) -> Option<RunContext> {
        let entry = self.state.registry.agents.get(&ctx.agent_id)?;
        let a = entry.value();
        
        // Determine current slot and try the next one
        let current_provider_str = ctx.model_config.provider.to_string().to_lowercase();
        let current_model_id = &ctx.model_config.model_id;
        
        // Check if we're on the primary slot (compare against slot 1 config)
        let is_primary = a.models.model.model_id == *current_model_id 
            || a.models.model.provider.to_string().to_lowercase() == current_provider_str;
        
        let is_secondary = a.models.model_config2.as_ref()
            .map(|c| c.model_id == *current_model_id)
            .unwrap_or(false);

        let failover_config = if is_primary {
            // Try Slot 2 first
            a.models.model_config2.as_ref()
                .filter(|c| !c.model_id.is_empty())
                .or_else(|| a.models.model_config3.as_ref().filter(|c| !c.model_id.is_empty()))
        } else if is_secondary {
            // Already failed on Slot 2, try Slot 3
            a.models.model_config3.as_ref().filter(|c| !c.model_id.is_empty())
        } else {
            // Already on Slot 3 or unknown — no more fallbacks
            None
        };

        let failover = failover_config?;

        // Build a new RunContext with the failover model configuration
        let mut new_ctx = ctx.clone();
        new_ctx.model_config = failover.clone();
        new_ctx.provider_name = failover.provider.to_string().to_lowercase();
        
        Some(new_ctx)
    }

    pub(crate) async fn check_budget(
        &self,
        ctx: &RunContext,
        running_cost: &mut f64,
        step_cost: f64,
        output_text: &str,
    ) -> Result<Option<String>, AppError> {
        let budget = ctx.budget_usd;
        *running_cost += step_cost;
        let current_cost = *running_cost;

        if budget > 0.0 && current_cost >= (budget * 1.05) {
            tracing::warn!(
                "⚠️ [Governance] Budget exceeded for mission {}: ${:.4} / ${:.4}",
                ctx.mission_id,
                current_cost,
                budget
            );
            return Ok(Some(format!(
                "(PAUSED: Budget Exceeded ${:.4}/${:.4}) {}",
                current_cost, budget, output_text
            )));
        }

        Ok(None)
    }
}

// Metadata: [provider]

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;

    #[tokio::test]
    async fn test_accumulate_usage() {
        let state = Arc::new(AppState::new_minimal_mock().await);
        let runner = AgentRunner::new(state);
        
        let mut total = Some(TokenUsage { input_tokens: 10, output_tokens: 5, total_tokens: 15 });
        let local = Some(TokenUsage { input_tokens: 20, output_tokens: 10, total_tokens: 30 });
        
        runner.accumulate_usage(&mut total, local);
        
        let tot = total.unwrap();
        assert_eq!(tot.input_tokens, 30);
        assert_eq!(tot.output_tokens, 15);
        assert_eq!(tot.total_tokens, 45);
    }

    static ENV_LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        ENV_LOCK
            .get_or_init(|| std::sync::Mutex::new(()))
            .lock()
            .unwrap()
    }

    #[tokio::test]
    async fn test_check_budget_exceeded() {
        let state = Arc::new(AppState::new_minimal_mock().await);
        let runner = AgentRunner::new(state);
        let mut ctx = RunContext::default();
        ctx.budget_usd = 1.0;
        ctx.current_cost_usd = 1.06; // Over 105%

        let mut cost = ctx.current_cost_usd;
        let res = runner.check_budget(&ctx, &mut cost, 0.0, "Result text").await.unwrap();
        assert!(res.is_some());
        assert!(res.unwrap().contains("Budget Exceeded"));
    }

    #[tokio::test]
    async fn test_check_budget_safe() {
        let state = Arc::new(AppState::new_minimal_mock().await);
        let runner = AgentRunner::new(state);
        let mut ctx = RunContext::default();
        ctx.budget_usd = 1.0;
        ctx.current_cost_usd = 0.5;

        let mut cost = ctx.current_cost_usd;
        let res = runner.check_budget(&ctx, &mut cost, 0.1, "Result text").await.unwrap();
        assert!(res.is_none());
        assert_eq!(cost, 0.6);
    }

    #[tokio::test]
    async fn test_check_budget_accumulates_across_calls() {
        let state = Arc::new(AppState::new_minimal_mock().await);
        let runner = AgentRunner::new(state);
        let mut ctx = RunContext::default();
        ctx.budget_usd = 1.0;
        ctx.current_cost_usd = 0.0;

        let mut running = 0.0f64;
        // 5 turns of $0.22 each = $1.10 > $1.05 threshold (5% buffer)
        for i in 1..=5 {
            let res = runner
                .check_budget(&ctx, &mut running, 0.22, "output")
                .await
                .unwrap();
            if i < 5 {
                assert!(res.is_none(), "Turn {i} should still be under budget");
            } else {
                assert!(res.is_some(), "Turn {i} should trigger budget exceeded");
            }
        }
        assert!((running - 1.10).abs() < 0.001);
    }

    #[test]
    fn test_resolve_api_key() {
        let _guard = env_lock();
        let mut config = crate::agent::types::ModelConfig::default();
        config.api_key = Some("config-key".to_string());

        // Priority should be config
        let key = resolve_api_key(&config, "UNUSED_ENV_VAR");
        assert_eq!(key, Some("config-key".to_string()));

        // Fallback to env
        config.api_key = None;
        std::env::set_var("TEST_PROVIDER_KEY", "env-key");
        let key = resolve_api_key(&config, "TEST_PROVIDER_KEY");
        assert_eq!(key, Some("env-key".to_string()));
        std::env::remove_var("TEST_PROVIDER_KEY");
    }

    #[tokio::test]
    async fn test_sscp_memory_threshold() {
        let mut app_state = AppState::new_minimal_mock().await;
        
        let mock_monitor = crate::security::monitoring::MockSystemMonitor {
            memory_pressure: 0.95, // Above 90% threshold
            cpu_load: 0.5,
        };
        
        let new_security_hub = crate::state::hubs::sec::SecurityHub {
            audit_trail: app_state.security.audit_trail.clone(),
            budget_guard: app_state.security.budget_guard.clone(),
            shell_scanner: app_state.security.shell_scanner.clone(),
            secret_redactor: app_state.security.secret_redactor.clone(),
            system_monitor: Arc::new(mock_monitor),
            permission_policy: app_state.security.permission_policy.clone(),
            deploy_token: app_state.security.deploy_token.clone(),
        };
        app_state.security = Arc::new(new_security_hub);

        let state = Arc::new(app_state);
        let runner = AgentRunner::new(state.clone());

        let mut config = crate::agent::types::ModelConfig::default();
        config.provider = crate::agent::types::ModelProvider::Ollama; // Local provider requires check

        let mut ctx = RunContext::default();
        ctx.agent_id = "test_agent".to_string();
        ctx.mission_id = "test_mission".to_string();
        ctx.model_config = config;

        let result = runner.dispatch_to_provider(&ctx, "System Prompt", "User Msg", None).await;
        
        assert!(result.is_err(), "Expected an error due to high memory pressure");
        if let Err(crate::error::AppError::InfrastructureError { provider_id, detail, .. }) = result {
            assert_eq!(provider_id, "sscp_guard");
            assert!(detail.contains("CRITICAL: System memory pressure is too high"));
        } else {
            panic!("Expected AppError::InfrastructureError for sscp_guard");
        }
    }
}

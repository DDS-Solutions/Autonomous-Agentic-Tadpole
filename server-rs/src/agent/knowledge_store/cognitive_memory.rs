//! @docs ARCHITECTURE:RAGSystems
//! @docs ARCHITECTURE:Retrieval
//!
//! ### AI Assist Note
//! **Cognitive Memory Controller**: Implements a background loop that
//! consolidates short-term episodic memories into long-term semantic knowledge.
//! Summarizes clusters of memories using the default LLM provider when they
//! exceed user-adjustable thresholds.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: database query error, LLM api communication failure, or worker loop panic.
//! - **Telemetry Link**: Search `[cognitive_memory]` in logs.


use crate::state::AppState;
use crate::agent::runner::AgentRunner;
use crate::error::AppError;
use std::sync::Arc;
use std::time::Duration;

pub const MIN_COMPRESSION_INTERVAL_SECS: u64 = 10;
pub const MAX_CONSOLIDATION_MEMORIES: usize = 100;

/// Tiered Memory Controller configuration.
#[derive(Debug, Clone)]
pub struct CognitiveMemoryConfig {
    pub compression_interval_secs: u64,
    pub compression_ceiling: usize,
}

impl Default for CognitiveMemoryConfig {
    fn default() -> Self {
        let compression_interval_secs = std::env::var("MEMORY_COMPRESSION_INTERVAL_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(60);
        let compression_ceiling = std::env::var("MEMORY_COMPRESSION_CEILING")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(5);

        Self {
            compression_interval_secs: compression_interval_secs.max(MIN_COMPRESSION_INTERVAL_SECS),
            compression_ceiling: compression_ceiling.clamp(2, MAX_CONSOLIDATION_MEMORIES),
        }
    }
}

pub async fn start_cognitive_memory_pipeline(app_state: Arc<AppState>) -> tokio::task::JoinHandle<()> {
    let config = CognitiveMemoryConfig::default();
    tracing::info!(
        "🧠 [CognitiveMemory] Tiered Cognitive Memory Pipeline active. Interval: {}s, Ceiling: {} entries.",
        config.compression_interval_secs,
        config.compression_ceiling
    );

    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(config.compression_interval_secs)).await;
            if let Err(e) = run_memory_consolidation(&app_state, &config).await {
                tracing::error!("❌ [CognitiveMemory] Memory consolidation failure: {}", e);
            }
        }
    })
}

async fn run_memory_consolidation(
    state: &Arc<AppState>,
    config: &CognitiveMemoryConfig,
) -> Result<(), AppError> {
    // 1. Fetch active agents
    let agents = sqlx::query_scalar::<_, String>("SELECT id FROM agents ORDER BY id")
        .fetch_all(&state.resources.pool)
        .await
        .map_err(AppError::Sqlx)?;

    let mut first_error = None;
    for agent_id in agents {
        if let Err(error) = consolidate_agent(state, config, &agent_id).await {
            tracing::warn!(%agent_id, %error, "[CognitiveMemory] Agent consolidation failed; continuing sweep");
            if first_error.is_none() { first_error = Some(error); }
        }
    }
    first_error.map_or(Ok(()), Err)
}

async fn consolidate_agent(
    state: &Arc<AppState>, config: &CognitiveMemoryConfig, agent_id: &str,
) -> Result<(), AppError> {
        // Query uncompressed memories for this agent
        let rows = sqlx::query(
            "SELECT id, text FROM fallback_memories WHERE agent_id = ? AND mission_id = 'manual' ORDER BY created_at ASC, id LIMIT ?"
        )
        .bind(&agent_id)
        .bind(MAX_CONSOLIDATION_MEMORIES as i64)
        .fetch_all(&state.resources.pool)
        .await
        .map_err(AppError::Sqlx)?;

        if rows.len() >= config.compression_ceiling {
            tracing::info!(
                "🧠 [CognitiveMemory] Agent {} has {} uncompressed memories. Consolidating...",
                agent_id,
                rows.len()
            );

            // Extract texts
            let mut ids = Vec::new();
            let mut memory_texts = Vec::new();
            for r in rows {
                use sqlx::Row;
                let id: String = r.get("id");
                let text: String = r.get("text");
                ids.push(id);
                memory_texts.push(text);
            }

            // Summarize via default LLM
            let (consolidated_text, usage, cost) = summarize_memories(state, &agent_id, &memory_texts).await?;
            state.security.budget_guard.record_usage(&agent_id, cost).await.map_err(|error| AppError::InternalServerError(error.to_string()))?;
            let memories: Vec<_> = ids.into_iter().zip(memory_texts).collect();
            let new_id = persist_consolidation(state, &agent_id, &memories, &consolidated_text, usage, cost).await?;

            tracing::info!(
                "🧠 [CognitiveMemory] Consolidating memories completed for agent {}. New entry ID: {}",
                agent_id,
                new_id
            );
        }
    Ok(())
}

async fn persist_consolidation(
    state: &Arc<AppState>, agent_id: &str, memories: &[(String, String)],
    consolidated_text: &str, usage: Option<crate::agent::types::TokenUsage>, cost: f64,
) -> Result<String, AppError> {
    if memories.is_empty() || memories.len() > MAX_CONSOLIDATION_MEMORIES || consolidated_text.trim().is_empty() || !cost.is_finite() || cost < 0.0 {
        return Err(AppError::BadRequest("Invalid consolidation batch, summary, or cost".to_string()));
    }
    let _lifecycle = state.registry.lock_agent(agent_id).await;
    let mut agent = state.registry.agents.get(agent_id).ok_or_else(|| AppError::NotFound("Agent was deleted during consolidation".to_string()))?.clone();
    let mut tx = state.resources.pool.begin().await?;

    // Save consolidated memory
    let new_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO fallback_memories (id, agent_id, text, mission_id) VALUES (?, ?, ?, 'consolidated')"
    )
    .bind(&new_id)
    .bind(&agent_id)
    .bind(&consolidated_text)
    .execute(&mut *tx)
    .await
    .map_err(AppError::Sqlx)?;

    // Source identity and text are compared in the same transaction as the summary.
    // A changed/deleted/already-compressed source rolls back the whole replacement.
    for (id, text) in memories {
        let result = sqlx::query("UPDATE fallback_memories SET mission_id = 'compressed' WHERE id = ? AND agent_id = ? AND mission_id = 'manual' AND text = ?")
            .bind(id).bind(&agent_id).bind(text).execute(&mut *tx).await?;
        if result.rows_affected() != 1 { return Err(AppError::Conflict("Memory sources changed during consolidation".to_string())); }
    }
    agent.economics.cost_usd += cost;
    if let Some(usage) = usage {
        agent.economics.tokens_used = agent.economics.tokens_used.saturating_add(usage.total_tokens);
        agent.economics.token_usage = usage;
    }
    agent.version = crate::agent::persistence::save_agent_db_in_tx(&mut tx, &agent).await?;
    tx.commit().await?;
    state.registry.agents.insert(agent_id.to_string(), agent);

    Ok(new_id)
}

async fn summarize_memories(
    state: &Arc<AppState>,
    agent_id: &str,
    texts: &[String],
) -> Result<(String, Option<crate::agent::types::TokenUsage>, f64), AppError> {
    let agent_ctx = state
        .registry
        .agents
        .get(agent_id)
        .ok_or_else(|| AppError::NotFound(format!("Agent {} not found", agent_id)))?.clone();

    if agent_ctx.health.status == "suspended" || !state.security.budget_guard.check_budget(agent_id, 0.0).await.map_err(|error| AppError::InternalServerError(error.to_string()))? {
        return Err(AppError::Forbidden("Memory consolidation is suspended or over quota".to_string()));
    }
    let runner = AgentRunner::new(state.clone());
    let provider = runner.resolve_provider(
        &agent_ctx.resolve_provider_context(state.base_dir.clone()),
        (*state.resources.http_client).clone(),
    );

    let system_prompt = "You are the Tiered Memory Controller of Tadpole OS. Summarize the following episodic short-term memories of the agent into a single, cohesive long-term semantic knowledge entry.";
    let user_message = texts.iter().map(|text| text.chars().take(1000).collect::<String>()).collect::<Vec<_>>().join("\n- ");

    let (response_text, _, usage) = provider
        .generate(system_prompt, &format!("Memories:\n- {}", user_message), None)
        .await?;

    if response_text.trim().is_empty() { return Err(AppError::InternalServerError("Consolidation returned empty text".to_string())); }
    let config = &agent_ctx.models.model;
    let cost = usage.as_ref().map(|usage| crate::agent::rates::calculate_cost_with_custom_rates(
        config.input_cost_per_m, config.output_cost_per_m, &config.model_id, usage.input_tokens, usage.output_tokens,
    )).unwrap_or(0.0);
    Ok((response_text, usage, cost))
}



#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::types::EngineAgent;
    use crate::test_support::{state_with_agent, ProviderStub};
    use axum::http::StatusCode;
    use std::sync::atomic::Ordering;

    const MEMORY_AGENT: &str = "memory-worker-agent";

    async fn worker_memories(state: &AppState) {
        for (id, text) in [("first", "First original memory"), ("second", "Second original memory")] {
            sqlx::query("INSERT INTO fallback_memories (id, agent_id, text, mission_id) VALUES (?, ?, ?, 'manual')")
                .bind(id).bind(MEMORY_AGENT).bind(text).execute(&state.resources.pool).await.unwrap();
        }
    }

    async fn consolidate_worker(state: &Arc<AppState>) -> Result<(), AppError> {
        tokio::time::timeout(Duration::from_secs(10), run_memory_consolidation(state, &CognitiveMemoryConfig {
            compression_interval_secs: MIN_COMPRESSION_INTERVAL_SECS, compression_ceiling: 2,
        })).await.expect("consolidation worker did not finish")
    }

    async fn assert_worker_memories_unchanged(state: &AppState) {
        let memories: Vec<(String, String, String)> = sqlx::query_as(
            "SELECT id, text, mission_id FROM fallback_memories WHERE agent_id = ? ORDER BY id"
        ).bind(MEMORY_AGENT).fetch_all(&state.resources.pool).await.unwrap();
        assert_eq!(memories, vec![
            ("first".to_string(), "First original memory".to_string(), "manual".to_string()),
            ("second".to_string(), "Second original memory".to_string(), "manual".to_string()),
        ]);
        let agent = state.registry.agents.get(MEMORY_AGENT).unwrap().clone();
        assert_eq!(agent.version, 1);
        assert_eq!(agent.economics.cost_usd, 0.0);
        assert_eq!(agent.economics.tokens_used, 0);
        assert_eq!(sqlx::query_scalar::<_, i64>("SELECT version FROM agents WHERE id = ?")
            .bind(MEMORY_AGENT).fetch_one(&state.resources.pool).await.unwrap(), 1);
    }

    #[tokio::test]
    async fn worker_regression_provider_failure_preserves_original_memories() {
        let provider = ProviderStub::start(StatusCode::SERVICE_UNAVAILABLE, "provider unavailable", false).await;
        let state = state_with_agent(MEMORY_AGENT, &provider.base_url).await;
        worker_memories(&state).await;
        assert!(consolidate_worker(&state).await.is_err());
        assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
        assert_worker_memories_unchanged(&state).await;
        assert_eq!(state.security.budget_guard.get_global_stats().1, 0.0);
    }

    #[tokio::test]
    async fn worker_regression_empty_provider_output_preserves_original_memories() {
        let provider = ProviderStub::start(StatusCode::OK, " \n\t ", false).await;
        let state = state_with_agent(MEMORY_AGENT, &provider.base_url).await;
        worker_memories(&state).await;
        assert!(matches!(consolidate_worker(&state).await, Err(AppError::InternalServerError(message))
            if message.contains("empty text")));
        assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
        assert_worker_memories_unchanged(&state).await;
        assert_eq!(state.security.budget_guard.get_global_stats().1, 0.0);
    }

    #[tokio::test]
    async fn worker_regression_suspension_prevents_provider_calls() {
        let provider = ProviderStub::start(StatusCode::OK, "summary", false).await;
        let state = state_with_agent(MEMORY_AGENT, &provider.base_url).await;
        worker_memories(&state).await;
        state.registry.agents.get_mut(MEMORY_AGENT).unwrap().health.status = "suspended".to_string();
        assert!(matches!(consolidate_worker(&state).await, Err(AppError::Forbidden(_))));
        assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
        assert_worker_memories_unchanged(&state).await;
        assert_eq!(state.registry.agents.get(MEMORY_AGENT).unwrap().health.status, "suspended");
    }

    #[tokio::test]
    async fn worker_regression_buffered_quota_exhaustion_prevents_provider_calls() {
        let provider = ProviderStub::start(StatusCode::OK, "summary", false).await;
        let state = state_with_agent(MEMORY_AGENT, &provider.base_url).await;
        worker_memories(&state).await;
        assert!(state.security.budget_guard.check_budget(MEMORY_AGENT, 0.0).await.unwrap());
        state.security.budget_guard.update_quota(MEMORY_AGENT, 0.1,
            Some(crate::security::metering::ResetPeriod::Never)).await.unwrap();
        state.security.budget_guard.record_usage(MEMORY_AGENT, 0.2).await.unwrap();
        assert!(matches!(consolidate_worker(&state).await, Err(AppError::Forbidden(_))));
        assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
        assert_worker_memories_unchanged(&state).await;
        assert_eq!(state.security.budget_guard.get_global_stats().1, 0.2);
    }

    #[tokio::test]
    async fn worker_regression_success_consolidates_and_meters_once() {
        let provider = ProviderStub::start(StatusCode::OK, "combined summary", false).await;
        let state = state_with_agent(MEMORY_AGENT, &provider.base_url).await;
        worker_memories(&state).await;
        consolidate_worker(&state).await.unwrap();
        consolidate_worker(&state).await.unwrap();
        assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
        let compressed: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM fallback_memories WHERE mission_id = 'compressed'")
            .fetch_one(&state.resources.pool).await.unwrap();
        assert_eq!(compressed, 2);
        let summaries: Vec<String> = sqlx::query_scalar("SELECT text FROM fallback_memories WHERE mission_id = 'consolidated'")
            .fetch_all(&state.resources.pool).await.unwrap();
        assert_eq!(summaries, vec!["combined summary".to_string()]);
        let agent = state.registry.agents.get(MEMORY_AGENT).unwrap().clone();
        assert_eq!(agent.version, 2);
        assert_eq!(agent.economics.tokens_used, 15);
        assert!((agent.economics.cost_usd - 0.00002).abs() < 1e-12);
        assert!((state.security.budget_guard.get_global_stats().1 - agent.economics.cost_usd).abs() < 1e-12);
        let (version, cost): (i64, f64) = sqlx::query_as("SELECT version, cost_usd FROM agents WHERE id = ?")
            .bind(MEMORY_AGENT).fetch_one(&state.resources.pool).await.unwrap();
        assert_eq!(version, 2);
        assert!((cost - agent.economics.cost_usd).abs() < 1e-12);
    }

    #[tokio::test]
    async fn finding_regression_consolidation_continues_after_denied_or_failing_agent() {
        for suspended in [true, false] {
            let good_provider = ProviderStub::start(StatusCode::OK, "healthy agent summary", false).await;
            let bad_provider = ProviderStub::start(StatusCode::SERVICE_UNAVAILABLE, "failed agent provider", false).await;
            let state = state_with_agent("a-failing-agent", &bad_provider.base_url).await;
            let mut bad = state.registry.agents.get("a-failing-agent").unwrap().clone();
            let mut good = bad.clone();
            good.identity.id = "z-healthy-agent".to_string();
            good.models.model.base_url = Some(good_provider.base_url.clone());
            good.version = crate::agent::persistence::create_agent_db(&state.resources.pool, &good).await.unwrap();
            state.registry.agents.insert(good.identity.id.clone(), good);
            if suspended {
                bad.health.status = "suspended".to_string();
                state.registry.agents.insert(bad.identity.id.clone(), bad);
            }
            for agent in ["a-failing-agent", "z-healthy-agent"] {
                for index in 0..2 {
                    sqlx::query("INSERT INTO fallback_memories (id, agent_id, text, mission_id) VALUES (?, ?, ?, 'manual')")
                        .bind(format!("{agent}-{index}")).bind(agent).bind(format!("memory {index}"))
                        .execute(&state.resources.pool).await.unwrap();
                }
            }
            assert!(run_memory_consolidation(&state, &CognitiveMemoryConfig { compression_interval_secs: 10, compression_ceiling: 2 }).await.is_err());
            assert_eq!(bad_provider.calls.load(Ordering::SeqCst), usize::from(!suspended));
            assert_eq!(good_provider.calls.load(Ordering::SeqCst), 1);
            assert_eq!(sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM fallback_memories WHERE agent_id = 'a-failing-agent' AND mission_id = 'manual'")
                .fetch_one(&state.resources.pool).await.unwrap(), 2);
            assert_eq!(sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM fallback_memories WHERE agent_id = 'z-healthy-agent' AND mission_id = 'consolidated'")
                .fetch_one(&state.resources.pool).await.unwrap(), 1);
        }
    }

    #[tokio::test]
    async fn consolidation_commits_once_and_rolls_back_changed_sources() {
        let state = Arc::new(AppState::new_minimal_mock().await);
        let mut agent = EngineAgent::default();
        agent.identity.id = "consolidation-agent".to_string();
        agent.version = crate::agent::persistence::create_agent_db(&state.resources.pool, &agent).await.unwrap();
        state.registry.agents.insert(agent.identity.id.clone(), agent.clone());
        for id in ["one", "two"] {
            sqlx::query("INSERT INTO fallback_memories (id, agent_id, text, mission_id) VALUES (?, ?, ?, 'manual')")
                .bind(id).bind(&agent.identity.id).bind(id).execute(&state.resources.pool).await.unwrap();
        }
        let changed = vec![("one".to_string(), "one".to_string()), ("two".to_string(), "stale".to_string())];
        assert!(matches!(persist_consolidation(&state, &agent.identity.id, &changed, "summary", None, 0.0).await, Err(AppError::Conflict(_))));
        assert_eq!(state.registry.agents.get(&agent.identity.id).unwrap().version, 1);
        assert_eq!(sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM fallback_memories WHERE mission_id = 'manual'").fetch_one(&state.resources.pool).await.unwrap(), 2);
        let originals = vec![("one".to_string(), "one".to_string()), ("two".to_string(), "two".to_string())];
        persist_consolidation(&state, &agent.identity.id, &originals, "summary", None, 0.0).await.unwrap();
        assert!(matches!(persist_consolidation(&state, &agent.identity.id, &originals, "duplicate", None, 0.0).await, Err(AppError::Conflict(_))));
        assert_eq!(sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM fallback_memories WHERE mission_id = 'consolidated'").fetch_one(&state.resources.pool).await.unwrap(), 1);
        assert!(persist_consolidation(&state, &agent.identity.id, &[], "", None, -1.0).await.is_err());
    }
}

// Metadata: [cognitive_memory]

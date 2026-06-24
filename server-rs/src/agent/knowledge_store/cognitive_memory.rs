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
//! - **Telemetry Link**: Search `[CognitiveMemory]` in logs.

use crate::state::AppState;
use crate::agent::runner::AgentRunner;
use crate::error::AppError;
use std::sync::Arc;
use std::time::Duration;

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
            compression_interval_secs,
            compression_ceiling,
        }
    }
}

pub async fn start_cognitive_memory_pipeline(app_state: Arc<AppState>) {
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
    });
}

async fn run_memory_consolidation(
    state: &Arc<AppState>,
    config: &CognitiveMemoryConfig,
) -> Result<(), AppError> {
    // 1. Fetch active agents
    let agents = sqlx::query_scalar::<_, String>("SELECT id FROM agents")
        .fetch_all(&state.resources.pool)
        .await
        .map_err(AppError::Sqlx)?;

    for agent_id in agents {
        // Query uncompressed memories for this agent
        let rows = sqlx::query(
            "SELECT id, text FROM fallback_memories WHERE agent_id = ? AND mission_id = 'manual' ORDER BY created_at ASC"
        )
        .bind(&agent_id)
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
            let consolidated_text = summarize_memories(state, &agent_id, &memory_texts).await?;

            // Save consolidated memory
            let new_id = uuid::Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT INTO fallback_memories (id, agent_id, text, mission_id) VALUES (?, ?, ?, 'consolidated')"
            )
            .bind(&new_id)
            .bind(&agent_id)
            .bind(&consolidated_text)
            .execute(&state.resources.pool)
            .await
            .map_err(AppError::Sqlx)?;

            // Mark old memories as compressed
            for id in ids {
                sqlx::query("UPDATE fallback_memories SET mission_id = 'compressed' WHERE id = ?")
                    .bind(id)
                    .execute(&state.resources.pool)
                    .await
                    .map_err(AppError::Sqlx)?;
            }

            tracing::info!(
                "🧠 [CognitiveMemory] Consolidating memories completed for agent {}. New entry ID: {}",
                agent_id,
                new_id
            );
        }
    }

    Ok(())
}

async fn summarize_memories(
    state: &Arc<AppState>,
    agent_id: &str,
    texts: &[String],
) -> Result<String, AppError> {
    let agent_ctx = state
        .registry
        .agents
        .get(agent_id)
        .ok_or_else(|| AppError::NotFound(format!("Agent {} not found", agent_id)))?;

    let runner = AgentRunner::new(state.clone());
    let provider = runner.resolve_provider(
        &agent_ctx.resolve_provider_context(state.base_dir.clone()),
        (*state.resources.http_client).clone(),
    );

    let system_prompt = "You are the Tiered Memory Controller of Tadpole OS. Summarize the following episodic short-term memories of the agent into a single, cohesive long-term semantic knowledge entry.";
    let user_message = texts.join("\n- ");

    let (response_text, _, _) = provider
        .generate(system_prompt, &format!("Memories:\n- {}", user_message), None)
        .await?;

    Ok(response_text)
}

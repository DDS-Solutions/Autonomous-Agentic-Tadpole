//! @docs ARCHITECTURE:State
//!
//! ### AI Assist Note
//! **State Persistence Subsystem**: Orchestrates durable synchronization of agent registries,
//! provider configurations, and model definitions to disk and SQLite with OCC versioning.
//!
//! ### 🔍 Debugging & Observability
//! - **Telemetry Link**: Search `[persist]` in tracing logs.
//! - **Trace Scope**: `server-rs::state::persist`

use dashmap::DashMap;
use sqlx::SqlitePool;
use std::path::Path;

use crate::agent::types::{EngineAgent, ModelEntry, ProviderConfig};
use crate::error::AppError;
use crate::state::AppState;

pub(crate) async fn hydrate_registries(
    base_dir: &Path,
    pool: &SqlitePool,
) -> (
    DashMap<String, ProviderConfig>,
    DashMap<String, ModelEntry>,
    DashMap<String, EngineAgent>,
) {
    tracing::info!("📂 [Registries] Loading Providers and Models...");
    let providers_list = crate::agent::persistence::load_providers(base_dir).await;
    let providers = DashMap::new();
    for p in providers_list {
        providers.insert(p.id.clone(), p);
    }

    let models_list = crate::agent::persistence::load_models(base_dir).await;
    let models = DashMap::new();
    for m in models_list {
        models.insert(m.id.clone(), m);
    }

    tracing::info!("📂 [Registries] Loading Agents...");
    let agents_list = match crate::agent::persistence::load_agents_db(pool).await {
        Ok(list) => list,
        Err(e) => {
            tracing::error!("🚨 [Registries] Failed to load agents from database: {:?}", e);
            Vec::new()
        }
    };

    let agents = DashMap::new();
    for a in agents_list {
        agents.insert(a.identity.id.clone(), a);
    }
    tracing::info!("✅ [Registries] Agents loaded (count: {}).", agents.len());

    (providers, models, agents)
}

impl AppState {
    /// Persists all current agent states to the database in a single transaction.
    /// Batched to avoid N individual round-trips (was the #1 shutdown bottleneck).
    pub async fn save_agents(&self) -> Result<(), AppError> {
        // Capture lock-free snapshot from concurrent DashMap.
        // Eliminates global 64-stripe convoy stalling during periodic flushes (CONC-01).
        // OCC verification (version matching in SQLite and per-agent lock below) guarantees safety.
        let agents: Vec<EngineAgent> = self
            .registry
            .agents
            .iter()
            .map(|entry| entry.value().clone())
            .collect();

        let mut tx = self.resources.pool.begin_with("BEGIN IMMEDIATE").await?;
        let mut versions = Vec::with_capacity(agents.len());
        for agent in &agents {
            versions.push(crate::agent::persistence::save_agent_db_in_tx(&mut tx, agent).await?);
        }
        tx.commit().await?;

        // Safely update versions under individual agent lifecycle locks
        for (agent, version) in agents.iter().zip(versions) {
            let _guard = self.registry.lock_agent(&agent.identity.id).await;
            if let Some(mut current) = self.registry.agents.get_mut(&agent.identity.id) {
                if current.version == agent.version {
                    current.version = version;
                }
            }
        }
        Ok(())
    }

    /// Persists all provider configurations to disk.
    pub async fn save_providers(&self) {
        let providers_vec: Vec<ProviderConfig> = self
            .registry
            .providers
            .iter()
            .map(|kv| kv.value().clone())
            .collect();
        if let Err(e) =
            crate::agent::persistence::save_providers(&self.base_dir, providers_vec).await
        {
            tracing::error!("❌ [State] Failed to persist providers to disk: {:?}", e);
        }
    }

    /// Persists all model metadata to disk.
    pub async fn save_models(&self) {
        let models_vec: Vec<ModelEntry> = self
            .registry
            .models
            .iter()
            .map(|kv| kv.value().clone())
            .collect();
        if let Err(e) = crate::agent::persistence::save_models(&self.base_dir, models_vec).await {
            tracing::error!("❌ [State] Failed to persist models to disk: {:?}", e);
        }
    }

    /// Flushes all volatile buffers to persistent storage.
    pub async fn flush_all(&self) {
        tracing::info!(
            "💾 [System] Flushing all volatile buffers and registries to persistence..."
        );

        // 1. Persist Registries
        if let Err(error) = self.save_agents().await {
            tracing::error!(%error, "[State] Agent flush failed; stale versions were not advanced");
        }
        self.save_providers().await;
        self.save_models().await;

        // 2. Flush Telemetry & Metering
        if let Err(e) = self.security.budget_guard.flush_to_db().await {
            tracing::error!("🚨 [System] Failed to flush budget data: {}", e);
        }
    }

    /// ### 🧬 Evolution: Scan Workspace Skills (scan_workspace_skills_sovereign)
    /// Triggers a recursive scan of the workspace for autonomously generated 
    /// Python/JS skills.
    pub async fn scan_workspace_skills_sovereign(&self) -> Result<usize, AppError> {
        self.registry
            .skills
            .reload_all()
            .await
            .map_err(|e| AppError::InternalServerError(format!("Failed to scan skills: {}", e)))?;

        let snapshot = self.registry.skills.snapshot();
        Ok(snapshot.skills.len() + snapshot.workflows.len() + snapshot.hooks.len())
    }
}

// Metadata: [persist]



//! @docs ARCHITECTURE:Persistence
//!
//! ### AI Assist Note
//! **Storage Synchronizer**: Bridges in-memory registries with persistent
//! **SQLite** and **JSON** storage. Orchestrates **Agent Reaping**
//! (`reap_stale_agents`) to ensure the swarm recovers from zombie/crashed
//! runs. Enforces **Credential Polarization** (SEC-02) by prioritizing
//! environment variables over disk-based JSON configs. Features
//! **Incremental Sync Manifests** to track external data ingestion
//! state across engine restarts.
//!
//! ### 🔍 Debugging & Observability
//! - **Telemetry Link**: Search `[persistence]` in tracing logs.
//! - **Failure Path**: SQLite statement-cache staling across migrations
//!   (causing row decoding panics), JSON parsing errors in
//!   `infra_providers.json`, or heartbeat timeout misconfiguration
//!   leading to premature reaping.
//! - **Trace Scope**: `server-rs::agent::persistence`

use crate::agent::types::{
    AgentCapabilities, AgentEconomics, AgentHealth, AgentIdentity, AgentModels, AgentState,
    EngineAgent, RoleBlueprint, TokenUsage,
};
use crate::error::AppError;

use sqlx::{Row, SqlitePool};

mod catalog;
mod recovery;
pub use catalog::{load_models, load_providers, save_models, save_providers};
pub use recovery::{claim_agent, reap_stale_agents, update_agent_heartbeat};

const DEFAULT_PROVIDER: &str = "ollama";
const DEFAULT_CATEGORY: &str = "user";

/// Loads agents from the database.
/// 
/// ### 🔍 Implementation Details: SQLx Reliability (P1)
/// This function uses a manually specified column list instead of `SELECT *`.
/// This is a **Security Pillar** requirement to prevent the `sqlx` row decoder 
/// from panicking if the underlying SQLite schema is migrated (adding/ordering columns) 
/// while the compiled binary's statement cache remains stale (ROUT-02).
/// ### 📡 Retrieval: load_agents_db
/// Reconstitutes the in-memory agent registry from the persistent SQLite store.
/// 
/// ### 🔍 Implementation Details: SQLx Reliability (P1)
/// This function utilizes a manually specified column manifest instead of `SELECT *`.
/// This is a **Security Pillar** requirement (ROUT-02) to prevent row decoding 
/// panics if the underlying schema is migrated (adding or reordering columns) 
/// while the engine binary's cached prepared statements remain stale.
/// 
/// ### 🧬 Logic: Data De-serialization
/// - **Metadata**: De-serialized from a JSONB string into an extensible HashMap.
/// - **Provider Context**: Reconstituted from raw string slugs via `ModelProvider::from_str`.
/// - **Telemetry**: Aggregated from `input_tokens` and `output_tokens` into a unified `TokenUsage` struct.
pub async fn load_agents_db(pool: &SqlitePool) -> Result<Vec<EngineAgent>, AppError> {
    let rows = sqlx::query(
        "SELECT
            id,
            name,
            role,
            department,
            description,
            model_id,
            tokens_used,
            status,
            current_task,
            input_tokens,
            output_tokens,
            theme_color,
            budget_usd,
            cost_usd,
            voice_id,
            metadata,
            skills,
            workflows,
            mcp_tools,
            connector_configs,
            model_2,
            model_3,
            model_config2,
            model_config3,
            active_model_slot,
            voice_engine,
            category,
            provider,
            api_key,
            base_url,
            system_prompt,
            temperature,
            active_mission,
            failure_count,
            last_failure_at,
            created_at,
            heartbeat_at,
            requires_oversight,
            working_memory,
            stt_engine,
            version,
            runner_policy
         FROM agents",
    )
    .fetch_all(pool)
    .await?;
    let mut agents = Vec::new();

    for row in rows {
        let metadata_str = row
            .try_get::<Option<String>, _>("metadata")
            .ok()
            .flatten()
            .unwrap_or_else(|| "{}".to_string());
        let metadata: std::collections::HashMap<String, serde_json::Value> =
            serde_json::from_str(&metadata_str).unwrap_or_default();
        let input_tokens = row.get::<Option<i64>, _>("input_tokens").unwrap_or(0) as u32;
        let output_tokens = row.get::<Option<i64>, _>("output_tokens").unwrap_or(0) as u32;

        let provider_str: String = row
            .try_get("provider")
            .unwrap_or_else(|_| DEFAULT_PROVIDER.to_string());
        let provider = match crate::agent::types::ModelProvider::from_str(&provider_str) {
            Some(p) => p,
            None => {
                let agent_id: String = row.try_get("id").unwrap_or_default();
                tracing::warn!(
                    "⚠️ [Persistence] Unknown model provider '{}' for agent '{}'. Falling back to Ollama.",
                    provider_str,
                    agent_id
                );
                crate::agent::types::ModelProvider::Ollama
            }
        };

        let agent = EngineAgent {
            identity: AgentIdentity {
                id: row.get("id"),
                name: row.get("name"),
                role: row.get("role"),
                department: row.get("department"),
                description: row.get("description"),
                category: row.try_get("category").unwrap_or_else(|_| DEFAULT_CATEGORY.to_string()),
                theme_color: row.get("theme_color"),
            },
            models: AgentModels {
                model_id: row.get("model_id"),
                model: crate::agent::types::ModelConfig {
                    provider,
                    model_id: row
                        .get::<Option<String>, _>("model_id")
                        .filter(|s| !s.trim().is_empty())
                        .unwrap_or_else(|| "".to_string()),
                    api_key: row
                        .try_get::<Option<String>, _>("api_key")
                        .ok()
                        .flatten()
                        .filter(|s| !s.trim().is_empty()),
                    base_url: row
                        .try_get::<Option<String>, _>("base_url")
                        .ok()
                        .flatten(),
                    system_prompt: row
                        .try_get::<Option<String>, _>("system_prompt")
                        .ok()
                        .flatten(),
                    temperature: row.get::<Option<f64>, _>("temperature").map(|f| f as f32),
                    ..Default::default()
                },
                model_2: row.try_get("model_2").ok(),
                model_3: row.try_get("model_3").ok(),
                model_config2: row
                    .get::<Option<String>, _>("model_config2")
                    .and_then(|s| serde_json::from_str(&s).ok()),
                model_config3: row
                    .get::<Option<String>, _>("model_config3")
                    .and_then(|s| serde_json::from_str(&s).ok()),
                active_model_slot: row.get::<Option<i32>, _>("active_model_slot"),
            },
            economics: AgentEconomics {
                budget_usd: row.get::<Option<f64>, _>("budget_usd").unwrap_or(0.0),
                cost_usd: row.get::<Option<f64>, _>("cost_usd").unwrap_or(0.0),
                tokens_used: row.get::<Option<i64>, _>("tokens_used").unwrap_or(0) as u32,
                token_usage: TokenUsage {
                    input_tokens,
                    output_tokens,
                    total_tokens: input_tokens + output_tokens,
                },
            },
            health: AgentHealth {
                status: row.get("status"),
                failure_count: row.get::<Option<i64>, _>("failure_count").unwrap_or(0) as u32,
                last_failure_at: row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("last_failure_at"),
                heartbeat_at: row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("heartbeat_at"),
            },
            capabilities: AgentCapabilities {
                skills: row
                    .try_get::<Option<String>, _>("skills")
                    .ok()
                    .flatten()
                    .as_deref()
                    .and_then(|s| serde_json::from_str(s).ok())
                    .unwrap_or_default(),
                workflows: row
                    .try_get::<Option<String>, _>("workflows")
                    .ok()
                    .flatten()
                    .as_deref()
                    .and_then(|s| serde_json::from_str(s).ok())
                    .unwrap_or_default(),
                mcp_tools: row
                    .try_get::<Option<String>, _>("mcp_tools")
                    .ok()
                    .flatten()
                    .as_deref()
                    .and_then(|s| serde_json::from_str(s).ok())
                    .unwrap_or_default(),
                skill_manifest: None,
            },
            state: AgentState {
                active_mission: row
                    .get::<Option<String>, _>("active_mission")
                    .and_then(|s| serde_json::from_str(&s).ok()),
                current_task: row.get::<Option<String>, _>("current_task"),
                working_memory: row
                    .get::<Option<String>, _>("working_memory")
                    .and_then(|s| serde_json::from_str(&s).ok())
                    .unwrap_or_else(|| serde_json::json!({})),
                current_reasoning_turn: 0,
            },
            metadata,
            voice_id: row.try_get("voice_id").ok(),
            voice_engine: row.try_get("voice_engine").ok(),
            connector_configs: row
                .try_get::<Option<String>, _>("connector_configs")
                .ok()
                .flatten()
                .as_deref()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or_default(),
            created_at: row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("created_at"),
            requires_oversight: row
                .get::<Option<bool>, _>("requires_oversight")
                .unwrap_or(false),
            stt_engine: row.try_get("stt_engine").ok(),
            version: row.get::<i64, _>("version") as u32,
            runner_policy: row
                .try_get::<Option<String>, _>("runner_policy")
                .ok()
                .flatten()
                .as_deref()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or_default(),
        };
        agents.push(agent);
    }
    Ok(agents)
}

async fn execute_save_agent<'c, E>(executor: E, agent: &EngineAgent, create: bool) -> Result<u32, AppError>
where
    E: sqlx::Executor<'c, Database = sqlx::Sqlite>,
{
    let primary_model_id = Some(&agent.models.model.model_id)
        .filter(|id| !id.trim().is_empty())
        .cloned()
        .or_else(|| {
            agent
                .models
                .model_id
                .as_ref()
                .filter(|id| !id.trim().is_empty())
                .cloned()
        });

    let res = sqlx::query("INSERT INTO agents (id, name, role, department, description, model_id, tokens_used, status, current_task, input_tokens, output_tokens, theme_color, budget_usd, cost_usd, metadata, skills, workflows, mcp_tools, connector_configs, model_2, model_3, model_config2, model_config3, active_model_slot, voice_id, voice_engine, failure_count, last_failure_at, created_at, heartbeat_at, active_mission, provider, api_key, base_url, system_prompt, temperature, category, requires_oversight, working_memory, stt_engine, version, runner_policy)
            SELECT ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?
            WHERE ? OR EXISTS (SELECT 1 FROM agents WHERE id = ?)
            ON CONFLICT(id) DO UPDATE SET
            name = excluded.name,
            role = excluded.role,
            department = excluded.department,
            description = excluded.description,
            model_id = excluded.model_id,
            tokens_used = excluded.tokens_used,
            status = excluded.status,
            current_task = excluded.current_task,
            input_tokens = excluded.input_tokens,
            output_tokens = excluded.output_tokens,
            theme_color = excluded.theme_color,
            budget_usd = excluded.budget_usd,
            cost_usd = excluded.cost_usd,
            metadata = excluded.metadata,
            skills = excluded.skills,
            workflows = excluded.workflows,
            mcp_tools = excluded.mcp_tools,
            connector_configs = excluded.connector_configs,
            model_2 = excluded.model_2,
            model_3 = excluded.model_3,
            model_config2 = excluded.model_config2,
            model_config3 = excluded.model_config3,
            active_model_slot = excluded.active_model_slot,
            voice_id = excluded.voice_id,
            voice_engine = excluded.voice_engine,
            failure_count = excluded.failure_count,
            last_failure_at = excluded.last_failure_at,
            created_at = excluded.created_at,
            heartbeat_at = excluded.heartbeat_at,
            active_mission = excluded.active_mission,
            provider = excluded.provider,
            api_key = excluded.api_key,
            base_url = excluded.base_url,
            system_prompt = excluded.system_prompt,
            temperature = excluded.temperature,
            category = excluded.category,
            requires_oversight = excluded.requires_oversight,
            working_memory = excluded.working_memory,
            stt_engine = excluded.stt_engine,
            version = agents.version + 1,
            runner_policy = excluded.runner_policy
            WHERE NOT ? AND agents.id = excluded.id AND agents.version = ?
            RETURNING version"
    )
    .bind(&agent.identity.id)
    .bind(&agent.identity.name)
    .bind(&agent.identity.role)
    .bind(&agent.identity.department)
    .bind(&agent.identity.description)
    .bind(&primary_model_id)
    .bind(agent.economics.tokens_used as i64)
    .bind(&agent.health.status)
    .bind(&agent.state.current_task)
    .bind(agent.economics.token_usage.input_tokens as i64)
    .bind(agent.economics.token_usage.output_tokens as i64)
    .bind(&agent.identity.theme_color)
    .bind(agent.economics.budget_usd)
    .bind(agent.economics.cost_usd)
    .bind(sqlx::types::Json(&agent.metadata))
    .bind(sqlx::types::Json(&agent.capabilities.skills))
    .bind(sqlx::types::Json(&agent.capabilities.workflows))
    .bind(sqlx::types::Json(&agent.capabilities.mcp_tools))
    .bind(sqlx::types::Json(&agent.connector_configs))
    .bind(&agent.models.model_2)
    .bind(&agent.models.model_3)
    .bind(agent.models.model_config2.as_ref().map(sqlx::types::Json))
    .bind(agent.models.model_config3.as_ref().map(sqlx::types::Json))
    .bind(agent.models.active_model_slot)
    .bind(&agent.voice_id)
    .bind(&agent.voice_engine)
    .bind(agent.health.failure_count as i64)
    .bind(agent.health.last_failure_at)
    .bind(agent.created_at)
    .bind(agent.health.heartbeat_at)
    .bind(agent.state.active_mission.as_ref().map(sqlx::types::Json))
    .bind(agent.models.model.provider.to_string())
    .bind(None::<String>)
    .bind(&agent.models.model.base_url)
    .bind(&agent.models.model.system_prompt)
    .bind(agent.models.model.temperature.map(|f| f as f64))
    .bind(&agent.identity.category)
    .bind(agent.requires_oversight)
    .bind(sqlx::types::Json(&agent.state.working_memory))
    .bind(&agent.stt_engine)
    .bind(if create { 1 } else { agent.version as i64 })
    .bind(sqlx::types::Json(&agent.runner_policy))
    .bind(create)
    .bind(&agent.identity.id)
    .bind(create)
    .bind(agent.version as i64)
    .fetch_optional(executor)
    .await?;

    match res {
        Some(row) => {
            let ver: i64 = row.get("version");
            Ok(ver as u32)
        }
        None => Err(AppError::Conflict(format!(
            "Agent '{}' is missing, already exists, or has a conflicting version",
            agent.identity.id
        ))),
    }
}

fn is_safe_builtin_skill(skill_id: &str) -> bool {
    matches!(
        skill_id,
        "read_file"
            | "read_codebase_file"
            | "list_files"
            | "list_dir"
            | "grep_search"
            | "get_current_time"
            | "calculate"
            | "share_finding"
            | "update_working_memory"
            | "get_file_contents"
            | "get_project_status"
            | "get_agent_metrics"
            | "get_current_mission_status"
            | "search_global_vault"
            | "list_skill_metadata"
            | "search_mission_knowledge"
            | "list_file_symbols"
            | "get_symbol_body"
    )
}

async fn auto_subscribe_agent_skills(
    conn: &mut sqlx::SqliteConnection,
    agent: &EngineAgent,
) -> Result<(), AppError> {
    let now = chrono::Utc::now().timestamp();
    for skill_id in &agent.capabilities.skills {
        let sub_id = format!("sub-{}-{}", skill_id, agent.identity.id);
        use sha2::{Digest, Sha256};
        let is_builtin = crate::agent::runner::tools::registry::BUILTIN_TOOLS.contains(&skill_id.as_str());
        let mut hasher = Sha256::new();
        if is_builtin {
            hasher.update(b"builtin:");
            hasher.update(skill_id.as_bytes());
        } else {
            hasher.update(agent.identity.id.as_bytes());
            hasher.update(b":");
            hasher.update(skill_id.as_bytes());
        }
        let scope_hash = hex::encode(hasher.finalize());
        let is_safe = is_safe_builtin_skill(skill_id);
        let status = if is_safe { "approved" } else { "pending" };
        let notes = if is_safe {
            "Baseline safe system capability auto-approved"
        } else if is_builtin {
            "Built-in mutating/external capability awaiting operator approval"
        } else {
            "Awaiting operator approval"
        };

        sqlx::query(
            "INSERT OR IGNORE INTO skill_subscriptions
             (id, agent_id, skill_id, scope_hash, subscription_status, installed_at, last_updated_at, notes)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(&sub_id)
        .bind(&agent.identity.id)
        .bind(skill_id)
        .bind(&scope_hash)
        .bind(status)
        .bind(now)
        .bind(now)
        .bind(notes)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

/// Persists and publishes an agent update while the caller holds its lifecycle lock.
/// Publication follows the strict versioned transaction; conflicts propagate.
pub(crate) async fn update_and_persist_agent<F>(
    state: &std::sync::Arc<crate::state::AppState>, agent_id: &str, update: F,
) -> Result<EngineAgent, AppError>
where F: FnOnce(&mut EngineAgent) {
    let mut updated = state.registry.agents.get(agent_id)
        .ok_or_else(|| AppError::NotFound(format!("Agent '{}' not found", agent_id)))?.clone();
    update(&mut updated);
    updated.version = save_agent_db(&state.resources.pool, &updated).await?;
    state.registry.agents.insert(agent_id.to_string(), updated.clone());
    state.emit_event(serde_json::json!({
        "type": "agent:update", "agent_id": agent_id, "data": updated.clone()
    }));
    Ok(updated)
}

pub async fn save_agent_db(pool: &SqlitePool, agent: &EngineAgent) -> Result<u32, AppError> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let new_ver = save_agent_db_in_tx(&mut tx, agent).await?;
    tx.commit().await?;
    Ok(new_ver)
}

/// Explicit creation never overwrites an existing identity. Ordinary saves cannot resurrect deletions.
pub async fn create_agent_db(pool: &SqlitePool, agent: &EngineAgent) -> Result<u32, AppError> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let version = create_agent_db_in_tx(&mut tx, agent).await?;
    tx.commit().await?;
    Ok(version)
}

pub async fn create_agent_db_in_tx(
    conn: &mut sqlx::SqliteConnection,
    agent: &EngineAgent,
) -> Result<u32, AppError> {
    let version = execute_save_agent(&mut *conn, agent, true).await?;
    sync_manifests_for_agent(&mut *conn, agent).await?;
    auto_subscribe_agent_skills(&mut *conn, agent).await?;
    Ok(version)
}

/// Transaction-compatible variant of `save_agent_db`.
pub async fn save_agent_db_in_tx(
    conn: &mut sqlx::SqliteConnection,
    agent: &EngineAgent,
) -> Result<u32, AppError> {
    let new_ver = execute_save_agent(&mut *conn, agent, false).await?;

    sync_manifests_for_agent(&mut *conn, agent).await?;
    auto_subscribe_agent_skills(&mut *conn, agent).await?;
    Ok(new_ver)
}

/// Synchronizes an agent's connector configurations with the `sync_manifest` table.
/// 
/// This ensures that the background data ingestion workers know which URIs to watch
/// for a specific specialist agent. It handles both "Cleanup" (deleting removed URIs)
/// and "Discovery" (adding new URIs).
async fn sync_manifests_for_agent(
    conn: &mut sqlx::SqliteConnection,
    agent: &EngineAgent,
) -> Result<(), AppError> {

    // 1. Delete manifests that are no longer in the agent config
    let current_uris: Vec<String> = agent
        .connector_configs
        .iter()
        .map(|c| c.uri.clone())
        .collect();

    sqlx::query("DELETE FROM sync_manifest WHERE agent_id = ? AND source_uri NOT IN (SELECT value FROM json_each(?))")
        .bind(&agent.identity.id)
        .bind(serde_json::to_string(&current_uris).map_err(|e| AppError::InternalServerError(e.to_string()))?)
        .execute(&mut *conn)
        .await?;

    // 2. Add new manifests
    for config in &agent.connector_configs {
        sqlx::query("INSERT OR IGNORE INTO sync_manifest (id, agent_id, source_type, source_uri, status) VALUES (?, ?, ?, ?, 'idle')")
            .bind(format!("{}-{}", agent.identity.id, config.uri))
            .bind(&agent.identity.id)
            .bind(&config.r#type)
            .bind(&config.uri)
            .execute(&mut *conn)
            .await?;
    }

    Ok(())
}

/// Loads all sync manifests from the database.
pub async fn load_sync_manifests(pool: &SqlitePool) -> Result<Vec<crate::agent::SyncManifest>, AppError> {
    let rows = sqlx::query_as::<_, crate::agent::SyncManifest>(
        "SELECT id, agent_id, source_type, source_uri, status, last_sync_at FROM sync_manifest"
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// ### ⚖️ Governance: Blueprint Discovery
/// Loads all registered Role Blueprints from the database.
pub async fn load_blueprints(pool: &SqlitePool) -> Result<Vec<RoleBlueprint>, AppError> {
    let rows = sqlx::query_as::<_, RoleBlueprint>(
        "SELECT id, name, department, description, skills, workflows, mcp_tools, requires_oversight, model_id, created_at FROM role_blueprints"
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// ### ⚖️ Governance: Promote to Role
/// Persists a Role Blueprint to the database.
pub async fn save_blueprint(pool: &SqlitePool, blueprint: &RoleBlueprint) -> Result<(), AppError> {
    execute_save_blueprint(pool, blueprint).await
}

/// Transaction-compatible variant of `save_blueprint`.
pub async fn execute_save_blueprint<'c, E>(executor: E, blueprint: &RoleBlueprint) -> Result<(), AppError>
where
    E: sqlx::Executor<'c, Database = sqlx::Sqlite>,
{
    sqlx::query(
        "INSERT INTO role_blueprints (id, name, department, description, skills, workflows, mcp_tools, requires_oversight, model_id, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET
            name = excluded.name,
            department = excluded.department,
            description = excluded.description,
            skills = excluded.skills,
            workflows = excluded.workflows,
            mcp_tools = excluded.mcp_tools,
            requires_oversight = excluded.requires_oversight,
            model_id = excluded.model_id"
    )
    .bind(&blueprint.id)
    .bind(&blueprint.name)
    .bind(&blueprint.department)
    .bind(&blueprint.description)
    .bind(&blueprint.skills)
    .bind(&blueprint.workflows)
    .bind(&blueprint.mcp_tools)
    .bind(blueprint.requires_oversight)
    .bind(&blueprint.model_id)
    .bind(blueprint.created_at.unwrap_or_else(chrono::Utc::now))
    .execute(executor)
    .await?;
    Ok(())
}

/// ### ⚖️ Governance: Role Retirement
/// Deletes a Role Blueprint from the system.
pub async fn delete_blueprint<'c, E>(executor: E, id: &str) -> Result<bool, AppError>
where
    E: sqlx::Executor<'c, Database = sqlx::Sqlite>,
{
    let res = sqlx::query("DELETE FROM role_blueprints WHERE id = ?")
        .bind(id)
        .execute(executor)
        .await?;
    Ok(res.rows_affected() > 0)
}

/// Updates the status of a sync manifest.
pub async fn update_sync_status(pool: &SqlitePool, id: &str, status: &str) -> Result<(), AppError> {
    sqlx::query("UPDATE sync_manifest SET status = ? WHERE id = ?")
        .bind(status)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Records a successful sync completion.
pub async fn complete_sync(
    pool: &SqlitePool,
    id: &str,
    last_sync: chrono::DateTime<chrono::Utc>,
) -> Result<(), AppError> {
    sqlx::query("UPDATE sync_manifest SET last_sync_at = ?, status = 'idle' WHERE id = ?")
        .bind(last_sync)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Transactionally deletes an agent and cascades deletions to all associated metadata tables.
/// Enforces strict child-first ordering to comply with SQLite foreign key constraints.
pub async fn delete_agent_cascade(pool: &SqlitePool, agent_id: &str) -> Result<(), AppError> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;

    // Financial history must retain both counterparties. Give callers a useful
    // conflict before touching dependent records rather than a generic FK error.
    let has_ledger_history = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM a2a_ledger WHERE debit_agent_id = ?1 OR credit_agent_id = ?1)",
    )
    .bind(agent_id)
    .fetch_one(&mut *tx)
    .await?;
    if has_ledger_history {
        return Err(AppError::Conflict(
            "Agent has A2A ledger history and cannot be deleted; suspend the agent instead".to_string(),
        ));
    }

    // Preserve oversight evidence while detaching its optional mission FK.
    sqlx::query("UPDATE oversight_log SET mission_id = NULL WHERE mission_id IN (SELECT id FROM mission_history WHERE agent_id = ?1)")
        .bind(agent_id)
        .execute(&mut *tx)
        .await?;

    // 1. Delete granular mission execution logs and nodes (depend on mission_history)
    sqlx::query("DELETE FROM mission_logs WHERE agent_id = ?1 OR mission_id IN (SELECT id FROM mission_history WHERE agent_id = ?1)")
        .bind(agent_id)
        .execute(&mut *tx)
        .await?;

    sqlx::query("DELETE FROM swarm_context WHERE agent_id = ?1 OR mission_id IN (SELECT id FROM mission_history WHERE agent_id = ?1)")
        .bind(agent_id)
        .execute(&mut *tx)
        .await?;

    sqlx::query("DELETE FROM agent_directives WHERE source_agent_id = ?1 OR target_agent_id = ?1 OR mission_id IN (SELECT id FROM mission_history WHERE agent_id = ?1)")
        .bind(agent_id)
        .execute(&mut *tx)
        .await?;

    sqlx::query("DELETE FROM peer_reviews WHERE requester_id = ?1 OR reviewer_id = ?1 OR mission_id IN (SELECT id FROM mission_history WHERE agent_id = ?1)")
        .bind(agent_id)
        .execute(&mut *tx)
        .await?;

    sqlx::query("DELETE FROM mission_nodes WHERE mission_id IN (SELECT id FROM mission_history WHERE agent_id = ?1)")
        .bind(agent_id)
        .execute(&mut *tx)
        .await?;

    sqlx::query("DELETE FROM mission_quotas WHERE cluster_id IN (SELECT id FROM mission_history WHERE agent_id = ?1)")
        .bind(agent_id)
        .execute(&mut *tx)
        .await?;

    // 2. Delete parent missions referencing agent_id
    sqlx::query("DELETE FROM mission_history WHERE agent_id = ?")
        .bind(agent_id)
        .execute(&mut *tx)
        .await?;

    // 3. Delete metadata, tasks, quotas, and manifests
    sqlx::query("DELETE FROM agent_economics_meta WHERE agent_id = ?")
        .bind(agent_id)
        .execute(&mut *tx)
        .await?;

    sqlx::query("DELETE FROM agent_quotas WHERE entity_id = ?")
        .bind(agent_id)
        .execute(&mut *tx)
        .await?;

    sqlx::query("DELETE FROM sync_manifest WHERE agent_id = ?")
        .bind(agent_id)
        .execute(&mut *tx)
        .await?;

    sqlx::query("DELETE FROM fallback_memories WHERE agent_id = ?")
        .bind(agent_id)
        .execute(&mut *tx)
        .await?;

    sqlx::query("DELETE FROM agent_hires WHERE hiring_agent_id = ? OR target_agent_id = ?")
        .bind(agent_id)
        .bind(agent_id)
        .execute(&mut *tx)
        .await?;

    // FORENSIC INTEGRITY: audit_trail is an append-only cryptographic Merkle chain.
    // Deleting historical entries breaks hash linkage (prev_hash -> current_hash)
    // and violates audit immutability. Historical audit records are preserved intact.

    sqlx::query("DELETE FROM skill_subscriptions WHERE agent_id = ?")
        .bind(agent_id)
        .execute(&mut *tx)
        .await?;

    sqlx::query("DELETE FROM agent_tasks WHERE agent_id = ? OR claimed_by = ?")
        .bind(agent_id)
        .bind(agent_id)
        .execute(&mut *tx)
        .await?;

    // 4. Finally, delete the agent row itself (last, satisfying FKs)
    sqlx::query("DELETE FROM agents WHERE id = ?")
        .bind(agent_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
#[path = "persistence/tests.rs"]
mod tests;

// Metadata: [persistence]

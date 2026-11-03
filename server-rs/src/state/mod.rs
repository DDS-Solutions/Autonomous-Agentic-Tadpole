//! Global Application State & Thread-Safe Context - The Sovereign State
//!
//! The `AppState` acts as the single source of truth for the swarm, managing
//! authenticated session maps, database pools, and real-time telemetry hubs.
//!
//! @docs ARCHITECTURE:State
//! @docs OPERATIONS_MANUAL:Governance
//!
//! ### AI Assist Note
//! **The Sovereign State**: Acts as the single source of truth for the swarm.
//! Manages the **Telemetry Hub**, **Agent Registry**, **Governance
//! Policy**, and **Resource Pool**. All asynchronous workers MUST hold
//! an `Arc<AppState>` to remain synchronized with the global swarm state.
//! Features a **Multi-Hub Architecture** to isolate concerns across
//! communication, security, and persistence.
//!
//! ### 🔍 Debugging & Observability
//! - **Telemetry Link**: Search `[mod]` in tracing logs.
//! - **System Identity**: TadpoleOS/1.1.59
//! - **Failure Path**: Double-locking `parking_lot::RwLock` in nested
//!   callbacks, DB pool exhaustion during high-concurrency bursts, or
//!   state corruption due to out-of-order event broadcasts.
//! - **Trace Scope**: `server-rs::state` (Search for `[Engine]` or `[State]` tags)

use std::sync::Arc;
use tokio::sync::OnceCell;

use crate::system::actors::ActorRegistry;

pub mod boot;
pub mod hubs;
pub mod kernel;
pub mod persist;
pub mod session;
pub mod telemetry;

use hubs::comm::CommunicationHub;
use hubs::gov::GovernanceHub;
use hubs::reg::RegistryHub;
use hubs::res::ResourceHub;
use hubs::sec::SecurityHub;

/// The global application state shared across all routes via Axum State.
/// Decomposed into logical hubs for modularity.
pub struct AppState {
    /// Manages real-time communication channels (logs, events, telemetry, audio).
    pub comms: Arc<CommunicationHub>,
    /// Manages operational limits and policy settings.
    pub governance: Arc<GovernanceHub>,
    /// Manages entities like agents, providers, models, and skills.
    pub registry: Arc<RegistryHub>,
    /// Manages security features like auditing, budget enforcement, and scanning.
    pub security: Arc<SecurityHub>,
    /// Manages shared system resources (DB pool, HTTP client, file contexts).
    pub resources: Arc<ResourceHub>,
    /// Global workspace root directory for data persistence.
    pub base_dir: std::path::PathBuf,
    /// Registry of system actors (late-initialized).
    pub actors: OnceCell<ActorRegistry>,
    /// Barrier for system boot synchronization.
    pub boot_gate: (tokio::sync::watch::Sender<bool>, tokio::sync::watch::Receiver<bool>),
    /// Cancellation channel for passive hot-reload background loop.
    pub hot_reload_tx: tokio::sync::watch::Sender<bool>,
    /// Thread-safe pub-sub system event bus.
    #[allow(dead_code)]
    pub event_bus: Arc<crate::system::event_bus::SystemEventBus>,
    /// Server startup timestamp for uptime calculations.
    pub start_time: chrono::DateTime<chrono::Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::AppError;

    #[tokio::test]
    async fn test_revert_to_node_sovereign() {
        let state = AppState::new_minimal_mock().await;
        let pool = &state.resources.pool;

        // 0. Setup an agent
        sqlx::query(
            "INSERT INTO agents (id, name, role, department, description, status, metadata) VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind("test-agent")
        .bind("Test Agent")
        .bind("Specialist")
        .bind("IT")
        .bind("Test agent for sovereign revert flow")
        .bind("idle")
        .bind("{}")
        .execute(pool)
        .await
        .unwrap();

        // 1. Setup a mission
        let mission_id = "test-mission";
        sqlx::query(
            "INSERT INTO mission_history (id, title, status, agent_id) VALUES (?, ?, 'active', ?)",
        )
        .bind(mission_id)
        .bind("Test Mission")
        .bind("test-agent")
        .execute(pool)
        .await
        .unwrap();

        // 2. (a) Happy Path: Append two nodes atomically and verify active_node_id advances
        let node1 = state
            .append_session_node_sovereign(mission_id, None, "user", "Hello", None)
            .await
            .unwrap();
        let node2 = state
            .append_session_node_sovereign(
                mission_id,
                Some(node1.clone()),
                "assistant",
                "Hi",
                Some(serde_json::json!({ "tokens": 5 })),
            )
            .await
            .unwrap();

        let active_after_append: String =
            sqlx::query_scalar("SELECT active_node_id FROM mission_history WHERE id = ?")
                .bind(mission_id)
                .fetch_one(pool)
                .await
                .unwrap();
        assert_eq!(active_after_append, node2);

        // Traverse linear history from leaf node2
        let history = state
            .traverse_session_history_sovereign(&node2)
            .await
            .unwrap();
        assert_eq!(history.len(), 2);

        // 3. Revert to node1
        state
            .revert_to_node_sovereign(mission_id, &node1)
            .await
            .unwrap();

        let active_node: String =
            sqlx::query_scalar("SELECT active_node_id FROM mission_history WHERE id = ?")
                .bind(mission_id)
                .fetch_one(pool)
                .await
                .unwrap();
        assert_eq!(active_node, node1);

        // 4. (b) Failure Path: Reverting to a non-existent node returns BadRequest
        let err = state
            .revert_to_node_sovereign(mission_id, "non-existent-node")
            .await;
        assert!(matches!(err, Err(AppError::BadRequest(_))));

        // 5. (c) Edge Case Path: Traversing an unknown leaf returns an empty history vector
        let empty_history = state
            .traverse_session_history_sovereign("unknown-leaf")
            .await
            .unwrap();
        assert!(empty_history.is_empty());
    }

    #[tokio::test]
    async fn test_security_sovereign_pii_fails_closed_in_fallback() {
        let state = AppState::new_minimal_mock().await;
        // In fallback (actors not set), pii check must fail closed (Ok(false))
        let result = state
            .check_security_sovereign(
                "test-agent",
                "pii",
                serde_json::json!({ "text": "secret ss# 123-45-6789" }),
            )
            .await
            .unwrap();
        assert_eq!(
            result, false,
            "PII check in fallback must fail closed (Ok(false))"
        );
    }

    #[tokio::test]
    async fn test_save_agents_preserves_api_key_and_releases_locks() {
        let state = AppState::new_mock().await;
        let pool = &state.resources.pool;

        let mut agent = crate::agent::types::EngineAgent {
            identity: crate::agent::types::AgentIdentity {
                id: "agent-key-test".to_string(),
                name: "Test Agent".to_string(),
                role: "Developer".to_string(),
                department: "Eng".to_string(),
                description: "Testing credential preservation".to_string(),
                ..Default::default()
            },
            models: crate::agent::types::AgentModels {
                model: crate::agent::types::ModelConfig {
                    provider: crate::agent::types::ModelProvider::Openai,
                    api_key: Some("sk-secret-key-12345".to_string()),
                    base_url: Some("https://api.openai.com/v1".to_string()),
                    ..Default::default()
                },
                ..Default::default()
            },
            version: 1,
            ..Default::default()
        };

        // Create the agent in DB first
        agent.version = crate::agent::persistence::create_agent_db(pool, &agent)
            .await
            .expect("create agent in db");

        state
            .registry
            .agents
            .insert(agent.identity.id.clone(), agent.clone());

        // Save agents to DB (periodic 30s flush)
        state.save_agents().await.expect("save_agents must succeed");

        // Verify the api_key was persisted in DB
        let row: (Option<String>,) = sqlx::query_as("SELECT api_key FROM agents WHERE id = ?")
            .bind("agent-key-test")
            .fetch_one(pool)
            .await
            .expect("fetch agent row");
        assert_eq!(row.0.as_deref(), Some("sk-secret-key-12345"));

        // Run save_agents a second time (simulating 30s flush)
        state
            .save_agents()
            .await
            .expect("second save_agents must succeed");

        // Verify the api_key was NOT wiped/nulled by upsert
        let row2: (Option<String>,) = sqlx::query_as("SELECT api_key FROM agents WHERE id = ?")
            .bind("agent-key-test")
            .fetch_one(pool)
            .await
            .expect("fetch agent row 2");
        assert_eq!(
            row2.0.as_deref(),
            Some("sk-secret-key-12345"),
            "API key must not be nulled on subsequent saves"
        );
    }

    #[tokio::test]
    async fn test_record_audit_direct_fallback() {
        let state = AppState::new_minimal_mock().await;
        let entry = state
            .record_audit(
                "agent-1",
                Some("mission-1"),
                Some("user-1"),
                "action",
                "params",
            )
            .await
            .expect("record_audit should succeed via direct fallback");
        assert_eq!(entry.agent_id, "agent-1");
        assert_eq!(entry.action, "action");
    }
}

// Metadata: [mod]


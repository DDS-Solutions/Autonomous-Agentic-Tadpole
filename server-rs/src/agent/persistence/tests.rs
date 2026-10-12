//! @docs ARCHITECTURE:Persistence
//!
//! ### AI Assist Note
//! Persistence, deletion, recovery, and optimistic-concurrency regression fixtures.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Storage failure, invalid input, or ownership conflict.
//! - **Telemetry Link**: Search `[persistence]` in server logs.

use super::*;
use sqlx::SqlitePool;

async fn cascade_test_pool() -> SqlitePool {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:").await.unwrap();
    sqlx::query("PRAGMA foreign_keys = ON").execute(&pool).await.unwrap();
    crate::db::migrations::run_migrations(&pool).await.unwrap();
    for id in ["delete-target", "counterparty"] {
        sqlx::query("INSERT INTO agents (id, name, role, department, description, status, metadata) VALUES (?1, ?1, 'worker', 'engineering', '', 'idle', '{}')")
            .bind(id).execute(&pool).await.unwrap();
    }
    pool
}

#[tokio::test]
async fn test_delete_agent_preserves_oversight_and_removes_economics() {
    let pool = cascade_test_pool().await;
    sqlx::query("INSERT INTO mission_history (id, agent_id, title, status) VALUES ('mission', 'delete-target', 'test', 'completed')")
        .execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO oversight_log (id, mission_id, agent_id, params, status) VALUES ('review', 'mission', 'delete-target', '{}', 'approved')")
        .execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO agent_economics_meta (agent_id) VALUES ('delete-target')")
        .execute(&pool).await.unwrap();

    delete_agent_cascade(&pool, "delete-target").await.unwrap();
    let mission_id = sqlx::query_scalar::<_, Option<String>>("SELECT mission_id FROM oversight_log WHERE id = 'review'")
        .fetch_one(&pool).await.unwrap();
    assert!(mission_id.is_none(), "oversight evidence must survive mission cleanup");
    for query in [
        "SELECT count(*) FROM agents WHERE id = 'delete-target'",
        "SELECT count(*) FROM agent_economics_meta WHERE agent_id = 'delete-target'",
        "SELECT count(*) FROM mission_history WHERE id = 'mission'",
    ] {
        assert_eq!(sqlx::query_scalar::<_, i64>(query).fetch_one(&pool).await.unwrap(), 0);
    }
    assert!(sqlx::query("PRAGMA foreign_key_check").fetch_all(&pool).await.unwrap().is_empty());
    delete_agent_cascade(&pool, "missing-agent").await.unwrap();
    pool.close().await;
}

#[tokio::test]
async fn test_delete_agent_rejects_either_ledger_counterparty_without_data_loss() {
    let pool = cascade_test_pool().await;
    sqlx::query("INSERT INTO a2a_ledger (tx_id, debit_agent_id, credit_agent_id, amount_micros, status, lock_id) VALUES ('tx', 'delete-target', 'counterparty', 1, 'COMMITTED', 'lock')")
        .execute(&pool).await.unwrap();
    for id in ["delete-target", "counterparty"] {
        assert!(matches!(delete_agent_cascade(&pool, id).await, Err(AppError::Conflict(_))));
    }
    assert_eq!(sqlx::query_scalar::<_, i64>("SELECT count(*) FROM agents WHERE id IN ('delete-target', 'counterparty')")
        .fetch_one(&pool).await.unwrap(), 2);
    assert_eq!(sqlx::query_scalar::<_, i64>("SELECT count(*) FROM a2a_ledger")
        .fetch_one(&pool).await.unwrap(), 1);
    pool.close().await;
}

#[tokio::test]
async fn test_agent_persistence_round_trip() -> Result<(), AppError> {
    let pool = SqlitePool::connect("sqlite::memory:").await?;

    // 1. Setup Schema (Matching exactly the final state after migrations)
    sqlx::query(
        r#"CREATE TABLE agents (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        role TEXT NOT NULL,
        department TEXT NOT NULL,
        description TEXT NOT NULL,
         model_id TEXT,
         tokens_used INTEGER DEFAULT 0,
         status TEXT NOT NULL,
         current_task TEXT,
         input_tokens INTEGER DEFAULT 0,
         output_tokens INTEGER DEFAULT 0,
         theme_color TEXT,
        budget_usd REAL DEFAULT 0.0,
        cost_usd REAL DEFAULT 0.0,
        metadata TEXT NOT NULL,
        skills TEXT,
        workflows TEXT,
        mcp_tools TEXT,
        connector_configs TEXT,
        model_2 TEXT,
        model_3 TEXT,
        model_config2 TEXT,
        model_config3 TEXT,
        active_model_slot INTEGER DEFAULT 1,
        failure_count INTEGER DEFAULT 0,
        last_failure_at DATETIME,
        heartbeat_at DATETIME,
        active_mission TEXT,
        provider TEXT,
        api_key TEXT,
        base_url TEXT,
        system_prompt TEXT,
        temperature REAL,
        voice_id TEXT,
        voice_engine TEXT,
        category TEXT,
        requires_oversight BOOLEAN DEFAULT 0,
        working_memory TEXT DEFAULT '{}',
        stt_engine TEXT,
        version INTEGER DEFAULT 1,
        created_at DATETIME,
        runner_policy TEXT DEFAULT '{"max_concurrent":1,"resume_blocked_first":true,"preflight_checks":["context_version","skill_updates"]}'
    )"#,
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        "CREATE TABLE sync_manifest (
            id TEXT PRIMARY KEY,
            agent_id TEXT NOT NULL,
            source_type TEXT NOT NULL,
            source_uri TEXT NOT NULL,
            status TEXT NOT NULL,
            last_sync_at DATETIME
        )",
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        "CREATE TABLE skill_subscriptions (
            id TEXT PRIMARY KEY NOT NULL,
            agent_id TEXT NOT NULL,
            skill_id TEXT NOT NULL,
            approved_at INTEGER,
            scope_hash TEXT NOT NULL,
            subscription_status TEXT NOT NULL DEFAULT 'pending',
            installed_at INTEGER,
            last_updated_at INTEGER,
            notes TEXT,
            UNIQUE(agent_id, skill_id)
        )",
    )
    .execute(&pool)
    .await?;

    // 2. Create Agent with full config
    let mut agent = crate::agent::types::EngineAgent {
        identity: crate::agent::types::AgentIdentity {
            id: "test-agent-1".to_string(),
            name: "Test Agent".to_string(),
            role: "Tester".to_string(),
            department: "QA".to_string(),
            description: "A test agent".to_string(),
            theme_color: Some("#ff0000".to_string()),
            ..Default::default()
        },
        health: crate::agent::types::AgentHealth {
            status: "idle".to_string(),
            ..Default::default()
        },
        capabilities: crate::agent::types::AgentCapabilities {
            skills: vec!["testing".to_string()],
            ..Default::default()
        },
        models: crate::agent::types::AgentModels {
            model_id: Some("gemini-1.5-pro".to_string()),
            ..Default::default()
        },
        version: 1,
        ..Default::default()
    };

    agent.models.model.provider = crate::agent::types::ModelProvider::Anthropic;
    agent.models.model.model_id = "claude-3-5-sonnet".to_string();
    agent.models.model.api_key = Some("sk-test-123".to_string());
    agent.models.model.base_url = Some("https://api.anthropic.com".to_string());
    agent.models.model.system_prompt = Some("You are a tester.".to_string());
    agent.models.model.temperature = Some(0.7);

    agent.state.current_task = Some("Validating persistence parity".to_string());
    agent.economics.token_usage = TokenUsage {
        input_tokens: 321,
        output_tokens: 123,
        total_tokens: 444,
    };
    agent.economics.tokens_used = 2222;
    agent.state.working_memory = serde_json::json!({"milestone": "test-passed"});

    // 3. Save
    create_agent_db(&pool, &agent).await?;

    // 4. Load
    let agents = load_agents_db(&pool).await?;
    let loaded = agents
        .iter()
        .find(|a| a.identity.id == "test-agent-1")
        .expect("Agent not found");

    // 5. Assert Parity
    assert_eq!(loaded.identity.name, agent.identity.name);
    assert_eq!(loaded.models.model.provider, agent.models.model.provider);
    assert_eq!(loaded.models.model.model_id, agent.models.model.model_id);
    assert_eq!(loaded.models.model.api_key, None); // SEC-02: Plaintext API keys are never persisted
    assert_eq!(loaded.models.model.base_url, agent.models.model.base_url);
    assert_eq!(loaded.models.model.system_prompt, agent.models.model.system_prompt);
    assert_eq!(loaded.models.model.temperature, agent.models.model.temperature);
    assert_eq!(loaded.identity.theme_color, agent.identity.theme_color);
    assert_eq!(loaded.capabilities.skills, agent.capabilities.skills);
    assert_eq!(loaded.requires_oversight, agent.requires_oversight);
    assert_eq!(loaded.state.current_task, agent.state.current_task);
    assert_eq!(
        loaded.economics.token_usage.input_tokens,
        agent.economics.token_usage.input_tokens
    );
    assert_eq!(
        loaded.economics.token_usage.output_tokens,
        agent.economics.token_usage.output_tokens
    );
    assert_eq!(
        loaded.economics.token_usage.total_tokens,
        agent.economics.token_usage.total_tokens
    );
    assert_eq!(loaded.economics.tokens_used, agent.economics.tokens_used);
    assert_eq!(loaded.state.working_memory["milestone"], "test-passed");

    Ok(())
}

#[tokio::test]
async fn test_atomic_claiming_and_reaping() -> Result<(), AppError> {
    let pool = crate::db::init_db("sqlite::memory:?skip_seed=true").await?;

    let agent = crate::agent::types::EngineAgent {
        identity: crate::agent::types::AgentIdentity {
            id: "agent-1".to_string(),
            name: "Agent 1".to_string(),
            role: "Worker".to_string(),
            ..Default::default()
        },
        health: crate::agent::types::AgentHealth {
            status: "idle".to_string(),
            heartbeat_at: None,
            ..Default::default()
        },
        created_at: Some(chrono::Utc::now() - chrono::Duration::seconds(700)),
        version: 1,
        ..Default::default()
    };
    create_agent_db(&pool, &agent).await?;

    // 2. Test Claiming
    let success = claim_agent(&pool, "agent-1").await?;
    assert!(success, "First claim should succeed");

    let success_retry = claim_agent(&pool, "agent-1").await?;
    assert!(!success_retry, "Second claim on busy agent should fail");

    // 3. Test Reaping (Set heartbeat to 600s ago)
    let old_heartbeat = chrono::Utc::now() - chrono::Duration::seconds(600);
    sqlx::query("UPDATE agents SET heartbeat_at = ? WHERE id = 'agent-1'")
        .bind(old_heartbeat)
        .execute(&pool)
        .await?;

    let state = std::sync::Arc::new(crate::state::AppState::with_pool(pool.clone()).await);
    let mut registry_agent = agent.clone();
    registry_agent.health.status = "busy".to_string();
    registry_agent.health.heartbeat_at = Some(old_heartbeat);
    state.registry.agents.insert("agent-1".to_string(), registry_agent);
    let reaped = reap_stale_agents(&state, 300).await?;
    assert_eq!(reaped, 1, "Should reap 1 stale agent");

    let final_status: String =
        sqlx::query_scalar("SELECT status FROM agents WHERE id = 'agent-1'")
            .fetch_one(&pool)
            .await?;
    assert_eq!(final_status, "idle", "Reaped agent should be idle");

    Ok(())
}

#[tokio::test]
async fn test_delete_agent_cascade_foreign_keys_on() -> Result<(), AppError> {
    let pool = crate::db::init_db("sqlite::memory:?skip_seed=true").await?;
    sqlx::query("PRAGMA foreign_keys = ON;").execute(&pool).await?;

    // 1. Insert an agent
    let agent = crate::agent::types::EngineAgent {
        identity: crate::agent::types::AgentIdentity {
            id: "cascade-agent-1".to_string(),
            name: "Cascade Agent".to_string(),
            role: "Tester".to_string(),
            ..Default::default()
        },
        version: 1,
        ..Default::default()
    };
    create_agent_db(&pool, &agent).await?;

    // 2. Insert dependent mission, mission_log, swarm_context, and directives
    let mid = "test-mission-123";
    sqlx::query("INSERT INTO mission_history (id, agent_id, title, status, budget_usd, cost_usd) VALUES (?, ?, ?, ?, ?, ?)")
        .bind(mid)
        .bind("cascade-agent-1")
        .bind("Test Mission")
        .bind("pending")
        .bind(10.0)
        .bind(0.0)
        .execute(&pool)
        .await?;

    sqlx::query("INSERT INTO mission_logs (id, mission_id, agent_id, source, text, severity) VALUES (?, ?, ?, ?, ?, ?)")
        .bind("log-1")
        .bind(mid)
        .bind("cascade-agent-1")
        .bind("test")
        .bind("sample text")
        .bind("info")
        .execute(&pool)
        .await?;

    sqlx::query("INSERT INTO swarm_context (id, mission_id, agent_id, topic, finding) VALUES (?, ?, ?, ?, ?)")
        .bind("ctx-1")
        .bind(mid)
        .bind("cascade-agent-1")
        .bind("testing")
        .bind("sample finding")
        .execute(&pool)
        .await?;

    sqlx::query("INSERT INTO agent_directives (id, mission_id, source_agent_id, target_agent_id, instruction, status) VALUES (?, ?, ?, ?, ?, ?)")
        .bind("dir-1")
        .bind(mid)
        .bind("cascade-agent-1")
        .bind("cascade-agent-1")
        .bind("self-instruction")
        .bind("pending")
        .execute(&pool)
        .await?;

    sqlx::query("INSERT INTO agent_directives (id, mission_id, source_agent_id, target_agent_id, instruction, status) VALUES (?, ?, ?, ?, ?, ?)")
        .bind("dir-2")
        .bind(mid)
        .bind("cascade-agent-1")
        .bind("cascade-agent-1")
        .bind("self-instruction")
        .bind("pending")
        .execute(&pool)
        .await?;

    sqlx::query("INSERT INTO peer_reviews (id, mission_id, requester_id, reviewer_id, content_to_review, status) VALUES (?, ?, ?, ?, ?, ?)")
        .bind("pr-1")
        .bind(mid)
        .bind("cascade-agent-1")
        .bind("cascade-agent-1")
        .bind("artifact-data")
        .bind("requested")
        .execute(&pool)
        .await?;

    let now = chrono::Utc::now();
    sqlx::query("INSERT INTO agent_quotas (id, entity_id, budget_usd, used_usd, reset_period, last_reset_at, next_reset_at) VALUES (?, ?, ?, ?, ?, ?, ?)")
        .bind("quota-1")
        .bind("cascade-agent-1")
        .bind(50.0)
        .bind(0.0)
        .bind("daily")
        .bind(now)
        .bind(now)
        .execute(&pool)
        .await?;

    sqlx::query("INSERT INTO sync_manifest (id, agent_id, source_type, source_uri) VALUES (?, ?, ?, ?)")
        .bind("sm-1")
        .bind("cascade-agent-1")
        .bind("fs")
        .bind("/data")
        .execute(&pool)
        .await?;

    // 3. Delete cascade must succeed child-first without foreign key constraint failure
    delete_agent_cascade(&pool, "cascade-agent-1").await?;

    // 4. Verify all records across all tables are wiped cleanly
    let agent_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM agents WHERE id = 'cascade-agent-1'")
        .fetch_one(&pool)
        .await?;
    assert_eq!(agent_count, 0);

    let mission_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM mission_history WHERE id = ?")
        .bind(mid)
        .fetch_one(&pool)
        .await?;
    assert_eq!(mission_count, 0);

    let log_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM mission_logs WHERE mission_id = ?")
        .bind(mid)
        .fetch_one(&pool)
        .await?;
    assert_eq!(log_count, 0);

    let context_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM swarm_context WHERE mission_id = ?")
        .bind(mid)
        .fetch_one(&pool)
        .await?;
    assert_eq!(context_count, 0);

    let directive_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM agent_directives WHERE mission_id = ?")
        .bind(mid)
        .fetch_one(&pool)
        .await?;
    assert_eq!(directive_count, 0);

    let pr_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM peer_reviews WHERE mission_id = ?")
        .bind(mid)
        .fetch_one(&pool)
        .await?;
    assert_eq!(pr_count, 0);

    let quota_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM agent_quotas WHERE entity_id = 'cascade-agent-1'")
        .fetch_one(&pool)
        .await?;
    assert_eq!(quota_count, 0);

    let manifest_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sync_manifest WHERE agent_id = 'cascade-agent-1'")
        .fetch_one(&pool)
        .await?;
    assert_eq!(manifest_count, 0);

    Ok(())
}

#[tokio::test]
async fn test_reap_stale_agents_respects_null_heartbeat_grace_period() -> Result<(), AppError> {
    let pool = crate::db::init_db("sqlite::memory:?skip_seed=true").await?;

    // 1. Fresh agent created just now, currently working, heartbeat_at IS NULL
    let fresh_agent = crate::agent::types::EngineAgent {
        identity: crate::agent::types::AgentIdentity {
            id: "fresh-agent".to_string(),
            name: "Fresh Worker".to_string(),
            role: "Worker".to_string(),
            ..Default::default()
        },
        health: crate::agent::types::AgentHealth {
            status: "working".to_string(),
            heartbeat_at: None,
            ..Default::default()
        },
        created_at: Some(chrono::Utc::now()),
        version: 1,
        ..Default::default()
    };
    create_agent_db(&pool, &fresh_agent).await?;

    // Reaping with 30s threshold must NOT harvest the fresh worker
    let state = std::sync::Arc::new(crate::state::AppState::with_pool(pool.clone()).await);
    state.registry.agents.insert(fresh_agent.identity.id.clone(), fresh_agent);
    let reaped = reap_stale_agents(&state, 30).await?;
    assert_eq!(reaped, 0, "Fresh agent within grace window must not be reaped");

    let status: String = sqlx::query_scalar("SELECT status FROM agents WHERE id = 'fresh-agent'")
        .fetch_one(&pool)
        .await?;
    assert_eq!(status, "working");

    Ok(())
}

async fn stale_recovery_fixture(active_mission: bool) -> std::sync::Arc<crate::state::AppState> {
    let state = crate::test_support::state_with_agent("stale-owner", "http://127.0.0.1:9/v1").await;
    let mut agent = state.registry.agents.get("stale-owner").unwrap().clone();
    agent.health.status = "busy".to_string();
    agent.health.heartbeat_at = Some(chrono::Utc::now() - chrono::Duration::seconds(700));
    if active_mission {
        let mission = crate::agent::mission::create_mission(&state.resources.pool, "stale-owner", "Orphan", 1.0).await.unwrap();
        crate::agent::mission::update_mission(&state.resources.pool, &mission.id, crate::agent::types::MissionStatus::Active, 0.0).await.unwrap();
        agent.state.active_mission = Some(serde_json::json!({"id": mission.id}));
    }
    agent.version = save_agent_db(&state.resources.pool, &agent).await.unwrap();
    state.registry.agents.insert("stale-owner".to_string(), agent);
    state
}

#[tokio::test]
async fn finding_regression_stale_recovery_preserves_live_runner_then_recovers_finished_owner() {
    let state = stale_recovery_fixture(false).await;
    let version = state.registry.agents.get("stale-owner").unwrap().version;
    let task = tokio::spawn(std::future::pending::<()>());
    state.comms.active_runners.insert("stale-owner".to_string(), crate::state::hubs::comm::RunnerHandle::new(task.abort_handle(), task.id()));
    assert_eq!(reap_stale_agents(&state, 300).await.unwrap(), 0);
    assert!(!claim_agent(&state.resources.pool, "stale-owner").await.unwrap());
    assert!(!task.is_finished());
    assert_eq!(state.registry.agents.get("stale-owner").unwrap().version, version);
    assert_eq!(sqlx::query_scalar::<_, String>("SELECT status FROM agents WHERE id = 'stale-owner'").fetch_one(&state.resources.pool).await.unwrap(), "busy");
    task.abort();
    let _ = task.await;
    assert_eq!(reap_stale_agents(&state, 300).await.unwrap(), 1);
    assert_eq!(state.registry.agents.get("stale-owner").unwrap().health.status, "idle");
    assert_eq!(state.registry.agents.get("stale-owner").unwrap().version, version + 1);
    assert!(!state.comms.active_runners.contains_key("stale-owner"));
    assert!(claim_agent(&state.resources.pool, "stale-owner").await.unwrap());
}

#[tokio::test]
async fn finding_regression_stale_recovery_finalizes_orphan_atomically_and_rejects_old_snapshot() {
    let state = stale_recovery_fixture(true).await;
    let old = state.registry.agents.get("stale-owner").unwrap().clone();
    sqlx::query("CREATE TRIGGER reject_recovery BEFORE UPDATE ON agents BEGIN SELECT RAISE(ABORT, 'injected recovery failure'); END")
        .execute(&state.resources.pool).await.unwrap();
    assert!(reap_stale_agents(&state, 300).await.is_err());
    assert_eq!(state.registry.agents.get("stale-owner").unwrap().version, old.version);
    assert_eq!(sqlx::query_scalar::<_, String>("SELECT status FROM mission_history WHERE agent_id = 'stale-owner'").fetch_one(&state.resources.pool).await.unwrap(), "active");
    sqlx::query("DROP TRIGGER reject_recovery").execute(&state.resources.pool).await.unwrap();
    assert_eq!(reap_stale_agents(&state, 300).await.unwrap(), 1);
    assert_eq!(sqlx::query_scalar::<_, String>("SELECT status FROM mission_history WHERE agent_id = 'stale-owner'").fetch_one(&state.resources.pool).await.unwrap(), "failed");
    assert_eq!(state.registry.agents.get("stale-owner").unwrap().health.status, "idle");
    assert!(state.registry.agents.get("stale-owner").unwrap().state.active_mission.is_none());
    assert!(matches!(save_agent_db(&state.resources.pool, &old).await, Err(AppError::Conflict(_))));
    assert_eq!(reap_stale_agents(&state, 300).await.unwrap(), 0);
}

#[tokio::test]
async fn test_occ_conflict_detection_prevents_lost_updates() -> Result<(), AppError> {
    let pool = crate::db::init_db("sqlite::memory:?skip_seed=true").await?;

    let agent = crate::agent::types::EngineAgent {
        identity: crate::agent::types::AgentIdentity {
            id: "occ-agent-1".to_string(),
            name: "OCC Agent".to_string(),
            role: "Tester".to_string(),
            ..Default::default()
        },
        version: 1,
        ..Default::default()
    };

    // Initial save establishes version
    let v1 = create_agent_db(&pool, &agent).await?;
    assert_eq!(v1, 1);

    // Branch A updates agent
    let mut agent_a = agent.clone();
    agent_a.version = v1;
    agent_a.identity.name = "Updated by A".to_string();
    let v2 = save_agent_db(&pool, &agent_a).await?;
    assert_eq!(v2, 2);

    // Branch B tries to update using stale version 1
    let mut agent_b = agent.clone();
    agent_b.version = v1; // Stale!
    agent_b.identity.name = "Stale Update by B".to_string();

    let conflict_result = save_agent_db(&pool, &agent_b).await;
    assert!(
        matches!(conflict_result, Err(AppError::Conflict(_))),
        "Expected Conflict error on stale OCC version, got: {:?}",
        conflict_result
    );

    Ok(())
}

#[tokio::test]
async fn finding_regression_stale_recovery_continues_after_agent_local_write_failure() {
    let state = stale_recovery_fixture(true).await;
    let blocked = state.registry.agents.get("stale-owner").unwrap().clone();
    let mut healthy = blocked.clone();
    healthy.identity.id = "z-healthy-orphan".to_string();
    healthy.state.active_mission = None;
    healthy.version = create_agent_db(&state.resources.pool, &healthy).await.unwrap();
    let mission = crate::agent::mission::create_mission(&state.resources.pool, &healthy.identity.id, "Healthy orphan", 1.0).await.unwrap();
    crate::agent::mission::update_mission(&state.resources.pool, &mission.id, crate::agent::types::MissionStatus::Active, 0.0).await.unwrap();
    healthy.state.active_mission = Some(serde_json::json!({"id": mission.id}));
    healthy.version = save_agent_db(&state.resources.pool, &healthy).await.unwrap();
    state.registry.agents.insert(healthy.identity.id.clone(), healthy.clone());
    sqlx::query("CREATE TRIGGER reject_one_recovery BEFORE UPDATE ON agents WHEN OLD.id = 'stale-owner' BEGIN SELECT RAISE(ABORT, 'injected agent-local recovery failure'); END")
        .execute(&state.resources.pool).await.unwrap();
    assert!(matches!(reap_stale_agents(&state, 300).await, Err(AppError::Sqlx(_))));
    assert_eq!(state.registry.agents.get("stale-owner").unwrap().version, blocked.version);
    assert!(state.registry.agents.get("stale-owner").unwrap().state.active_mission.is_some());
    let recovered = state.registry.agents.get(&healthy.identity.id).unwrap().clone();
    assert_eq!(recovered.version, healthy.version + 1);
    assert_eq!(recovered.health.status, "idle");
    assert!(recovered.state.active_mission.is_none());
    assert_eq!(sqlx::query_scalar::<_, String>("SELECT status FROM mission_history WHERE id = ?")
        .bind(&mission.id).fetch_one(&state.resources.pool).await.unwrap(), "failed");
    sqlx::query("DROP TRIGGER reject_one_recovery").execute(&state.resources.pool).await.unwrap();
    assert_eq!(reap_stale_agents(&state, 300).await.unwrap(), 1);
    assert_eq!(state.registry.agents.get(&healthy.identity.id).unwrap().version, recovered.version);
}

#[tokio::test]
async fn finding_regression_parallel_writers_complete_without_deadlock_and_keep_occ() {
    let options = "sqlite::memory:".parse::<sqlx::sqlite::SqliteConnectOptions>().unwrap()
        .foreign_keys(true).busy_timeout(std::time::Duration::from_secs(10));
    let pool = sqlx::sqlite::SqlitePoolOptions::new().max_connections(4)
        .connect_with(options).await.unwrap();
    crate::db::migrations::run_migrations(&pool).await.unwrap();
    let initial_agents: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM agents").fetch_one(&pool).await.unwrap();
    let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(8));
    let mut writers = tokio::task::JoinSet::new();
    for index in 0..8 {
        let pool = pool.clone();
        let barrier = barrier.clone();
        writers.spawn(async move {
            let mut agent = EngineAgent::default();
            agent.identity.id = format!("parallel-writer-{index}");
            barrier.wait().await;
            agent.version = create_agent_db(&pool, &agent).await.unwrap();
            agent.identity.name = format!("Updated writer {index}");
            agent.version = save_agent_db(&pool, &agent).await.unwrap();
            agent
        });
    }
    let agents = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        let mut agents = Vec::new();
        while let Some(result) = writers.join_next().await { agents.push(result.unwrap()); }
        agents
    }).await.expect("parallel writer transactions did not finish");
    assert_eq!(agents.len(), 8);
    assert!(agents.iter().all(|agent| agent.version == 2));
    let original = agents[0].clone();
    let mut other = original.clone();
    other.identity.name = "Competing update".to_string();
    let (left, right) = tokio::join!(save_agent_db(&pool, &original), save_agent_db(&pool, &other));
    assert!(matches!((&left, &right), (Ok(3), Err(AppError::Conflict(_))) | (Err(AppError::Conflict(_)), Ok(3))));
    let rows = load_agents_db(&pool).await.unwrap();
    let stored = rows.iter().find(|agent| agent.identity.id == original.identity.id).unwrap();
    assert_eq!(stored.version, 3);
    let mut deletions = tokio::task::JoinSet::new();
    for agent in agents {
        let pool = pool.clone();
        deletions.spawn(async move { delete_agent_cascade(&pool, &agent.identity.id).await.unwrap(); });
    }
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while let Some(result) = deletions.join_next().await { result.unwrap(); }
    }).await.expect("parallel deletion transactions did not finish");
    assert_eq!(sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM agents WHERE id LIKE 'parallel-writer-%'")
        .fetch_one(&pool).await.unwrap(), 0);
    assert_eq!(sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM agents").fetch_one(&pool).await.unwrap(), initial_agents);
    assert!(sqlx::query("PRAGMA foreign_key_check").fetch_all(&pool).await.unwrap().is_empty());
}

// Metadata: [persistence_tests]

//! Agentic Engine Primitives - Integration Verification Suite
//!
//! Integration tests for Status Ledger, Receipt system, and Task Claim Lock.
//!
//! @docs ARCHITECTURE:AgenticEngine
//!
//! ### AI Assist Note
//! **Verification Strategy**: Uses `tower::ServiceExt` for in-memory request
//! dispatching. This avoids network overhead while testing the full Axum stack
//! including middleware.

use axum::{
    body::Body,
    http::{header::AUTHORIZATION, Request, StatusCode},
    routing::{get, post},
    Router,
};
use serde_json::json;
use std::sync::Arc;
use tower::ServiceExt;

use crate::{
    routes::{
        agent::create_agent,
        agentic_engine::{
            claim_task, get_status_ledger, post_receipt, update_status_ledger,
            get_context_packet, update_context_packet, get_subscribed_skills,
            subscribe_skill, approve_skill, get_maintenance_report,
        },
    },
    state::AppState,
};

async fn test_app() -> (Router, Arc<AppState>) {
    let app_state = Arc::new(
        AppState::new()
            .await
            .expect("Failed to initialize state for agentic engine tests"),
    );

    let protected = Router::new()
        .route("/v1/agents", post(create_agent))
        .route(
            "/v1/agents/{id}/status-ledger",
            get(get_status_ledger).put(update_status_ledger),
        )
        .route(
            "/v1/agents/{id}/tasks/{task_id}/claim",
            post(claim_task),
        )
        .route(
            "/v1/agents/{id}/tasks/{task_id}/receipts",
            post(post_receipt),
        )
        .route(
            "/v1/agents/{id}/context-packet",
            get(get_context_packet).put(update_context_packet),
        )
        .route(
            "/v1/agents/{id}/skills/subscribed",
            get(get_subscribed_skills),
        )
        .route(
            "/v1/agents/{id}/skills/{skill_id}/subscribe",
            post(subscribe_skill),
        )
        .route(
            "/v1/agents/{id}/skills/{skill_id}/approve",
            post(approve_skill),
        )
        .route(
            "/v1/agents/{id}/maintenance-report",
            get(get_maintenance_report),
        )
        .route(
            "/v1/oversight/token-burn",
            get(crate::routes::oversight::get_token_burn),
        )
        .route_layer(axum::middleware::from_fn_with_state(
            app_state.clone(),
            crate::middleware::auth::validate_token,
        ));

    let app = Router::new()
        .merge(protected)
        .route(
            "/metrics",
            get(crate::routes::health::metrics_handler),
        )
        .with_state(app_state.clone());

    app_state.registry.agents.clear(); // Ensure clean state
    (app, app_state)
}

fn valid_auth(state: &AppState) -> String {
    format!("Bearer {}", state.security.deploy_token)
}

#[tokio::test]
async fn test_status_ledger_lifecycle() {
    let (app, state) = test_app().await;

    // 1. Create an agent first (so references check out)
    let agent_id = "agent-agentic-123";
    let payload = json!({
        "id": agent_id,
        "name": "Agentic Engine Agent",
        "role": "Coordinator",
        "department": "Orchestration",
        "description": "Test Agent for status ledger",
        "status": "idle",
        "budgetUsd": 50.0,
        "costUsd": 0.0,
        "tokensUsed": 0,
        "tokenUsage": {
            "inputTokens": 0,
            "outputTokens": 0,
            "totalTokens": 0
        },
        "metadata": {},
        "skills": [],
        "workflows": [],
        "model": "gpt-4o",
        "modelConfig": {
            "provider": "openai",
            "modelId": "gpt-4o"
        }
    });

    let req = Request::builder()
        .method("POST")
        .uri("/v1/agents")
        .header(AUTHORIZATION, valid_auth(&state))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&payload).unwrap()))
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    // 2. GET status-ledger
    let req = Request::builder()
        .method("GET")
        .uri(format!("/v1/agents/{}/status-ledger", agent_id))
        .header(AUTHORIZATION, valid_auth(&state))
        .body(Body::empty())
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    
    let bytes = axum::body::to_bytes(res.into_body(), 10000).await.unwrap();
    let ledger: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(ledger["agentId"], agent_id);
    assert_eq!(ledger["automationState"], "manual");
    assert_eq!(ledger["lastQueueResult"], "none");

    // 3. PUT status-ledger
    let update_payload = json!({
        "automationState": "installed",
        "lastQueueResult": "checking",
        "notes": "Running agentic engine verify"
    });

    let req = Request::builder()
        .method("PUT")
        .uri(format!("/v1/agents/{}/status-ledger", agent_id))
        .header(AUTHORIZATION, valid_auth(&state))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&update_payload).unwrap()))
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // 4. GET again to verify
    let req = Request::builder()
        .method("GET")
        .uri(format!("/v1/agents/{}/status-ledger", agent_id))
        .header(AUTHORIZATION, valid_auth(&state))
        .body(Body::empty())
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    
    let bytes = axum::body::to_bytes(res.into_body(), 10000).await.unwrap();
    let ledger: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(ledger["automationState"], "installed");
    assert_eq!(ledger["lastQueueResult"], "checking");
    assert_eq!(ledger["notes"], "Running agentic engine verify");
}

#[tokio::test]
async fn test_task_claim_and_receipts() {
    let (app, state) = test_app().await;
    let agent_id = "agent-task-123";
    let task_id = "task-456";

    // 1. Create agent
    let payload = json!({
        "id": agent_id,
        "name": "Task Agent",
        "role": "Runner",
        "department": "QA",
        "description": "Test Agent for tasks",
        "status": "idle",
        "budgetUsd": 50.0,
        "costUsd": 0.0,
        "tokensUsed": 0,
        "tokenUsage": {
            "inputTokens": 0,
            "outputTokens": 0,
            "totalTokens": 0
        },
        "metadata": {},
        "skills": [],
        "workflows": [],
        "model": "gpt-4o",
        "modelConfig": {
            "provider": "openai",
            "modelId": "gpt-4o"
        }
    });

    let req = Request::builder()
        .method("POST")
        .uri("/v1/agents")
        .header(AUTHORIZATION, valid_auth(&state))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&payload).unwrap()))
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    // Seed task directly in DB
    sqlx::query(
        "INSERT INTO agent_tasks (id, agent_id, title, description, status, priority, metadata)
         VALUES (?1, ?2, 'Test Task', 'A task to claim', 'todo', 'normal', '{}')"
    )
    .bind(task_id)
    .bind(agent_id)
    .execute(&state.resources.pool)
    .await
    .unwrap();

    // 2. Claim task (Happy path)
    let req = Request::builder()
        .method("POST")
        .uri(format!("/v1/agents/{}/tasks/{}/claim", agent_id, task_id))
        .header(AUTHORIZATION, valid_auth(&state))
        .body(Body::empty())
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // 3. Claim again (Failure path: Conflict)
    let req = Request::builder()
        .method("POST")
        .uri(format!("/v1/agents/{}/tasks/{}/claim", agent_id, task_id))
        .header(AUTHORIZATION, valid_auth(&state))
        .body(Body::empty())
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CONFLICT);

    // 4. Post receipt (Happy path: complete task)
    let receipt_payload = json!({
        "receiptType": "done",
        "notes": "Task completed successfully"
    });

    let req = Request::builder()
        .method("POST")
        .uri(format!("/v1/agents/{}/tasks/{}/receipts", agent_id, task_id))
        .header(AUTHORIZATION, valid_auth(&state))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&receipt_payload).unwrap()))
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    // Verify task status in DB
    let status: String = sqlx::query_scalar("SELECT status FROM agent_tasks WHERE id = ?1")
        .bind(task_id)
        .fetch_one(&state.resources.pool)
        .await
        .unwrap();
    assert_eq!(status, "done");
}

#[tokio::test]
async fn test_context_packet_endpoints() {
    let (app, state) = test_app().await;
    let agent_id = "agent-context-999";

    // 1. Create agent
    let payload = json!({
        "id": agent_id,
        "name": "Context Agent",
        "role": "Tester",
        "department": "QA",
        "description": "Test Agent for context packet",
        "status": "idle",
        "budgetUsd": 10.0,
        "costUsd": 0.0,
        "tokensUsed": 0,
        "tokenUsage": { "inputTokens": 0, "outputTokens": 0, "totalTokens": 0 },
        "metadata": {},
        "skills": [],
        "workflows": [],
        "model": "gpt-4o",
        "modelConfig": { "provider": "openai", "modelId": "gpt-4o" }
    });
    let req = Request::builder()
        .method("POST")
        .uri("/v1/agents")
        .header(AUTHORIZATION, valid_auth(&state))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&payload).unwrap()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    // 2. GET context-packet (Should auto-create ledger and return default)
    let req = Request::builder()
        .method("GET")
        .uri(format!("/v1/agents/{}/context-packet", agent_id))
        .header(AUTHORIZATION, valid_auth(&state))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    
    let bytes = axum::body::to_bytes(res.into_body(), 10000).await.unwrap();
    let data: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(data["agentId"], agent_id);
    assert_eq!(data["contextVersion"], 1);
    assert_eq!(data["contextPacket"], json!({}));

    // 3. PUT context-packet
    let put_payload = json!({
        "contextVersion": 42,
        "contextPacket": {
            "api_key_override": "temp-test-key",
            "active_env": "production"
        }
    });
    let req = Request::builder()
        .method("PUT")
        .uri(format!("/v1/agents/{}/context-packet", agent_id))
        .header(AUTHORIZATION, valid_auth(&state))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&put_payload).unwrap()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // 4. GET again to verify
    let req = Request::builder()
        .method("GET")
        .uri(format!("/v1/agents/{}/context-packet", agent_id))
        .header(AUTHORIZATION, valid_auth(&state))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    
    let bytes = axum::body::to_bytes(res.into_body(), 10000).await.unwrap();
    let data: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(data["contextVersion"], 42);
    assert_eq!(data["contextPacket"]["api_key_override"], "temp-test-key");
}

#[tokio::test]
async fn test_skill_subscriptions_endpoints() {
    use crate::agent::skill_manifest::{SkillManifest, Permission};
    let (app, state) = test_app().await;
    let agent_id = "agent-skills-888";
    let skill_id = "rust_compiler_skill";

    // Register mock skill in global registry
    state.registry.skill_registry.insert(SkillManifest {
        name: skill_id.to_string(),
        requires_oversight: true,
        permissions: vec![Permission::FilesystemWrite],
        ..Default::default()
    });

    // 1. Create agent
    let payload = json!({
        "id": agent_id,
        "name": "Skills Agent",
        "role": "QA",
        "department": "Security",
        "description": "Test Agent for skills",
        "status": "idle",
        "budgetUsd": 20.0,
        "costUsd": 0.0,
        "tokensUsed": 0,
        "tokenUsage": { "inputTokens": 0, "outputTokens": 0, "totalTokens": 0 },
        "metadata": {},
        "skills": [],
        "workflows": [],
        "model": "gpt-4o",
        "modelConfig": { "provider": "openai", "modelId": "gpt-4o" }
    });
    let req = Request::builder()
        .method("POST")
        .uri("/v1/agents")
        .header(AUTHORIZATION, valid_auth(&state))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&payload).unwrap()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    // 2. Subscribe (Should be pending because it's first time or oversight required)
    let sub_payload = json!({
        "notes": "Need compiler tool for verification"
    });
    let req = Request::builder()
        .method("POST")
        .uri(format!("/v1/agents/{}/skills/{}/subscribe", agent_id, skill_id))
        .header(AUTHORIZATION, valid_auth(&state))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&sub_payload).unwrap()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let bytes = axum::body::to_bytes(res.into_body(), 10000).await.unwrap();
    let sub_res: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(sub_res["subscriptionStatus"], "pending");

    // 3. GET subscribed skills
    let req = Request::builder()
        .method("GET")
        .uri(format!("/v1/agents/{}/skills/subscribed", agent_id))
        .header(AUTHORIZATION, valid_auth(&state))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    
    let bytes = axum::body::to_bytes(res.into_body(), 10000).await.unwrap();
    let subs: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(subs.is_array());
    assert_eq!(subs.as_array().unwrap().len(), 1);
    assert_eq!(subs[0]["skillId"], skill_id);
    assert_eq!(subs[0]["subscriptionStatus"], "pending");

    // 4. Approve skill
    let req = Request::builder()
        .method("POST")
        .uri(format!("/v1/agents/{}/skills/{}/approve", agent_id, skill_id))
        .header(AUTHORIZATION, valid_auth(&state))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // 5. GET subscribed again to verify approval
    let req = Request::builder()
        .method("GET")
        .uri(format!("/v1/agents/{}/skills/subscribed", agent_id))
        .header(AUTHORIZATION, valid_auth(&state))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    
    let bytes = axum::body::to_bytes(res.into_body(), 10000).await.unwrap();
    let subs: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(subs[0]["subscriptionStatus"], "approved");
}

#[tokio::test]
async fn test_token_burn_and_maintenance_report() {
    let (app, state) = test_app().await;
    let agent_id = "agent-maintenance-777";
    let task_id_1 = "task-burn-1";
    let task_id_2 = "task-burn-2";

    // 1. Create agent
    let payload = json!({
        "id": agent_id,
        "name": "Maintenance Agent",
        "role": "QA",
        "department": "Infrastructure",
        "description": "Test Agent for maintenance metrics",
        "status": "idle",
        "budgetUsd": 10.0,
        "costUsd": 5.0, // 50% budget consumed
        "tokensUsed": 1000,
        "tokenUsage": { "inputTokens": 600, "outputTokens": 400, "totalTokens": 1000 },
        "metadata": {},
        "skills": [],
        "workflows": [],
        "model": "gpt-4o",
        "modelConfig": { "provider": "openai", "modelId": "gpt-4o" }
    });
    
    let req = Request::builder()
        .method("POST")
        .uri("/v1/agents")
        .header(AUTHORIZATION, valid_auth(&state))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&payload).unwrap()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    // Seed completed tasks with token consumption in database
    sqlx::query(
        "INSERT INTO agent_tasks (id, agent_id, title, description, status, priority, tokens_in, tokens_out, cost_usd, provider_id, claimed_at, updated_at, current_receipt)
         VALUES 
         (?1, ?2, 'Task 1', 'Desc 1', 'done', 'normal', 100, 200, 0.5, 'openai', 1000, 1010, 'done'),
         (?3, ?2, 'Task 2', 'Desc 2', 'done', 'normal', 300, 400, 1.0, 'openai', 1000, 1025, 'done')"
    )
    .bind(task_id_1)
    .bind(agent_id)
    .bind(task_id_2)
    .execute(&state.resources.pool)
    .await
    .unwrap();

    // 2. GET /v1/oversight/token-burn
    let req = Request::builder()
        .method("GET")
        .uri("/v1/oversight/token-burn")
        .header(AUTHORIZATION, valid_auth(&state))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let bytes = axum::body::to_bytes(res.into_body(), 10000).await.unwrap();
    let data: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(data["totalTokensIn"], 400);
    assert_eq!(data["totalTokensOut"], 600);
    assert_eq!(data["totalCostUsd"], 1.5);
    assert!(data["providers"].is_array());
    assert!(data["agents"].is_array());
    assert!(data["recentBurns"].is_array());

    // 3. GET /v1/agents/:id/maintenance-report
    let req = Request::builder()
        .method("GET")
        .uri(format!("/v1/agents/{}/maintenance-report", agent_id))
        .header(AUTHORIZATION, valid_auth(&state))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let bytes = axum::body::to_bytes(res.into_body(), 10000).await.unwrap();
    let data: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(data["agentId"], agent_id);
    assert!(data["overallScore"].as_f64().unwrap() > 0.0);
    assert_eq!(data["budgetHeadroom"]["status"], "optimal"); // 50% headroom is optimal (warning threshold is < 25%)
    assert_eq!(data["latency"]["status"], "optimal"); // 10s and 25s latency are both < 60s
    assert_eq!(data["errorRate"]["status"], "optimal"); // 0 failed tasks

    // 4. GET /metrics and verify content format & new gauges exist
    let req = Request::builder()
        .method("GET")
        .uri("/metrics")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    
    let bytes = axum::body::to_bytes(res.into_body(), 50000).await.unwrap();
    let metrics_text = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(metrics_text.contains("oversight_tokens_in"));
    assert!(metrics_text.contains("oversight_tokens_out"));
    assert!(metrics_text.contains("oversight_cost_usd"));
}


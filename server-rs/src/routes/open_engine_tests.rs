//! Open Engine Primitives - Integration Verification Suite
//!
//! Integration tests for Status Ledger, Receipt system, and Task Claim Lock.
//!
//! @docs ARCHITECTURE:OpenEngine
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
        open_engine::{claim_task, get_status_ledger, post_receipt, update_status_ledger},
    },
    state::AppState,
};

async fn test_app() -> (Router, Arc<AppState>) {
    let app_state = Arc::new(
        AppState::new()
            .await
            .expect("Failed to initialize state for open engine tests"),
    );

    let app = Router::new()
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
        .route_layer(axum::middleware::from_fn_with_state(
            app_state.clone(),
            crate::middleware::auth::validate_token,
        ))
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
    let agent_id = "agent-open-123";
    let payload = json!({
        "id": agent_id,
        "name": "Open Engine Agent",
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
        "notes": "Running open engine verify"
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
    assert_eq!(ledger["notes"], "Running open engine verify");
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

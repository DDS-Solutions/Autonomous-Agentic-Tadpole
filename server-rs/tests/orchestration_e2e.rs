//! @docs ARCHITECTURE:Runner
//!
//! ### AI Context Alignment
//! - **Subsystem**: Sovereign Engine / End-to-End Orchestration Integration Tests
//! - **Primary Entrypoints**:
//!   - `e2e_diamond_dag_blackboard_and_raii_cleanup`
//!   - `e2e_durable_workflow_crash_and_resume`
//!   - `e2e_cascade_router_tier2_escalation`
//!   - `e2e_observe_mode_blocks_blackboard_write_and_allows_read`
//!   - `e2e_cooperative_mission_handle_abort_cancels_child_wave`
//!
//! ### ⚠️ Invariants & Non-Negotiables
//! - `[Scheduler]` Cooperative `MissionHandle` abort cancels active child waves without leaving orphaned tasks or blackboard state.
//! - `[Traceability]` Telemetry targets `[orchestration_e2e]` during test execution and preserves `User-Agent: TadpoleOS/1.1.59`.
//!
//! ### 🔍 Debugging & Observability
//! - **Telemetry Link**: Search `[orchestration_e2e]` in tracing logs.
//! - **Witness Tests**: Self-contained end-to-end orchestration suite driving `AgentRunner::run`.

use axum::{
    http::{HeaderMap, StatusCode},
    routing::{any, post},
    Json, Router,
};
use parking_lot::Mutex;
use server_rs::agent::persistence::{create_agent_db, load_agents_db, save_agent_db};
use server_rs::agent::runner::AgentRunner;
use server_rs::agent::types::{
    AcceptMode, EngineAgent, ExecutionPlane, ModelConfig, ModelProvider, TaskPayload,
};
use server_rs::db::init_db;
use server_rs::security::metering::BudgetGuard;
use server_rs::security::permissions::PermissionMode;
use server_rs::state::AppState;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Once};
use std::time::Duration;
use tokio::sync::{Barrier, Notify};
use tokio::task::JoinHandle;
use tokio::time::timeout;

const EXPECTED_USER_AGENT: &str = concat!("TadpoleOS/", env!("CARGO_PKG_VERSION"));
const TEST_TIMEOUT: Duration = Duration::from_secs(60);

static INIT_TRACING: Once = Once::new();

fn init_test_tracing() {
    INIT_TRACING.call_once(|| {
        let _ = tracing_subscriber::fmt().with_test_writer().try_init();
    });
}

type MockHandler = Arc<
    dyn Fn(
            usize,
            serde_json::Value,
        ) -> Pin<Box<dyn Future<Output = (StatusCode, serde_json::Value)> + Send>>
        + Send
        + Sync,
>;

struct MockOpenAiServer {
    base_url: String,
    calls: Arc<AtomicUsize>,
    violations: Arc<Mutex<Vec<String>>>,
    server: JoinHandle<()>,
}

impl MockOpenAiServer {
    async fn start(handler: MockHandler) -> Self {
        let calls = Arc::new(AtomicUsize::new(0));
        let violations: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let calls_ref = Arc::clone(&calls);
        let violations_ref = Arc::clone(&violations);
        let unrouted_ref = Arc::clone(&violations);

        let app = Router::new()
            .route(
                "/v1/chat/completions",
                post(move |headers: HeaderMap, Json(body): Json<serde_json::Value>| {
                    let calls = Arc::clone(&calls_ref);
                    let violations = Arc::clone(&violations_ref);
                    let handler = Arc::clone(&handler);
                    async move {
                        let ua = headers
                            .get("user-agent")
                            .and_then(|v| v.to_str().ok())
                            .unwrap_or_default();
                        if ua != EXPECTED_USER_AGENT {
                            let msg = format!(
                                "Expected User-Agent '{}', received '{}'",
                                EXPECTED_USER_AGENT, ua
                            );
                            violations.lock().push(msg.clone());
                            return (
                                StatusCode::BAD_REQUEST,
                                Json(openai_error_response("invalid_user_agent", &msg)),
                            );
                        }
                        if !body.get("messages").is_some_and(|m| m.is_array()) {
                            let msg = "OpenAI request body missing 'messages' array".to_string();
                            violations.lock().push(msg.clone());
                            return (
                                StatusCode::BAD_REQUEST,
                                Json(openai_error_response("invalid_request", &msg)),
                            );
                        }
                        let idx = calls.fetch_add(1, Ordering::SeqCst);
                        let (status, payload) = handler(idx, body).await;
                        (status, Json(payload))
                    }
                }),
            )
            .fallback(any(move |uri: axum::http::Uri| {
                let violations = Arc::clone(&unrouted_ref);
                async move {
                    let msg = format!("Unexpected mock HTTP route: {}", uri);
                    violations.lock().push(msg.clone());
                    (
                        StatusCode::NOT_FOUND,
                        Json(openai_error_response("not_found", &msg)),
                    )
                }
            }));

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind local mock openai listener");
        let base_url = format!("http://{}/v1", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        Self {
            base_url,
            calls,
            violations,
            server,
        }
    }

    fn take_violations(&self) -> Vec<String> {
        std::mem::take(&mut *self.violations.lock())
    }
}

impl Drop for MockOpenAiServer {
    fn drop(&mut self) {
        self.server.abort();
    }
}

fn openai_error_response(err_type: &str, message: &str) -> serde_json::Value {
    serde_json::json!({
        "error": {
            "message": message,
            "type": err_type,
            "code": err_type
        }
    })
}

fn openai_text_response(model: &str, text: &str) -> serde_json::Value {
    let call_num = uuid::Uuid::new_v4().simple().to_string();
    serde_json::json!({
        "id": format!("chatcmpl-e2e-{}", &call_num[..12]),
        "object": "chat.completion",
        "created": 1_760_000_000_i64,
        "model": model,
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": text
            },
            "finish_reason": "stop"
        }],
        "usage": {
            "prompt_tokens": 12,
            "completion_tokens": 8,
            "total_tokens": 20
        }
    })
}

fn openai_tool_response(model: &str, tool_name: &str, args: serde_json::Value) -> serde_json::Value {
    let args_json = serde_json::to_string(&args).expect("serialize tool arguments");
    let uid = uuid::Uuid::new_v4().simple().to_string();
    serde_json::json!({
        "id": format!("chatcmpl-e2e-{}", &uid[..12]),
        "object": "chat.completion",
        "created": 1_760_000_000_i64,
        "model": model,
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": null,
                "tool_calls": [{
                    "id": format!("call_e2e_{}", &uid[..12]),
                    "type": "function",
                    "function": {
                        "name": tool_name,
                        "arguments": args_json
                    }
                }]
            },
            "finish_reason": "tool_calls"
        }],
        "usage": {
            "prompt_tokens": 15,
            "completion_tokens": 10,
            "total_tokens": 25
        }
    })
}

fn req_model(body: &serde_json::Value) -> &str {
    body.get("model").and_then(|v| v.as_str()).unwrap_or("")
}

fn flatten_messages(body: &serde_json::Value) -> String {
    body.get("messages")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .map(|m| {
                    let role = m.get("role").and_then(|r| r.as_str()).unwrap_or("");
                    let content = m.get("content").and_then(|c| c.as_str()).unwrap_or("");
                    format!("[{role}]: {content}")
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

fn messages_with_tool_or_user_containing(body: &serde_json::Value, needle: &str) -> bool {
    body.get("messages")
        .and_then(|v| v.as_array())
        .is_some_and(|arr| {
            arr.iter().any(|m| {
                let role = m.get("role").and_then(|r| r.as_str()).unwrap_or("");
                role != "system"
                    && m.get("content")
                        .and_then(|c| c.as_str())
                        .is_some_and(|c| c.contains(needle))
            })
        })
}

async fn configure_e2e_state(mut state: AppState) -> Arc<AppState> {
    Arc::get_mut(&mut state.security).unwrap().budget_guard =
        Arc::new(BudgetGuard::new(state.resources.pool.clone()));
    Arc::get_mut(&mut state.resources).unwrap().http_client = Arc::new(
        reqwest::Client::builder()
            .no_proxy()
            .user_agent(EXPECTED_USER_AGENT)
            .build()
            .expect("build loopback http client"),
    );
    Arc::get_mut(&mut state.resources).unwrap().inference_semaphore =
        Arc::new(tokio::sync::Semaphore::new(32));
    // Autonomous E2E swarms explicitly opt into spawn_subagent execution without interactive oversight.
    state
        .security
        .permission_policy
        .set_mode("spawn_subagent", PermissionMode::Allow)
        .await;
    Arc::new(state)
}

async fn setup_e2e_state() -> Arc<AppState> {
    init_test_tracing();
    configure_e2e_state(AppState::new_minimal_mock().await).await
}

async fn setup_e2e_state_with_pool(pool: sqlx::SqlitePool) -> Arc<AppState> {
    init_test_tracing();
    configure_e2e_state(AppState::with_pool(pool).await).await
}

async fn register_e2e_agent(
    state: &Arc<AppState>,
    id: &str,
    role: &str,
    base_url: &str,
    max_turns: u32,
    cascade_routing: bool,
) {
    let mut agent = EngineAgent::default();
    agent.identity.id = id.to_string();
    agent.identity.name = id.to_string();
    agent.identity.role = role.to_string();
    agent.economics.budget_usd = 50.0;
    agent.requires_oversight = false;
    agent.runner_policy.cascade_routing = cascade_routing;
    agent.models.model = ModelConfig {
        provider: ModelProvider::Openai,
        model_id: format!("e2e-{}", id),
        api_key: Some("e2e-test-key".to_string()),
        base_url: Some(base_url.to_string()),
        max_turns: Some(max_turns),
        input_cost_per_m: Some(0.0),
        output_cost_per_m: Some(0.0),
        ..Default::default()
    };
    agent.version = create_agent_db(&state.resources.pool, &agent)
        .await
        .expect("persist e2e agent");
    state.registry.agents.insert(id.to_string(), agent);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_diamond_dag_blackboard_and_raii_cleanup() {
    timeout(TEST_TIMEOUT, async {
        tracing::info!("[orchestration_e2e] Starting diamond DAG + blackboard + RAII cleanup test");
        let wave_log: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let wave2_barrier = Arc::new(Barrier::new(2));
        let violations: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

        let wave_log_ref = Arc::clone(&wave_log);
        let barrier_ref = Arc::clone(&wave2_barrier);
        let violations_ref = Arc::clone(&violations);

        let server = MockOpenAiServer::start(Arc::new(move |_idx, body| {
            let wave_log = Arc::clone(&wave_log_ref);
            let barrier = Arc::clone(&barrier_ref);
            let violations = Arc::clone(&violations_ref);
            Box::pin(async move {
                let model = req_model(&body).to_string();
                let full = flatten_messages(&body);

                // Route child task nonces BEFORE root goal nonce: derive_subtask_payload inherits
                // the parent's model_id and Socratic Context Envelope embeds primary_goal in child prompts.
                if full.contains("Your swarm reported back with these pooled results:") {
                    wave_log.lock().push("root:pool_synthesis".to_string());
                    if !full.contains("FINAL_D:secret-dag-token") {
                        violations
                            .lock()
                            .push("Pooled synthesis prompt missing FINAL_D:secret-dag-token".into());
                    }
                    return (
                        StatusCode::OK,
                        openai_text_response(&model, "SYNTHESIS:FINAL_D:secret-dag-token"),
                    );
                }
                if messages_with_tool_or_user_containing(&body, "SYNTHESIS:FINAL_D:secret-dag-token") {
                    wave_log.lock().push("root:synthesize".to_string());
                    return (
                        StatusCode::OK,
                        openai_text_response(&model, "DIAMOND_ORCHESTRATION_OK"),
                    );
                }
                if messages_with_tool_or_user_containing(&body, "STEP_D_5A7C") {
                    if messages_with_tool_or_user_containing(&body, "secret-dag-token") {
                        wave_log.lock().push("finish:D".to_string());
                        return (
                            StatusCode::OK,
                            openai_text_response(&model, "FINAL_D:secret-dag-token"),
                        );
                    }
                    if !messages_with_tool_or_user_containing(&body, "OUTPUT_FROM_B")
                        || !messages_with_tool_or_user_containing(&body, "OUTPUT_FROM_C")
                    {
                        violations.lock().push(
                            "Task D missing upstream OUTPUT_FROM_B / OUTPUT_FROM_C".into(),
                        );
                    }
                    if !full.contains("shared_finding") {
                        violations
                            .lock()
                            .push("Task D missing SharedBlackboard summary 'shared_finding'".into());
                    }
                    wave_log.lock().push("start:D".to_string());
                    return (
                        StatusCode::OK,
                        openai_tool_response(
                            &model,
                            "blackboard_read",
                            serde_json::json!({ "key": "shared_finding" }),
                        ),
                    );
                }
                if messages_with_tool_or_user_containing(&body, "STEP_C_8D1E") {
                    if !messages_with_tool_or_user_containing(&body, "OUTPUT_FROM_A") {
                        violations
                            .lock()
                            .push("Task C prompt missing upstream OUTPUT_FROM_A".into());
                    }
                    if timeout(Duration::from_secs(5), barrier.wait()).await.is_err() {
                        violations
                            .lock()
                            .push("Wave 2 concurrency barrier timed out in worker-c".into());
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            openai_error_response("barrier_timeout", "worker-c concurrency barrier timeout"),
                        );
                    }
                    wave_log.lock().push("run:C".to_string());
                    return (StatusCode::OK, openai_text_response(&model, "OUTPUT_FROM_C"));
                }
                if messages_with_tool_or_user_containing(&body, "STEP_B_2C4B") {
                    if !messages_with_tool_or_user_containing(&body, "OUTPUT_FROM_A") {
                        violations
                            .lock()
                            .push("Task B prompt missing upstream OUTPUT_FROM_A".into());
                    }
                    // Proves Wave 2 (worker-b & worker-c) runs concurrently across wave tasks.
                    if timeout(Duration::from_secs(5), barrier.wait()).await.is_err() {
                        violations
                            .lock()
                            .push("Wave 2 concurrency barrier timed out in worker-b".into());
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            openai_error_response("barrier_timeout", "worker-b concurrency barrier timeout"),
                        );
                    }
                    wave_log.lock().push("run:B".to_string());
                    return (StatusCode::OK, openai_text_response(&model, "OUTPUT_FROM_B"));
                }
                if messages_with_tool_or_user_containing(&body, "STEP_A_9F1A") {
                    if messages_with_tool_or_user_containing(&body, "Blackboard key 'shared_finding' updated")
                    {
                        wave_log.lock().push("finish:A".to_string());
                        return (StatusCode::OK, openai_text_response(&model, "OUTPUT_FROM_A"));
                    }
                    wave_log.lock().push("start:A".to_string());
                    return (
                        StatusCode::OK,
                        openai_tool_response(
                            &model,
                            "blackboard_write",
                            serde_json::json!({
                                "key": "shared_finding",
                                "value": "secret-dag-token",
                                "tags": ["e2e", "dag"]
                            }),
                        ),
                    );
                }
                if messages_with_tool_or_user_containing(&body, "RUN_DIAMOND_DAG_NONCE_11A")
                    && !full.contains("EXECUTION PROTOCOL")
                {
                    wave_log.lock().push("root:spawn".to_string());
                    return (
                        StatusCode::OK,
                        openai_tool_response(
                            &model,
                            "spawn_subagent",
                            serde_json::json!({
                                "tasks": [
                                    {
                                        "id": "task-a",
                                        "agent_id": "worker-a",
                                        "message": "STEP_A_9F1A discover token"
                                    },
                                    {
                                        "id": "task-b",
                                        "agent_id": "worker-b",
                                        "message": "STEP_B_2C4B enrich branch 1",
                                        "depends_on": ["task-a"]
                                    },
                                    {
                                        "id": "task-c",
                                        "agent_id": "worker-c",
                                        "message": "STEP_C_8D1E enrich branch 2",
                                        "depends_on": ["task-a"]
                                    },
                                    {
                                        "id": "task-d",
                                        "agent_id": "worker-d",
                                        "message": "STEP_D_5A7C aggregate findings",
                                        "depends_on": ["task-b", "task-c"]
                                    }
                                ]
                            }),
                        ),
                    );
                }

                let msg = format!("Unmatched mock request for model '{}'", model);
                violations.lock().push(msg.clone());
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    openai_error_response("unmatched_route", &msg),
                )
            })
        }))
        .await;

        let state = setup_e2e_state().await;
        register_e2e_agent(&state, "root-ceo", "CEO", &server.base_url, 6, false).await;
        for worker in ["worker-a", "worker-b", "worker-c", "worker-d"] {
            register_e2e_agent(&state, worker, "Specialist", &server.base_url, 4, false).await;
        }

        let runner = AgentRunner::new(Arc::clone(&state));
        let output = runner
            .run(
                "root-ceo".to_string(),
                TaskPayload {
                    message: "RUN_DIAMOND_DAG_NONCE_11A".to_string(),
                    budget_usd: Some(25.0),
                    execution_plane: Some(ExecutionPlane::EngineMaintenance),
                    accept_mode: Some(AcceptMode::Auto),
                    ..Default::default()
                },
            )
            .await
            .expect("diamond DAG mission should succeed");

        assert!(
            violations.lock().is_empty() && server.take_violations().is_empty(),
            "Handler/server contract violations: {:?}",
            violations.lock()
        );
        assert_eq!(output, "DIAMOND_ORCHESTRATION_OK");

        let events = wave_log.lock().clone();
        let pos_finish_a = events.iter().position(|e| e == "finish:A").expect("A finished");
        let pos_b = events.iter().position(|e| e == "run:B").expect("B ran");
        let pos_c = events.iter().position(|e| e == "run:C").expect("C ran");
        let pos_start_d = events.iter().position(|e| e == "start:D").expect("D started");

        assert!(
            pos_finish_a < pos_b && pos_finish_a < pos_c,
            "Wave 1 (A) must finish before Wave 2 (B, C): {:?}",
            events
        );
        assert!(
            pos_b < pos_start_d && pos_c < pos_start_d,
            "Wave 2 (B, C) must finish before Wave 3 (D): {:?}",
            events
        );

        assert!(
            state.comms.mission_handles.is_empty(),
            "Root mission_handles should be cleaned up on mission exit"
        );
        assert!(
            state.comms.blackboard.is_empty(),
            "SharedBlackboard should have zero remaining partitions after RAII drop; active: {:?}",
            state.comms.blackboard.active_mission_ids()
        );
    })
    .await
    .expect("e2e_diamond_dag_blackboard_and_raii_cleanup timed out");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_durable_workflow_crash_and_resume() {
    timeout(TEST_TIMEOUT, async {
        tracing::info!("[orchestration_e2e] Starting durable workflow crash & real DB restart resume test");
        let fail_step_2 = Arc::new(AtomicBool::new(true));
        let step_1_calls = Arc::new(AtomicUsize::new(0));
        let step_2_calls = Arc::new(AtomicUsize::new(0));

        let fail_ref = Arc::clone(&fail_step_2);
        let s1_ref = Arc::clone(&step_1_calls);
        let s2_ref = Arc::clone(&step_2_calls);

        let server = MockOpenAiServer::start(Arc::new(move |_idx, body| {
            let fail_step_2 = Arc::clone(&fail_ref);
            let step_1_calls = Arc::clone(&s1_ref);
            let step_2_calls = Arc::clone(&s2_ref);
            Box::pin(async move {
                let model = req_model(&body).to_string();
                if model == "e2e-wf-runner"
                    && messages_with_tool_or_user_containing(&body, "E2E_STEP_2_FINALIZE_7B21")
                {
                    step_2_calls.fetch_add(1, Ordering::SeqCst);
                    if fail_step_2.load(Ordering::SeqCst) {
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            openai_error_response("simulated_crash", "simulated step 2 crash"),
                        );
                    }
                    return (
                        StatusCode::OK,
                        openai_text_response(&model, "STEP_2_FINALIZE_OK"),
                    );
                }
                if model == "e2e-wf-runner"
                    && messages_with_tool_or_user_containing(&body, "E2E_STEP_1_COLLECT_4A19")
                {
                    step_1_calls.fetch_add(1, Ordering::SeqCst);
                    return (
                        StatusCode::OK,
                        openai_text_response(&model, "STEP_1_BASELINE_OK"),
                    );
                }
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    openai_error_response("unmatched_route", "unexpected workflow request"),
                )
            })
        }))
        .await;

        // Real file-backed SQLite DB to verify persistence across full AppState drop/restart.
        let temp_dir = tempfile::tempdir().expect("create durable sqlite tempdir");
        let db_path = temp_dir.path().join("durable_e2e.sqlite");
        let db_url = format!("sqlite:{}?mode=rwc", db_path.display());

        let wf_content =
            "## Collect\nE2E_STEP_1_COLLECT_4A19 baseline.\n## Finalize\nE2E_STEP_2_FINALIZE_7B21 report.\n";
        let wf_id = "e2e-durable-crash-resume-wf".to_string();

        // Phase 1: Initial process state runs Step 1 (memoizes) then crashes on Step 2.
        let s1_after_crash;
        let s2_after_crash;
        {
            let pool1 = init_db(&db_url).await.expect("init phase 1 sqlite db");
            let state1 = setup_e2e_state_with_pool(pool1).await;
            register_e2e_agent(&state1, "wf-runner", "Specialist", &server.base_url, 2, false).await;

            let wf_dir1 = state1.base_dir.join("data/workflows");
            tokio::fs::create_dir_all(&wf_dir1).await.unwrap();
            tokio::fs::write(wf_dir1.join("e2e_resume_sop.md"), wf_content)
                .await
                .unwrap();
            {
                let mut agent = state1.registry.agents.get_mut("wf-runner").unwrap();
                agent.capabilities.workflows = vec!["e2e_resume_sop".to_string()];
                agent.version = save_agent_db(&state1.resources.pool, &agent)
                    .await
                    .expect("persist workflow assignment");
            }

            let runner1 = AgentRunner::new(Arc::clone(&state1));
            let first_err = runner1
                .run(
                    "wf-runner".to_string(),
                    TaskPayload {
                        message: "/workflow e2e_resume_sop".to_string(),
                        workflow_id: Some(wf_id.clone()),
                        budget_usd: Some(25.0),
                        ..Default::default()
                    },
                )
                .await
                .expect_err("Step 2 simulated 500 failure should fail first workflow run");
            assert!(!first_err.to_string().is_empty());

            s1_after_crash = step_1_calls.load(Ordering::SeqCst);
            s2_after_crash = step_2_calls.load(Ordering::SeqCst);
            assert!(
                s1_after_crash >= 1,
                "Step 1 must execute in Phase 1; first_err={}",
                first_err
            );
            assert!(
                s2_after_crash >= 1,
                "Step 2 must be attempted in Phase 1; first_err={}",
                first_err
            );

            state1.resources.pool.close().await;
        }

        // Phase 2: Fresh AppState & AgentRunner rehydrated strictly from SQLite on disk.
        fail_step_2.store(false, Ordering::SeqCst);
        let pool2 = init_db(&db_url).await.expect("reopen phase 2 sqlite db");

        let wf_status_after_crash: String =
            sqlx::query_scalar("SELECT status FROM durable_workflows WHERE workflow_id = ?")
                .bind(&wf_id)
                .fetch_one(&pool2)
                .await
                .expect("durable_workflows row persisted on disk after crash");
        assert_eq!(wf_status_after_crash, "FAILED");

        let step0_row: (String, Option<String>) = sqlx::query_as(
            "SELECT status, output_json FROM durable_steps WHERE workflow_id = ? AND step_index = 0",
        )
        .bind(&wf_id)
        .fetch_one(&pool2)
        .await
        .expect("Step 0 row persisted in durable_steps");
        assert_eq!(step0_row.0, "COMPLETED");
        assert!(
            step0_row
                .1
                .as_deref()
                .is_some_and(|out| out.contains("STEP_1_BASELINE_OK")),
            "Step 0 memoized output_json must contain STEP_1_BASELINE_OK"
        );

        let state2 = setup_e2e_state_with_pool(pool2).await;
        for agent in load_agents_db(&state2.resources.pool)
            .await
            .expect("reload agents from SQLite")
        {
            state2.registry.agents.insert(agent.identity.id.clone(), agent);
        }
        let wf_dir2 = state2.base_dir.join("data/workflows");
        tokio::fs::create_dir_all(&wf_dir2).await.unwrap();
        tokio::fs::write(wf_dir2.join("e2e_resume_sop.md"), wf_content)
            .await
            .unwrap();

        let runner2 = AgentRunner::new(Arc::clone(&state2));
        let resumed_out = runner2
            .run(
                "wf-runner".to_string(),
                TaskPayload {
                    message: "/workflow e2e_resume_sop".to_string(),
                    workflow_id: Some(wf_id.clone()),
                    budget_usd: Some(25.0),
                    ..Default::default()
                },
            )
            .await
            .expect("resumed workflow on fresh AppState should succeed cleanly");

        assert!(resumed_out.contains("STEP_1_BASELINE_OK"));
        assert!(resumed_out.contains("STEP_2_FINALIZE_OK"));
        assert_eq!(
            step_1_calls.load(Ordering::SeqCst),
            s1_after_crash,
            "Memoized Step 1 must replay from disk SQLite with zero extra LLM provider calls on resume"
        );
        assert!(
            step_2_calls.load(Ordering::SeqCst) > s2_after_crash,
            "Step 2 must re-execute on resume after prior failure"
        );

        let wf_status_final: String =
            sqlx::query_scalar("SELECT status FROM durable_workflows WHERE workflow_id = ?")
                .bind(&wf_id)
                .fetch_one(&state2.resources.pool)
                .await
                .unwrap();
        assert_eq!(wf_status_final, "COMPLETED");
        assert!(
            server.take_violations().is_empty(),
            "Unexpected server contract violations"
        );
    })
    .await
    .expect("e2e_durable_workflow_crash_and_resume timed out");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_cascade_router_tier2_escalation() {
    timeout(TEST_TIMEOUT, async {
        tracing::info!("[orchestration_e2e] Starting CascadeRouter Tier 1 -> Tier 2 escalation test");

        let tier1_server = MockOpenAiServer::start(Arc::new(|_idx, _body| {
            Box::pin(async move {
                (
                    StatusCode::BAD_REQUEST,
                    openai_error_response(
                        "capability_unsupported",
                        "schema error: function calling not supported",
                    ),
                )
            })
        }))
        .await;

        let tier2_server = MockOpenAiServer::start(Arc::new(|_idx, body| {
            Box::pin(async move {
                let model = req_model(&body).to_string();
                if model == "tier2-reasoning" {
                    return (
                        StatusCode::OK,
                        openai_text_response(&model, "CASCADE_TIER2_RECOVERED"),
                    );
                }
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    openai_error_response("unmatched_route", "unexpected tier2 model"),
                )
            })
        }))
        .await;

        let state = setup_e2e_state().await;
        register_e2e_agent(&state, "cascade-e2e", "Specialist", &tier1_server.base_url, 3, true)
            .await;
        {
            let mut agent = state.registry.agents.get_mut("cascade-e2e").unwrap();
            agent.models.model.model_id = "tier1-fast".to_string();
            agent.models.model_2 = Some("tier2-reasoning".to_string());
            agent.models.model_config2 = Some(ModelConfig {
                provider: ModelProvider::Openai,
                model_id: "tier2-reasoning".to_string(),
                api_key: Some("e2e-tier2-key".to_string()),
                base_url: Some(tier2_server.base_url.clone()),
                max_turns: Some(3),
                input_cost_per_m: Some(0.0),
                output_cost_per_m: Some(0.0),
                ..Default::default()
            });
        }

        let runner = AgentRunner::new(Arc::clone(&state));
        let cascade_out = runner
            .run(
                "cascade-e2e".to_string(),
                TaskPayload {
                    message: "Check repository status CASCADE_NONCE_3F90".to_string(),
                    cascade_routing: Some(true),
                    budget_usd: Some(25.0),
                    ..Default::default()
                },
            )
            .await
            .expect("opt-in cascade escalation should recover via Tier 2");
        assert_eq!(cascade_out, "CASCADE_TIER2_RECOVERED");
        assert!(tier1_server.calls.load(Ordering::SeqCst) >= 1);
        assert!(tier2_server.calls.load(Ordering::SeqCst) >= 1);
        assert!(tier1_server.take_violations().is_empty());
        assert!(tier2_server.take_violations().is_empty());
    })
    .await
    .expect("e2e_cascade_router_tier2_escalation timed out");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_observe_mode_blocks_blackboard_write_and_allows_read() {
    timeout(TEST_TIMEOUT, async {
        tracing::info!("[orchestration_e2e] Starting AcceptMode::Observe guard verification");

        let saw_observe_block = Arc::new(AtomicBool::new(false));
        let saw_read_ok = Arc::new(AtomicBool::new(false));
        let block_ref = Arc::clone(&saw_observe_block);
        let read_ref = Arc::clone(&saw_read_ok);

        // Route purely by transcript content (H3), zero call-index coupling.
        let observe_server = MockOpenAiServer::start(Arc::new(move |_idx, body| {
            let saw_observe_block = Arc::clone(&block_ref);
            let saw_read_ok = Arc::clone(&read_ref);
            Box::pin(async move {
                let model = req_model(&body).to_string();
                let full = flatten_messages(&body);

                let blocked = full.contains(
                    "Observe (Shadow) mode active: mutation tool 'blackboard_write' is blocked",
                );
                let read_done = full.contains("Blackboard key 'readonly-probe' not found in scope");
                if blocked {
                    saw_observe_block.store(true, Ordering::SeqCst);
                }
                if read_done {
                    saw_read_ok.store(true, Ordering::SeqCst);
                }

                if blocked && read_done {
                    return (
                        StatusCode::OK,
                        openai_text_response(&model, "OBSERVE_AUDIT_DONE"),
                    );
                }
                if blocked {
                    return (
                        StatusCode::OK,
                        openai_tool_response(
                            &model,
                            "blackboard_read",
                            serde_json::json!({ "key": "readonly-probe" }),
                        ),
                    );
                }
                (
                    StatusCode::OK,
                    openai_tool_response(
                        &model,
                        "blackboard_write",
                        serde_json::json!({ "key": "blocked-key", "value": "should-never-exist" }),
                    ),
                )
            })
        }))
        .await;

        let state = setup_e2e_state().await;
        register_e2e_agent(&state, "observe-e2e", "Specialist", &observe_server.base_url, 5, false)
            .await;

        // Pre-seed a mission partition to verify blackboard_read is unblocked and blocked-key is absent.
        let runner = AgentRunner::new(Arc::clone(&state));
        let observe_out = runner
            .run(
                "observe-e2e".to_string(),
                TaskPayload {
                    message: "Audit blackboard in observe mode OBSERVE_NONCE_88C2".to_string(),
                    accept_mode: Some(AcceptMode::Observe),
                    budget_usd: Some(25.0),
                    ..Default::default()
                },
            )
            .await
            .expect("observe run should finish cleanly after read-only probe");

        assert_eq!(observe_out, "OBSERVE_AUDIT_DONE");
        assert!(
            saw_observe_block.load(Ordering::SeqCst),
            "Observe mode must block blackboard_write"
        );
        assert!(
            saw_read_ok.load(Ordering::SeqCst),
            "Observe mode must allow blackboard_read"
        );
        // Verify SharedBlackboard itself never stored "blocked-key" during or after the run.
        assert!(
            state.comms.blackboard.is_empty(),
            "SharedBlackboard must have zero mutations from Observe-blocked blackboard_write; active: {:?}",
            state.comms.blackboard.active_mission_ids()
        );
        assert!(observe_server.take_violations().is_empty());
    })
    .await
    .expect("e2e_observe_mode_blocks_blackboard_write_and_allows_read timed out");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_cooperative_mission_handle_abort_cancels_child_wave() {
    timeout(TEST_TIMEOUT, async {
        tracing::info!("[orchestration_e2e] Starting cooperative MissionHandle abort test");

        // Permit-storing notify_one() prevents missed-wakeup races even before awaiting notified() (B4).
        let child_entered = Arc::new(Notify::new());
        let child_entered_ref = Arc::clone(&child_entered);

        let abort_server = MockOpenAiServer::start(Arc::new(move |_idx, body| {
            let child_entered = Arc::clone(&child_entered_ref);
            Box::pin(async move {
                let model = req_model(&body).to_string();
                let full = flatten_messages(&body);
                if messages_with_tool_or_user_containing(&body, "BLOCK_ON_CHILD_WAIT_A91E") {
                    child_entered.notify_one();
                    tokio::time::sleep(Duration::from_secs(30)).await;
                    return (
                        StatusCode::OK,
                        openai_text_response(&model, "SHOULD_BE_CANCELLED"),
                    );
                }
                if messages_with_tool_or_user_containing(&body, "SPAWN_SLOW_CHILD_E4D1")
                    && !full.contains("EXECUTION PROTOCOL")
                {
                    return (
                        StatusCode::OK,
                        openai_tool_response(
                            &model,
                            "spawn_subagent",
                            serde_json::json!({
                                "tasks": [{
                                    "id": "slow-child",
                                    "agent_id": "slow-alpha",
                                    "message": "BLOCK_ON_CHILD_WAIT_A91E"
                                }]
                            }),
                        ),
                    );
                }
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    openai_error_response("unmatched_route", "unexpected abort-test request"),
                )
            })
        }))
        .await;

        let state = setup_e2e_state().await;
        register_e2e_agent(&state, "abort-ceo", "CEO", &abort_server.base_url, 4, false).await;
        register_e2e_agent(&state, "slow-alpha", "Alpha", &abort_server.base_url, 2, false).await;

        let runner = AgentRunner::new(Arc::clone(&state));
        let run_handle = tokio::spawn(async move {
            runner
                .run(
                    "abort-ceo".to_string(),
                    TaskPayload {
                        message: "SPAWN_SLOW_CHILD_E4D1".to_string(),
                        budget_usd: Some(25.0),
                        ..Default::default()
                    },
                )
                .await
        });

        timeout(Duration::from_secs(5), child_entered.notified())
            .await
            .expect("slow child should reach provider before abort");

        let mut handles = Vec::new();
        for _ in 0..20 {
            handles = state
                .comms
                .mission_handles
                .iter()
                .map(|entry| entry.value().clone())
                .collect();
            if !handles.is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        assert!(
            !handles.is_empty(),
            "Active mission handle must be registered while child wave runs"
        );
        for h in handles {
            h.abort();
        }

        let aborted_result = timeout(Duration::from_secs(5), run_handle)
            .await
            .expect("aborted root mission must terminate promptly without waiting 30s")
            .expect("root task join must succeed");
        assert!(
            aborted_result.is_err(),
            "Aborted mission must return cancellation error; got {:?}",
            aborted_result
        );

        assert!(
            state.comms.mission_handles.is_empty(),
            "mission_handles must be cleaned up after cooperative abort"
        );
        assert!(
            state.comms.blackboard.is_empty(),
            "SharedBlackboard must have zero orphaned partitions after cooperative abort; active: {:?}",
            state.comms.blackboard.active_mission_ids()
        );
        assert!(abort_server.take_violations().is_empty());
    })
    .await
    .expect("e2e_cooperative_mission_handle_abort_cancels_child_wave timed out");
}

// Metadata: [orchestration_e2e]

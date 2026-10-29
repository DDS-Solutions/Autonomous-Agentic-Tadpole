//! @docs ARCHITECTURE:Core
//!
//! ### AI Assist Note
//! Local provider fixtures for background worker regression tests.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Loopback binding failure or a worker never reaching its provider.
//! - **Telemetry Link**: Test assertions identify the failing worker.

use crate::{
    agent::types::{EngineAgent, ModelConfig, ModelProvider},
    state::AppState,
};
use axum::{
    http::{HeaderMap, StatusCode},
    routing::post,
    Json, Router,
};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tokio::{
    sync::{Notify, Semaphore},
    task::JoinHandle,
};

pub(crate) struct ProviderStub {
    pub base_url: String,
    pub calls: Arc<AtomicUsize>,
    entered: Arc<Notify>,
    release: Arc<Semaphore>,
    server: JoinHandle<()>,
}

impl ProviderStub {
    pub async fn start(status: StatusCode, text: &str, blocked: bool) -> Self {
        Self::start_blocking_at(status, text, if blocked { 0 } else { usize::MAX }).await
    }

    pub async fn start_blocking_at(status: StatusCode, text: &str, block_at: usize) -> Self {
        let calls = Arc::new(AtomicUsize::new(0));
        let entered = Arc::new(Notify::new());
        let release = Arc::new(Semaphore::new(0));
        let handler_calls = calls.clone();
        let handler_entered = entered.clone();
        let handler_release = release.clone();
        let text = text.to_string();
        let app = Router::new().route("/v1/chat/completions", post(move |headers: HeaderMap| {
            let calls = handler_calls.clone();
            let entered = handler_entered.clone();
            let release = handler_release.clone();
            let text = text.clone();
            async move {
                assert_eq!(headers.get("user-agent").unwrap(), "TadpoleOS/1.1.59");
                let call_index = calls.fetch_add(1, Ordering::SeqCst);
                entered.notify_one();
                if call_index == block_at {
                    release.acquire().await.unwrap().forget();
                }
                (status, Json(serde_json::json!({
                    "choices": [{"message": {"content": text}}],
                    "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}
                })))
            }
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base_url = format!("http://{}/v1", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        Self {
            base_url,
            calls,
            entered,
            release,
            server,
        }
    }

    pub async fn wait_for_call(&self) {
        self.wait_for_calls(1).await;
    }

    pub async fn wait_for_calls(&self, count: usize) {
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            while self.calls.load(Ordering::SeqCst) < count {
                self.entered.notified().await;
            }
        })
        .await
        .expect("worker did not reach the local provider");
    }

    pub fn respond(&self) {
        self.release.add_permits(1);
    }
}

impl Drop for ProviderStub {
    fn drop(&mut self) {
        self.server.abort();
    }
}

pub(crate) async fn state_with_agent(agent_id: &str, base_url: &str) -> Arc<AppState> {
    let mut state = AppState::new_minimal_mock().await;
    Arc::get_mut(&mut state.security).unwrap().budget_guard = Arc::new(
        crate::security::metering::BudgetGuard::new(state.resources.pool.clone()),
    );
    Arc::get_mut(&mut state.resources).unwrap().http_client = Arc::new(
        reqwest::Client::builder()
            .no_proxy()
            .user_agent("TadpoleOS/1.1.59")
            .build()
            .unwrap(),
    );
    let mut agent = EngineAgent::default();
    agent.identity.id = agent_id.to_string();
    agent.models.model = ModelConfig {
        provider: ModelProvider::Openai,
        model_id: "worker-regression-fixture".to_string(),
        api_key: Some("local-test-key".to_string()),
        base_url: Some(base_url.to_string()),
        max_turns: Some(1),
        input_cost_per_m: Some(1.0),
        output_cost_per_m: Some(2.0),
        ..Default::default()
    };
    agent.version = crate::agent::persistence::create_agent_db(&state.resources.pool, &agent)
        .await
        .unwrap();
    state.registry.agents.insert(agent_id.to_string(), agent);
    Arc::new(state)
}

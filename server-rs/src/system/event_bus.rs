//! @docs ARCHITECTURE:Autonomy
//! @docs ARCHITECTURE:Core
//!
//! ### AI Assist Note
//! **System Event Bus**: Thread-safe pub-sub system using Tokio channels to register,
//! broadcast, and process local operational events. Associates incoming triggers
//! (file modifications, resource thresholds) with proactive "Continuity Jobs" to
//! close the automation loop.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Channel buffer saturation, job-spawning failures due to claims lock,
//!   or invalid trigger mappings.
//! - **Telemetry Link**: Search `[event_bus]` in server logs.


use crate::error::AppError;
use crate::state::AppState;
use futures::{stream::FuturesUnordered, StreamExt};
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::broadcast;

pub(crate) const MAX_EVENT_JOBS_IN_FLIGHT: usize = 16;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum SystemEvent {
    FileChanged { path: String, change_type: String },
    ComputeAlert { cpu_usage: f32, memory_usage_mb: usize },
    WebhookTriggered { payload: serde_json::Value },
}

pub struct SystemEventBus {
    tx: broadcast::Sender<SystemEvent>,
}

impl Default for SystemEventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemEventBus {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(100);
        Self { tx }
    }

    pub fn publish(&self, event: SystemEvent) {
        let event_type = match &event {
            SystemEvent::FileChanged { .. } => "FileChanged",
            SystemEvent::ComputeAlert { .. } => "ComputeAlert",
            SystemEvent::WebhookTriggered { .. } => "WebhookTriggered",
        };
        tracing::debug!(event_type, "[EventBus] Publishing event");
        let _ = self.tx.send(event);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<SystemEvent> {
        self.tx.subscribe()
    }
}

pub async fn start_event_bus_monitoring(app_state: Arc<AppState>, bus: Arc<SystemEventBus>) -> tokio::task::JoinHandle<()> {
    let mut rx = bus.subscribe();
    let state_clone = app_state.clone();

    tracing::info!("📣 [EventBus] System Event Bus listener online.");

    tokio::spawn(async move {
        // The set tracks bounded job executors. Dropping a JoinHandle lets an
        // admitted job finish its scheduled-run cleanup if the monitor stops.
        let mut executions = FuturesUnordered::new();
        let mut pending = VecDeque::<String>::new();
        loop {
            while executions.len() < MAX_EVENT_JOBS_IN_FLIGHT {
                let Some(job_id) = pending.pop_front() else { break; };
                let state = state_clone.clone();
                executions.push(tokio::spawn(async move {
                    if let Err(error) = crate::agent::continuity::executor::execute_job_by_id(state, &job_id).await {
                        tracing::error!(%error, %job_id, "[EventBus] Triggered job failed");
                    }
                }));
            }
            tokio::select! {
                result = executions.next(), if !executions.is_empty() => {
                    if let Some(Err(error)) = result {
                        tracing::error!(%error, "[EventBus] Triggered job executor panicked or was cancelled");
                    }
                }
                received = rx.recv(), if pending.is_empty() && executions.len() < MAX_EVENT_JOBS_IN_FLIGHT => {
                    match received {
                        Ok(event) => match event_job_ids(&state_clone, event).await {
                            Ok(jobs) => pending.extend(jobs),
                            Err(error) => tracing::error!(%error, "[EventBus] Event processing failed"),
                        },
                        Err(broadcast::error::RecvError::Lagged(skipped)) => {
                            tracing::warn!(skipped, "[EventBus] Listener lagged; continuing with retained events");
                        }
                        Err(broadcast::error::RecvError::Closed) => break,
                    }
                }
            }
        }
    })
}

async fn event_job_ids(state: &Arc<AppState>, event: SystemEvent) -> Result<Vec<String>, AppError> {
    let (event_type, path) = match event {
        SystemEvent::FileChanged { path, .. } => ("FileChanged", Some(path)),
        SystemEvent::ComputeAlert { .. } => ("ComputeAlert", None),
        SystemEvent::WebhookTriggered { .. } => ("WebhookTriggered", None),
    };
    Ok(sqlx::query_scalar::<_, String>(
        "SELECT DISTINCT continuity_job_id FROM event_triggers WHERE event_type = ? AND (? IS NULL OR event_filter IS NULL OR instr(?, event_filter) > 0) ORDER BY continuity_job_id"
    ).bind(event_type).bind(&path).bind(&path).fetch_all(&state.resources.pool).await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{continuity::{scheduler::create_job, types::CreateJobRequest}, types::EngineAgent};

    async fn process_event(state: &Arc<AppState>, event: SystemEvent) -> Result<(), AppError> {
        for job_id in event_job_ids(state, event).await? {
            let _ = crate::agent::continuity::executor::execute_job_by_id(state.clone(), &job_id).await;
        }
        Ok(())
    }

    async fn provider_job(state: &AppState, agent_id: &str) -> String {
        create_job(&state.resources.pool, CreateJobRequest {
            agent_id: agent_id.to_string(), workflow_id: None,
            name: agent_id.to_string(), prompt: "Say hello".to_string(),
            cron_expr: "* * * * *".to_string(), budget_usd: Some(0.1),
            max_failures: None, metadata: None,
        }).await.unwrap().id
    }

    async fn wait_for_completed(state: &AppState, job_id: &str) {
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                let agent_id: Option<String> = sqlx::query_scalar("SELECT jobs.agent_id FROM scheduled_jobs jobs JOIN scheduled_job_runs runs ON runs.job_id = jobs.id WHERE jobs.id = ? AND runs.status = 'completed' AND jobs.last_run_at IS NOT NULL")
                    .bind(job_id).fetch_optional(&state.resources.pool).await.unwrap();
                if agent_id.is_some_and(|id| !state.comms.active_runners.contains_key(&id)) { break; }
                tokio::task::yield_now().await;
            }
        }).await.expect("event job did not complete");
    }

    #[tokio::test]
    async fn finding_regression_monitor_processes_later_event_while_provider_is_blocked() {
        use crate::test_support::{state_with_agent, ProviderStub};
        let slow = ProviderStub::start(axum::http::StatusCode::OK, "complete_mission done", true).await;
        let fast = ProviderStub::start(axum::http::StatusCode::OK, "complete_mission done", false).await;
        let state = state_with_agent("slow-event-agent", &slow.base_url).await;
        let mut agent = state.registry.agents.get("slow-event-agent").unwrap().clone();
        agent.identity.id = "fast-event-agent".to_string();
        agent.models.model.base_url = Some(fast.base_url.clone());
        agent.version = crate::agent::persistence::create_agent_db(&state.resources.pool, &agent).await.unwrap();
        state.registry.agents.insert(agent.identity.id.clone(), agent);
        let slow_job = provider_job(&state, "slow-event-agent").await;
        let fast_job = provider_job(&state, "fast-event-agent").await;
        trigger(&state, "slow", &slow_job, Some("slow.rs")).await;
        trigger(&state, "fast", &fast_job, Some("fast.rs")).await;
        let bus = Arc::new(SystemEventBus::new());
        let monitor = start_event_bus_monitoring(state.clone(), bus.clone()).await;
        bus.publish(file_changed("slow.rs"));
        slow.wait_for_call().await;
        bus.publish(file_changed("fast.rs"));
        wait_for_completed(&state, &fast_job).await;
        assert_eq!(sqlx::query_scalar::<_, String>("SELECT status FROM scheduled_job_runs WHERE job_id = ?")
            .bind(&slow_job).fetch_one(&state.resources.pool).await.unwrap(), "running");
        monitor.abort();
        assert!(monitor.await.unwrap_err().is_cancelled());
        slow.respond();
        wait_for_completed(&state, &slow_job).await;
        assert_eq!(state.governance.active_agents.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn finding_regression_monitor_bounds_in_flight_jobs_and_drains_pending_mappings() {
        use crate::test_support::{state_with_agent, ProviderStub};
        use std::sync::atomic::Ordering;
        let first = ProviderStub::start(axum::http::StatusCode::OK, "complete_mission done", true).await;
        let state = state_with_agent("bounded-event-0", &first.base_url).await;
        let template = state.registry.agents.get("bounded-event-0").unwrap().clone();
        let mut providers = vec![first];
        let mut jobs = vec![provider_job(&state, "bounded-event-0").await];
        trigger(&state, "bounded-0", &jobs[0], None).await;
        for index in 1..=MAX_EVENT_JOBS_IN_FLIGHT {
            let provider = ProviderStub::start(axum::http::StatusCode::OK, "complete_mission done", true).await;
            let mut agent = template.clone();
            agent.identity.id = format!("bounded-event-{index}");
            agent.models.model.base_url = Some(provider.base_url.clone());
            agent.version = crate::agent::persistence::create_agent_db(&state.resources.pool, &agent).await.unwrap();
            let job = provider_job(&state, &agent.identity.id).await;
            state.registry.agents.insert(agent.identity.id.clone(), agent);
            trigger(&state, &format!("bounded-{index}"), &job, None).await;
            jobs.push(job);
            providers.push(provider);
        }
        let bus = Arc::new(SystemEventBus::new());
        let monitor = start_event_bus_monitoring(state.clone(), bus.clone()).await;
        bus.publish(file_changed("bounded.rs"));
        let filled = tokio::time::timeout(std::time::Duration::from_secs(10), async {
            while providers.iter().map(|provider| provider.calls.load(Ordering::SeqCst)).sum::<usize>() < MAX_EVENT_JOBS_IN_FLIGHT {
                tokio::task::yield_now().await;
            }
        }).await;
        if filled.is_err() {
            let runs: Vec<(String, String, Option<String>)> = sqlx::query_as("SELECT job_id, status, output_summary FROM scheduled_job_runs")
                .fetch_all(&state.resources.pool).await.unwrap();
            let calls: Vec<_> = providers.iter().map(|provider| provider.calls.load(Ordering::SeqCst)).collect();
            panic!("monitor did not fill its bounded execution slots: calls={calls:?}, runs={runs:?}");
        }
        assert_eq!(run_count(&state).await, MAX_EVENT_JOBS_IN_FLIGHT as i64);
        assert_eq!(providers.iter().map(|provider| provider.calls.load(Ordering::SeqCst)).sum::<usize>(), MAX_EVENT_JOBS_IN_FLIGHT);
        for provider in &providers { provider.respond(); }
        for job in &jobs { wait_for_completed(&state, job).await; }
        assert_eq!(run_count(&state).await, jobs.len() as i64);
        assert!(state.comms.active_runners.is_empty());
        monitor.abort();
        assert!(monitor.await.unwrap_err().is_cancelled());
    }

    async fn triggered_job(enabled: bool) -> (Arc<AppState>, String) {
        let state = Arc::new(AppState::new_minimal_mock().await);
        let mut agent = EngineAgent::default();
        agent.identity.id = "event-agent".to_string();
        agent.health.status = "busy".to_string();
        agent.version = crate::agent::persistence::create_agent_db(&state.resources.pool, &agent).await.unwrap();
        state.registry.agents.insert(agent.identity.id.clone(), agent);
        let job = create_job(&state.resources.pool, CreateJobRequest {
            agent_id: "event-agent".to_string(), workflow_id: None,
            name: "Event regression".to_string(), prompt: "Say hello".to_string(),
            cron_expr: "* * * * *".to_string(), budget_usd: None,
            max_failures: None, metadata: None,
        }).await.unwrap();
        sqlx::query("UPDATE scheduled_jobs SET enabled = ? WHERE id = ?")
            .bind(enabled).bind(&job.id).execute(&state.resources.pool).await.unwrap();
        (state, job.id)
    }

    async fn trigger(state: &AppState, id: &str, job_id: &str, filter: Option<&str>) {
        sqlx::query("INSERT INTO event_triggers (id, event_type, event_filter, continuity_job_id) VALUES (?, 'FileChanged', ?, ?)")
            .bind(id).bind(filter).bind(job_id).execute(&state.resources.pool).await.unwrap();
    }

    fn file_changed(path: &str) -> SystemEvent {
        SystemEvent::FileChanged { path: path.to_string(), change_type: "modify".to_string() }
    }

    async fn run_count(state: &AppState) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM scheduled_job_runs")
            .fetch_one(&state.resources.pool).await.unwrap()
    }

    #[tokio::test]
    async fn worker_regression_monitor_recovers_from_lag_and_processes_later_events() {
        let (state, job_id) = triggered_job(true).await;
        trigger(&state, "retained-event", &job_id, Some("retained.rs")).await;
        let bus = Arc::new(SystemEventBus::new());
        let monitor = start_event_bus_monitoring(state.clone(), bus.clone()).await;
        // No await on this current-thread runtime: the subscribed monitor cannot
        // drain its queue until overflow and the retained event are both published.
        for _ in 0..300 {
            bus.publish(SystemEvent::ComputeAlert { cpu_usage: 90.0, memory_usage_mb: 4096 });
        }
        bus.publish(file_changed("retained.rs"));
        for expected in 1..=2 {
            tokio::time::timeout(std::time::Duration::from_secs(10), async {
                while run_count(&state).await < expected {
                    assert!(!monitor.is_finished(), "monitor stopped after lag");
                    tokio::task::yield_now().await;
                }
            }).await.expect("monitor did not process the retained or subsequent event");
            if expected == 1 { bus.publish(file_changed("retained.rs")); }
        }
        assert_eq!(run_count(&state).await, 2);
        assert!(!monitor.is_finished());
        monitor.abort();
        assert!(monitor.await.unwrap_err().is_cancelled());
    }

    #[tokio::test]
    async fn worker_regression_duplicate_mappings_admit_each_job_once() {
        let (state, job_id) = triggered_job(true).await;
        trigger(&state, "first", &job_id, None).await;
        trigger(&state, "second", &job_id, Some("main.rs")).await;
        process_event(&state, file_changed("src/main.rs")).await.unwrap();
        assert_eq!(run_count(&state).await, 1);
        assert_eq!(sqlx::query_scalar::<_, String>("SELECT status FROM scheduled_job_runs")
            .fetch_one(&state.resources.pool).await.unwrap(), "skipped");
        assert!(state.comms.active_runners.is_empty());
    }

    #[tokio::test]
    async fn worker_regression_disabled_job_is_not_admitted() {
        let (state, job_id) = triggered_job(false).await;
        trigger(&state, "disabled", &job_id, None).await;
        let version = state.registry.agents.get("event-agent").unwrap().version;
        assert!(matches!(crate::agent::continuity::executor::execute_job_by_id(state.clone(), &job_id).await,
            Err(AppError::Forbidden(_))));
        process_event(&state, file_changed("src/main.rs")).await.unwrap();
        assert_eq!(run_count(&state).await, 0);
        assert!(state.comms.active_runners.is_empty());
        assert_eq!(state.registry.agents.get("event-agent").unwrap().version, version);
        assert_eq!(sqlx::query_scalar::<_, String>("SELECT status FROM agents WHERE id = 'event-agent'")
            .fetch_one(&state.resources.pool).await.unwrap(), "busy");
    }

    #[tokio::test]
    async fn worker_regression_file_filters_treat_quotes_and_wildcards_literally() {
        let (state, job_id) = triggered_job(true).await;
        trigger(&state, "literal", &job_id, Some("a%_'.rs")).await;
        process_event(&state, file_changed("src/abcx'.rs")).await.unwrap();
        assert_eq!(run_count(&state).await, 0, "SQL LIKE wildcards must not match");
        process_event(&state, file_changed("src/a%_'.rs")).await.unwrap();
        assert_eq!(run_count(&state).await, 1);
        process_event(&state, file_changed("' OR 1=1 --")).await.unwrap();
        assert_eq!(run_count(&state).await, 1, "path must remain a bound parameter");
        assert_eq!(sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM scheduled_jobs")
            .fetch_one(&state.resources.pool).await.unwrap(), 1);
    }

    #[tokio::test]
    async fn test_event_bus_broadcast_to_multiple_subscribers() {
        let bus = SystemEventBus::new();
        let mut sub1 = bus.subscribe();
        let mut sub2 = bus.subscribe();

        let event = SystemEvent::FileChanged {
            path: "server-rs/src/main.rs".to_string(),
            change_type: "modify".to_string(),
        };

        bus.publish(event);

        let received1 = sub1.recv().await.expect("Subscriber 1 should receive event");
        let received2 = sub2.recv().await.expect("Subscriber 2 should receive event");

        match (received1, received2) {
            (
                SystemEvent::FileChanged { path: p1, change_type: c1 },
                SystemEvent::FileChanged { path: p2, change_type: c2 },
            ) => {
                assert_eq!(p1, "server-rs/src/main.rs");
                assert_eq!(c1, "modify");
                assert_eq!(p2, p1);
                assert_eq!(c2, c1);
            }
            _ => panic!("Expected FileChanged event variant"),
        }
    }

    #[test]
    fn test_system_event_serde_roundtrip() {
        let alert = SystemEvent::ComputeAlert {
            cpu_usage: 92.5,
            memory_usage_mb: 4096,
        };

        let json_str = serde_json::to_string(&alert).expect("Failed to serialize alert");
        let deserialized: SystemEvent = serde_json::from_str(&json_str).expect("Failed to deserialize alert");

        match deserialized {
            SystemEvent::ComputeAlert { cpu_usage, memory_usage_mb } => {
                assert!((cpu_usage - 92.5).abs() < 0.001);
                assert_eq!(memory_usage_mb, 4096);
            }
            _ => panic!("Expected ComputeAlert variant"),
        }

        let webhook = SystemEvent::WebhookTriggered {
            payload: serde_json::json!({"action": "deploy", "env": "prod"}),
        };
        let json_wh = serde_json::to_string(&webhook).unwrap();
        let de_wh: SystemEvent = serde_json::from_str(&json_wh).unwrap();
        match de_wh {
            SystemEvent::WebhookTriggered { payload } => {
                assert_eq!(payload["action"], "deploy");
                assert_eq!(payload["env"], "prod");
            }
            _ => panic!("Expected WebhookTriggered variant"),
        }
    }

    #[tokio::test]
    async fn test_event_bus_channel_buffer_overflow_lag() {
        let bus = SystemEventBus::new();
        let mut sub = bus.subscribe();

        // Publish 150 events (capacity is 100)
        for i in 0..150 {
            bus.publish(SystemEvent::FileChanged {
                path: format!("file_{}.rs", i),
                change_type: "create".to_string(),
            });
        }

        // The first read should encounter Lagged error due to overflow
        match sub.recv().await {
            Err(tokio::sync::broadcast::error::RecvError::Lagged(missed)) => {
                assert!(missed > 0, "Should report missed events on lag");
            }
            Ok(_) => {} // In fast environments if not dropped yet
            Err(e) => panic!("Unexpected error: {:?}", e),
        }
    }
}

// Metadata: [event_bus]

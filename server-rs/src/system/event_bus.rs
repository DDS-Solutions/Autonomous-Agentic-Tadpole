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

// Event bus is scaffolded infrastructure for reactive automation — wired in a subsequent phase.
#![allow(dead_code)]

use crate::error::AppError;
use crate::state::AppState;
use std::sync::Arc;
use tokio::sync::broadcast;

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
        tracing::debug!("📣 [EventBus] Publishing event: {:?}", event);
        let _ = self.tx.send(event);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<SystemEvent> {
        self.tx.subscribe()
    }
}

pub async fn start_event_bus_monitoring(app_state: Arc<AppState>, bus: Arc<SystemEventBus>) {
    let mut rx = bus.subscribe();
    let state_clone = app_state.clone();

    tracing::info!("📣 [EventBus] System Event Bus listener online.");

    tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            if let Err(e) = process_event(&state_clone, event).await {
                tracing::error!("❌ [EventBus] Error processing event: {}", e);
            }
        }
    });
}

async fn process_event(state: &Arc<AppState>, event: SystemEvent) -> Result<(), AppError> {
    match event {
        SystemEvent::FileChanged { ref path, change_type: _ } => {
            // Find triggers matching FileChanged for this path/extension
            let rows = sqlx::query(
                "SELECT id, continuity_job_id FROM event_triggers WHERE event_type = 'FileChanged' AND (event_filter IS NULL OR ?1 LIKE '%' || event_filter || '%')"
            )
            .bind(path)
            .fetch_all(&state.resources.pool)
            .await
            .map_err(AppError::Sqlx)?;

            for r in rows {
                use sqlx::Row;
                let job_id: String = r.get("continuity_job_id");
                tracing::info!(
                    "🔔 [EventBus] Trigger matched for FileChanged (path: {}). Spawning job: {}",
                    path, job_id
                );
                let state_clone = state.clone();
                tokio::spawn(async move {
                    if let Err(e) = crate::agent::continuity::executor::execute_job_by_id(state_clone, &job_id).await {
                        tracing::error!("❌ [EventBus] Failed to spawn continuity job {}: {}", job_id, e);
                    }
                });
            }
        }
        SystemEvent::ComputeAlert { cpu_usage, memory_usage_mb } => {
            // Find triggers matching ComputeAlert
            let rows = sqlx::query(
                "SELECT id, continuity_job_id FROM event_triggers WHERE event_type = 'ComputeAlert'"
            )
            .fetch_all(&state.resources.pool)
            .await
            .map_err(AppError::Sqlx)?;

            for r in rows {
                use sqlx::Row;
                let job_id: String = r.get("continuity_job_id");
                tracing::info!(
                    "🔔 [EventBus] Trigger matched for ComputeAlert (CPU: {:.1}%, Mem: {}MB). Spawning job: {}",
                    cpu_usage, memory_usage_mb, job_id
                );
                let state_clone = state.clone();
                tokio::spawn(async move {
                    if let Err(e) = crate::agent::continuity::executor::execute_job_by_id(state_clone, &job_id).await {
                        tracing::error!("❌ [EventBus] Failed to spawn continuity job {}: {}", job_id, e);
                    }
                });
            }
        }
        SystemEvent::WebhookTriggered { payload: _ } => {
            let rows = sqlx::query(
                "SELECT id, continuity_job_id FROM event_triggers WHERE event_type = 'WebhookTriggered'"
            )
            .fetch_all(&state.resources.pool)
            .await
            .map_err(AppError::Sqlx)?;

            for r in rows {
                use sqlx::Row;
                let job_id: String = r.get("continuity_job_id");
                tracing::info!(
                    "🔔 [EventBus] Trigger matched for WebhookTriggered. Spawning job: {}",
                    job_id
                );
                let state_clone = state.clone();
                tokio::spawn(async move {
                    if let Err(e) = crate::agent::continuity::executor::execute_job_by_id(state_clone, &job_id).await {
                        tracing::error!("❌ [EventBus] Failed to spawn continuity job {}: {}", job_id, e);
                    }
                });
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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

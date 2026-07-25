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



// Metadata: [event_bus]

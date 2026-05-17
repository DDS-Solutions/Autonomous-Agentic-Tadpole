//! @docs ARCHITECTURE:TelemetryBridge
//! 
//! ### AI Assist Note
//! **Telemetry Bridge (Persistence Layer)**: Orchestrates the automated
//! persistence of real-time telemetry events into the SQLite audit ledger.
//! Ensures that all mission-critical spans (thoughts, tool calls, results)
//! are recorded for the QA-99 auditor to synthesize mission reports.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Database write contention during high-frequency swarm
//!   activity, malformed event schemas, or broadcast channel lag.
//! - **Trace Scope**: `server-rs::telemetry::bridge`

use crate::state::AppState;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::broadcast;

pub struct TelemetryBridge {
    app_state: Arc<AppState>,
}

impl TelemetryBridge {
    pub fn new(app_state: Arc<AppState>) -> Self {
        Self { app_state }
    }

    pub async fn run(self, mut rx_telemetry: broadcast::Receiver<Value>, mut rx_logs: broadcast::Receiver<crate::types::LogEntry>) {
        tracing::info!("📡 [Telemetry] Bridge started. Monitoring multi-channel mission event streams.");

        loop {
            tokio::select! {
                res = rx_telemetry.recv() => {
                    match res {
                        Ok(msg) => {
                            if let Err(e) = self.handle_telemetry_event(msg).await {
                                tracing::error!("❌ [Telemetry] Bridge failed to persist telemetry: {}", e);
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!("⚠️ [Telemetry] Bridge lagged by {} telemetry messages.", n);
                        }
                        Err(broadcast::error::RecvError::Closed) => break,
                    }
                }
                res = rx_logs.recv() => {
                    match res {
                        Ok(log) => {
                            if let Err(e) = self.handle_log_entry(log).await {
                                tracing::error!("❌ [Telemetry] Bridge failed to persist log: {}", e);
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!("⚠️ [Telemetry] Bridge lagged by {} log messages.", n);
                        }
                        Err(broadcast::error::RecvError::Closed) => break,
                    }
                }
            }
        }
    }

    async fn handle_log_entry(&self, log: crate::types::LogEntry) -> Result<(), crate::error::AppError> {
        if let Some(mid) = log.mission_id {
            let agent_id = log.agent_id.as_deref().unwrap_or("System");
            
            crate::agent::mission::log_step(
                &self.app_state.resources.pool,
                &mid,
                agent_id,
                &log.source,
                &log.text,
                &log.severity,
                None,
            ).await?;
        }
        Ok(())
    }

    async fn handle_telemetry_event(&self, msg: Value) -> Result<(), crate::error::AppError> {
        let event_type = msg["type"].as_str().unwrap_or_default();

        if event_type == "trace:span" {
            if let Some(span) = msg.get("span") {
                let mission_id = span["mission_id"].as_str();
                let agent_id = span["agent_id"].as_str().unwrap_or("System");
                let name = span["name"].as_str().unwrap_or("Unknown");
                
                if let Some(mid) = mission_id {
                    // SEC: Expanded whitelist of mission-critical spans to ensure QA-99 observability parity.
                    let is_critical = matches!(name, 
                        "think" | "execute_tool" | "Agent" | 
                        "AgentExecution" | "ToolOrchestration" | "McpToolCall" | 
                        "IntelligenceLoop" | "Refinement"
                    );

                    if is_critical {
                        let text = span["attributes"]["text"].as_str()
                            .or_else(|| span["attributes"]["thought"].as_str())
                            .or_else(|| span["attributes"]["message"].as_str())
                            .unwrap_or(name);
                        
                        crate::agent::mission::log_step(
                            &self.app_state.resources.pool,
                            mid,
                            agent_id,
                            "Bridge:Span",
                            text,
                            "info",
                            Some(span["attributes"].clone()),
                        ).await?;
                    }
                }
            }
        }

        Ok(())
    }
}

// Metadata: [bridge]

// Metadata: [bridge]

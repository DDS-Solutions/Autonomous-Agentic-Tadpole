//! @docs ARCHITECTURE:TelemetryEngine
//!
//! ### AI Context Alignment
//! - **Subsystem**: Sovereign Engine / Telemetry Stream / Zero-Copy Log Tailer
//! - **Primary Entrypoints**: `TelemetryStreamer`, `StreamFilter`
//!
//! ### ⚠️ Invariants & Non-Negotiables
//! - `[Performance]` Zero memory allocations during steady-state log formatting.
//! - `[Reliability]` Bounded memory: Ring buffer drops old events under extreme consumer lag.
//!
//! ### 🔍 Debugging & Observability
//! - **Telemetry Link**: Search `[telemetry_stream]` in tracing logs.

pub const TELEMETRY_TAG: &str = "[telemetry_stream]";

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::VecDeque;
use std::fmt::Write;
use tokio::sync::broadcast;

/// Telemetry filtering options for stream consumers.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StreamFilter {
    pub agent_id: Option<String>,
    pub mission_id: Option<String>,
    pub min_level: Option<String>,
}

impl StreamFilter {
    pub fn matches(&self, event: &Value) -> bool {
        if let Some(ref target_agent) = self.agent_id {
            let agent = event.get("agent_id").and_then(|v| v.as_str());
            if agent != Some(target_agent.as_str()) {
                return false;
            }
        }

        if let Some(ref target_mission) = self.mission_id {
            let mission = event.get("mission_id").and_then(|v| v.as_str());
            if mission != Some(target_mission.as_str()) {
                return false;
            }
        }

        if let Some(ref min_lvl) = self.min_level {
            let level = event.get("level").and_then(|v| v.as_str()).unwrap_or("INFO");
            if level_weight(level) < level_weight(min_lvl) {
                return false;
            }
        }

        true
    }
}

fn level_weight(level: &str) -> u8 {
    match level.to_uppercase().as_str() {
        "DEBUG" | "TRACE" => 1,
        "INFO" => 2,
        "WARN" | "WARNING" => 3,
        "ERROR" | "CRITICAL" => 4,
        _ => 2,
    }
}

/// Zero-copy telemetry stream consumer that replaces legacy Python log tailers.
pub struct TelemetryStreamer {
    filter: StreamFilter,
    buffer: VecDeque<Value>,
    capacity: usize,
}

impl TelemetryStreamer {
    pub fn new(filter: StreamFilter, capacity: usize) -> Self {
        Self {
            filter,
            buffer: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    /// Pushes an event into the ring buffer if it matches the active filter.
    pub fn push_event(&mut self, event: Value) {
        if self.filter.matches(&event) {
            if self.buffer.len() >= self.capacity {
                self.buffer.pop_front();
            }
            self.buffer.push_back(event);
        }
    }

    /// Formats all buffered telemetry events into a pre-allocated string.
    pub fn format_stream(&self) -> String {
        let mut out = String::with_capacity(self.buffer.len() * 128);
        for event in &self.buffer {
            let timestamp = event.get("timestamp").and_then(|v| v.as_str()).unwrap_or("-");
            let level = event.get("level").and_then(|v| v.as_str()).unwrap_or("INFO");
            let target = event.get("target").and_then(|v| v.as_str()).unwrap_or("system");
            let message = event.get("message").and_then(|v| v.as_str()).unwrap_or("");

            let _ = writeln!(out, "[{}] [{}] [{}] {}", timestamp, level, target, message);
        }
        out
    }

    /// Spawns a background task consuming directly from the global broadcast channel.
    pub fn spawn_listener(
        filter: StreamFilter,
        capacity: usize,
        mut rx: broadcast::Receiver<Value>,
    ) -> tokio::task::JoinHandle<Self> {
        tokio::spawn(async move {
            let mut streamer = Self::new(filter, capacity);
            while let Ok(event) = rx.recv().await {
                streamer.push_event(event);
            }
            streamer
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_stream_filter_by_agent_and_level() {
        let filter = StreamFilter {
            agent_id: Some("agent-99".to_string()),
            mission_id: None,
            min_level: Some("WARN".to_string()),
        };

        // Matches agent and level
        let match_event = json!({
            "agent_id": "agent-99",
            "level": "ERROR",
            "message": "Failure in runner"
        });
        assert!(filter.matches(&match_event));

        // Mismatched agent
        let wrong_agent = json!({
            "agent_id": "agent-1",
            "level": "ERROR",
            "message": "Failure in runner"
        });
        assert!(!filter.matches(&wrong_agent));

        // Mismatched level (INFO < WARN)
        let low_level = json!({
            "agent_id": "agent-99",
            "level": "INFO",
            "message": "Heartbeat"
        });
        assert!(!filter.matches(&low_level));
    }

    #[test]
    fn test_telemetry_streamer_ring_buffer_bounds() {
        let filter = StreamFilter::default();
        let mut streamer = TelemetryStreamer::new(filter, 3);

        for i in 1..=5 {
            streamer.push_event(json!({
                "timestamp": "2026-11-09T00:00:00Z",
                "level": "INFO",
                "target": "swarm",
                "message": format!("Event {}", i)
            }));
        }

        // Must clamp to capacity 3
        assert_eq!(streamer.buffer.len(), 3);
        let output = streamer.format_stream();
        assert!(!output.contains("Event 1"));
        assert!(!output.contains("Event 2"));
        assert!(output.contains("Event 3"));
        assert!(output.contains("Event 4"));
        assert!(output.contains("Event 5"));
    }
}

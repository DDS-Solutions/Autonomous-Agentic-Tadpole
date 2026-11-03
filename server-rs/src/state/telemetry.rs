//! @docs ARCHITECTURE:State
//!
//! ### AI Assist Note
//! **State Telemetry Subsystem**: Publishes system and agent events, in-flight secret redactions,
//! and cooperative scheduler yields across WebSockets, OTel pipelines, and real-time streams.
//!
//! ### 🔍 Debugging & Observability
//! - **Telemetry Link**: Search `[telemetry]` in tracing logs.
//! - **Trace Scope**: `server-rs::state::telemetry`

use crate::state::AppState;

impl AppState {
    /// ### 📡 Observability: System Broadcast (broadcast_sys)
    /// Publishes a high-priority system event to all connected telemetry 
    /// consumers (WebSockets, OTel exporters).
    /// 
    /// ### 🛡️ Neural Shield: Secret Redaction
    /// Automatically performs in-flight redaction of the log message using 
    /// industry-standard regex patterns to prevent accidental leakage of 
    /// API keys, tokens, or PII.
    pub fn broadcast_sys(&self, text: &str, severity: &str, mission_id: Option<String>) {
        let safe_text = self.security.secret_redactor.redact(text);
        let entry = crate::types::LogEntry::new("System", &safe_text, severity, mission_id);
        let _ = self.comms.tx.send(entry);
    }

    /// Helper to broadcast an agent-sourced log with identity metadata.
    pub fn broadcast_agent(
        &self,
        text: &str,
        severity: &str,
        mission_id: Option<String>,
        agent_id: &str,
        agent_name: &str,
    ) {
        let safe_text = self.security.secret_redactor.redact(text);
        let mut entry = crate::types::LogEntry::new("Agent", &safe_text, severity, mission_id);
        entry.agent_id = Some(agent_id.to_string());
        entry.agent_name = Some(agent_name.to_string());
        let _ = self.comms.tx.send(entry);
    }

    /// Helper to broadcast an arbitrary Engine event.
    pub fn emit_event(&self, event: serde_json::Value) {
        let mut full_event = event;
        let seq = self
            .comms
            .event_sequence
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        if let Some(obj) = full_event.as_object_mut() {
            obj.insert("_seq".to_string(), serde_json::json!(seq));
        }

        let _ = self.comms.event_tx.send(full_event);
    }

    /// ### ⏳ Governance: Oversight Synchronization (yield_phase_transition)
    /// Forces the current agent mission thread to yield execution back to the 
    /// Tokio scheduler.
    pub async fn yield_phase_transition(&self, agent_id: &str, phase: &str) {
        tracing::debug!(
            "⏳ [Oversight] Agent {} yielding at boundary: {}",
            agent_id,
            phase
        );

        // Emits a phase transition telemetry event for UI tracking
        self.emit_event(serde_json::json!({
            "type": "agent:phase_transition",
            "agent_id": agent_id,
            "phase": phase
        }));

        // Explicitly suspend the task to allow other scheduler components
        // (like the monitoring loops) to execute.
        tokio::task::yield_now().await;
    }
}

// Metadata: [telemetry]



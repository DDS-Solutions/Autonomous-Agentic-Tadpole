//! @docs ARCHITECTURE:Runner
//!
//! ### AI Assist Note
//! **Oversight Gate**: The human-in-the-loop safety mechanism. Intercepts
//! sensitive tool calls and skill proposals, rerouting them to the
//! **Sapphire Gate** UI for manual approval. Uses `tokio::sync::oneshot`
//! to pause agent execution until a decision is received.
//!
//! ### 🔍 Debugging & Observability
//! - **Telemetry Link**: Search `[oversight]` in tracing logs.
//! - **Failure Path**: Oversight channel timeout, duplicate entry IDs,
//!   or telemetry broadcast failure during status updates.
//! - **Trace Scope**: `server-rs::agent::runner::oversight`

use super::AgentRunner;

impl AgentRunner {
    // ─────────────────────────────────────────────────────────
    //  OVERSIGHT (HUMAN-IN-THE-LOOP)
    // ─────────────────────────────────────────────────────────

    /// Submits a tool call for manual user approval.
    /// Returns true if approved, false if rejected.
    pub async fn submit_oversight(
        &self,
        mut tool_call: crate::agent::types::ToolCallAudit,
        mission_id: Option<String>,
    ) -> Result<bool, crate::error::AppError> {
        let entry_id = uuid::Uuid::new_v4().to_string();

        tool_call.mission_id = mission_id.clone();

        let payload_json = crate::utils::security::redact_secrets(&serde_json::to_string(&tool_call).unwrap_or_default());
        let params_json = crate::utils::security::redact_secrets(&serde_json::to_string(&tool_call.params).unwrap_or_default());

        let mut sanitized_tool_call = tool_call.clone();
        if let Ok(redacted_val) = serde_json::from_str::<serde_json::Value>(&params_json) {
            sanitized_tool_call.params = redacted_val;
        }

        let entry = crate::agent::types::OversightEntry {
            id: entry_id.clone(),
            mission_id: mission_id.clone(),
            tool_call: Some(sanitized_tool_call),
            skill_proposal: None,
            status: "pending".to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
        };

        // Verify foreign key integrity for mission_id to prevent failure in ad-hoc/test environments
        let valid_mission_id = if let Some(ref mid) = mission_id {
            match sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM mission_history WHERE id = ?)")
                .bind(mid)
                .fetch_one(&self.state.resources.pool)
                .await
            {
                Ok(true) => Some(mid.clone()),
                Ok(false) => {
                    tracing::debug!("Mission ID {} does not exist in mission_history, omitting FK", mid);
                    None
                }
                Err(e) => {
                    tracing::warn!("Failed to query mission_history for mission {}: {}", mid, e);
                    None
                }
            }
        } else {
            None
        };

        // 1. [Persistence] Record action attempt in SQLite for audit history FIRST
        sqlx::query(
            "INSERT INTO oversight_log (id, mission_id, agent_id, entry_type, skill, params, status, payload) VALUES (?, ?, ?, 'tool_call', ?, ?, 'pending', ?)"
        )
        .bind(&entry_id)
        .bind(&valid_mission_id)
        .bind(&tool_call.agent_id)
        .bind(&tool_call.skill)
        .bind(&params_json)
        .bind(&payload_json)
        .execute(&self.state.resources.pool)
        .await?;

        // 2. Create channel for decision and register in memory only AFTER DB write succeeds
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.state
            .comms
            .oversight_resolvers
            .insert(entry_id.clone(), tx);

        // 3. Register in the queue for UI discovery
        self.state
            .comms
            .oversight_queue
            .insert(entry_id.clone(), entry.clone());

        // 4. Notify the UI
        self.state.emit_event(serde_json::json!({
            "type": "oversight:new",
            "entry": entry
        }));

        // 5. Await the user's click in the dashboard with a bounded timeout
        let timeout_duration = if cfg!(test) {
            std::time::Duration::from_millis(250)
        } else {
            let secs = std::env::var("OVERSIGHT_TIMEOUT_SECS")
                .ok()
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(300);
            std::time::Duration::from_secs(secs)
        };

        let approved = match tokio::time::timeout(timeout_duration, rx).await {
            Ok(Ok(decision)) => {
                self.state.comms.oversight_queue.remove(&entry_id);
                decision
            }
            Ok(Err(_)) => {
                self.state.comms.oversight_queue.remove(&entry_id);
                self.state.comms.oversight_resolvers.remove(&entry_id);
                let _ = sqlx::query("UPDATE oversight_log SET status = 'rejected' WHERE id = ?")
                    .bind(&entry_id)
                    .execute(&self.state.resources.pool)
                    .await;
                false
            }
            Err(_) => {
                tracing::warn!("⏱️ Oversight request '{}' timed out", entry_id);
                self.state.comms.oversight_queue.remove(&entry_id);
                self.state.comms.oversight_resolvers.remove(&entry_id);
                let _ = sqlx::query("UPDATE oversight_log SET status = 'timeout' WHERE id = ?")
                    .bind(&entry_id)
                    .execute(&self.state.resources.pool)
                    .await;
                false
            }
        };

        Ok(approved)
    }


    // ─────────────────────────────────────────────────────────
    //  TELEMETRY HELPERS
    // ─────────────────────────────────────────────────────────

    pub(crate) fn broadcast_agent_status(&self, agent_id: &str, mission_id: &str, status: &str) {
        let task = self
            .state
            .registry
            .agents
            .get(agent_id)
            .and_then(|a| a.state.current_task.clone());

        let _ = self.state.comms.telemetry_tx.send(serde_json::json!({
            "type": "agent:status",
            "agent_id": agent_id,
            "mission_id": mission_id,
            "status": status,
            "current_task": task
        }));
    }

    /// Centralized status and task update that syncs registry AND broadcasts telemetry.
    pub(crate) fn update_status(
        &self,
        agent_id: &str,
        mission_id: &str,
        status: &str,
        task: Option<&str>,
    ) {
        if let Some(mut agent) = self.state.registry.agents.get_mut(agent_id) {
            agent.health.status = status.to_string();
            agent.state.current_task = task.map(|t| t.to_string());
        }
        self.broadcast_agent_status(agent_id, mission_id, status);
    }

    pub(crate) fn broadcast_agent_message(&self, agent_id: &str, mission_id: &str, text: &str) {
        let _ = self.state.comms.telemetry_tx.send(serde_json::json!({
            "type": "agent:message",
            "agent_id": agent_id,
            "mission_id": mission_id,
            "text": text,
            "message_id": uuid::Uuid::new_v4().to_string()
        }));
    }
}





#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use crate::agent::types::EngineAgent;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_update_status() {
        let state = Arc::new(AppState::new_minimal_mock().await);
        let runner = AgentRunner::new(state.clone());
        let agent_id = "test-agent";
        
        let mut agent = EngineAgent::default();
        agent.identity.id = agent_id.to_string();
        state.registry.agents.insert(agent_id.to_string(), agent);

        runner.update_status(agent_id, "mission-1", "busy", Some("Thinking..."));

        let agent = state.registry.agents.get(agent_id).unwrap();
        assert_eq!(agent.health.status, "busy");
        assert_eq!(agent.state.current_task.as_deref(), Some("Thinking..."));
    }

    #[tokio::test]
    async fn test_broadcast_agent_message() {
        let state = Arc::new(AppState::new_minimal_mock().await);
        let runner = AgentRunner::new(state.clone());
        let mut rx = state.comms.telemetry_tx.subscribe();

        runner.broadcast_agent_message("agent-1", "mission-1", "Hello World");

        let msg = rx.recv().await.unwrap();
        assert_eq!(msg["type"], "agent:message");
        assert_eq!(msg["text"], "Hello World");
    }
}

// Metadata: [oversight]

//! @docs ARCHITECTURE:Runner
//!
//! ### AI Assist Note
//! **Working Memory & Shared Blackboard Tool Handlers**: Implements version-checked
//! persistent agent working-memory merges (`handle_update_working_memory`) and
//! mission-scoped `SharedBlackboard` facet read/write handlers (`handle_blackboard_write`,
//! `handle_blackboard_read`).
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Superseded mission OCC conflict on working memory, or empty blackboard key.
//! - **Telemetry Link**: Search `[blackboard_tools]` in tracing logs.

use super::error::ToolExecutionError;
use super::recursive_merge_json;
use crate::agent::runner::{AgentRunner, RunContext};
use crate::error::AppError;

impl AgentRunner {
    /// Updates the agent's persistent working memory (scratchpad) under the agent lifecycle lock.
    pub(crate) async fn handle_update_working_memory(
        &self,
        ctx: &RunContext,
        fc: &crate::agent::types::ToolCall,
        output_text: &mut String,
    ) -> Result<(), AppError> {
        let new_memory = fc
            .args
            .get("memory")
            .cloned()
            .unwrap_or(serde_json::json!({}));

        let _lifecycle = self.state.registry.lock_agent(&ctx.agent_id).await;
        let mut agent = self
            .state
            .registry
            .agents
            .get(&ctx.agent_id)
            .ok_or_else(|| AppError::NotFound("Agent was deleted".to_string()))?
            .clone();
        if agent
            .state
            .active_mission
            .as_ref()
            .and_then(|mission| mission.get("id"))
            .and_then(|id| id.as_str())
            != Some(ctx.mission_id.as_str())
        {
            return Err(AppError::Conflict(
                "Working-memory update belongs to a superseded mission".to_string(),
            ));
        }

        recursive_merge_json(&mut agent.state.working_memory, &new_memory);
        agent.version =
            crate::agent::persistence::save_agent_db(&self.state.resources.pool, &agent).await?;
        self.state
            .registry
            .agents
            .insert(ctx.agent_id.clone(), agent.clone());
        self.state
            .emit_event(serde_json::json!({"type": "agent:update", "data": agent}));
        *output_text = "(WORKING MEMORY UPDATED SUCCESSFULLY)".to_string();

        Ok(())
    }

    /// Writes a key (supports `base#facet` composite syntax) to the mission-partitioned SharedBlackboard.
    pub(crate) async fn handle_blackboard_write(
        &self,
        ctx: &RunContext,
        fc: &crate::agent::types::ToolCall,
    ) -> Result<String, ToolExecutionError> {
        let key = fc
            .args
            .get("key")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                ToolExecutionError::ExecutionFailed("'key' must be a non-empty string".to_string())
            })?;
        let value = fc
            .args
            .get("value")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        let tags: Vec<String> = fc
            .args
            .get("tags")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(ToString::to_string))
                    .collect()
            })
            .unwrap_or_default();

        let scope = ctx.blackboard_scope();
        let entry = self
            .state
            .comms
            .blackboard
            .set(scope, key, value, &ctx.agent_id, tags);
        Ok(format!(
            "Blackboard key '{}' updated (scope: '{}', version: {})",
            entry.key, scope, entry.version
        ))
    }

    /// Reads a key (`base#facet` supported) or lists entries (optionally filtered by tag) from SharedBlackboard.
    pub(crate) async fn handle_blackboard_read(
        &self,
        ctx: &RunContext,
        fc: &crate::agent::types::ToolCall,
    ) -> Result<String, ToolExecutionError> {
        let scope = ctx.blackboard_scope();
        if let Some(key) = fc
            .args
            .get("key")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            return match self.state.comms.blackboard.get(scope, key) {
                Some(entry) => serde_json::to_string_pretty(&*entry)
                    .map_err(|e| ToolExecutionError::ExecutionFailed(e.to_string())),
                None => Ok(format!(
                    "Blackboard key '{}' not found in scope '{}'",
                    key, scope
                )),
            };
        }

        let tag_filter = fc.args.get("tag").and_then(|v| v.as_str());
        let entries = self.state.comms.blackboard.list(scope, tag_filter);
        if entries.is_empty() {
            return Ok(format!("No blackboard entries found in scope '{}'", scope));
        }
        let values: Vec<&crate::agent::blackboard::BlackboardEntry> =
            entries.iter().map(|e| e.as_ref()).collect();
        serde_json::to_string_pretty(&values)
            .map_err(|e| ToolExecutionError::ExecutionFailed(e.to_string()))
    }
}

// Metadata: [blackboard_tools]

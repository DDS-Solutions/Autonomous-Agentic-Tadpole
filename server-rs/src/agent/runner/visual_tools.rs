//! @docs ARCHITECTURE:Registry
//! 
//! ### AI Assist Note
//! **@docs ARCHITECTURE:Runner**
//! This module implements high-fidelity logic for the Sovereign Reality layer.
//! 
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Runtime logic error, state desynchronization, or resource exhaustion.
//! - **Telemetry Link**: Search `[visual_tools]` in tracing logs.

/**
 * @docs ARCHITECTURE:Runner
 * 
 * ### AI Assist Note
 * **Visual & Browser Tools**: Bridges the backend agent logic with the 
 * frontend "Sovereign Browser Specialist". 
 */

use crate::agent::runner::{AgentRunner, RunContext};
use crate::agent::types::ToolCall;
use crate::agent::runner::tools::error::ToolExecutionError;
use serde_json::json;
use tokio::time::{timeout, Duration};

impl AgentRunner {
    /// Triggers the Sovereign Browser Specialist to analyze the active dashboard UI state.
    pub async fn handle_visual_inspect(
        &self,
        ctx: &RunContext,
        fc: &ToolCall,
    ) -> Result<String, ToolExecutionError> {
        let prompt = fc.args.get("prompt").and_then(|v| v.as_str()).unwrap_or("Analyze the dashboard UI.");
        let request_id = uuid::Uuid::new_v4().to_string();

        tracing::info!("🔍 [Visual] Agent {} requesting UI inspection: {}", ctx.agent_id, prompt);

        self.state.emit_event(json!({
            "type": "ui:inspect",
            "request_id": request_id,
            "agent_id": ctx.agent_id,
            "prompt": prompt
        }));

        let mut rx = self.state.comms.event_tx.subscribe();
        
        match timeout(Duration::from_secs(20), async {
            while let Ok(event) = rx.recv().await {
                if event["type"] == "ui:inspect_response" && event["request_id"] == request_id {
                    tracing::info!("🔍 [Visual] Received response for request {}", request_id);
                    return Ok(event["analysis"].as_str().unwrap_or("Analysis complete.").to_string());
                }
            }
            Err(ToolExecutionError::ExecutionFailed("Event telemetry stream closed unexpectedly.".to_string()))
        }).await {
            Ok(res) => res,
            Err(_) => {
                tracing::warn!("🔍 [Visual] UI inspection TIMEOUT for request {}", request_id);
                Err(ToolExecutionError::ExecutionFailed(
                    "UI inspection timed out. Ensure the TadpoleOS dashboard is open and active in your browser tab.".to_string()
                ))
            }
        }
    }

    /// Saves a piece of information to the browser's local vector memory.
    pub async fn handle_save_local_memory(
        &self,
        _ctx: &RunContext,
        fc: &ToolCall,
    ) -> Result<String, ToolExecutionError> {
        let text = fc.args.get("text").and_then(|v| v.as_str()).ok_or_else(|| ToolExecutionError::Validation("Missing 'text' parameter".to_string()))?;
        let metadata = fc.args.get("metadata").cloned().unwrap_or(json!({}));

        self.state.emit_event(json!({
            "type": "memory:save",
            "text": text,
            "metadata": metadata
        }));

        Ok("Information archived to local browser memory.".to_string())
    }

    /// Performs a semantic search in the browser's local vector memory.
    pub async fn handle_search_local_memory(
        &self,
        ctx: &RunContext,
        fc: &ToolCall,
    ) -> Result<String, ToolExecutionError> {
        let query = fc.args.get("query").and_then(|v| v.as_str()).ok_or_else(|| ToolExecutionError::Validation("Missing 'query' parameter".to_string()))?;
        let limit = fc.args.get("limit").and_then(|v| v.as_u64()).unwrap_or(5);
        let request_id = uuid::Uuid::new_v4().to_string();

        tracing::info!("🔍 [Memory] Agent {} searching local memory: {}", ctx.agent_id, query);

        self.state.emit_event(json!({
            "type": "memory:search",
            "request_id": request_id,
            "query": query,
            "limit": limit
        }));

        let mut rx = self.state.comms.event_tx.subscribe();
        
        match timeout(Duration::from_secs(10), async {
            while let Ok(event) = rx.recv().await {
                if event["type"] == "memory:search_response" && event["request_id"] == request_id {
                    let results = event["results"].as_array().ok_or_else(|| ToolExecutionError::ExecutionFailed("Invalid results format".to_string()))?;
                    if results.is_empty() {
                        return Ok("No relevant local memories found.".to_string());
                    }
                    return Ok(results.iter().map(|v| v.as_str().unwrap_or("")).collect::<Vec<_>>().join("\n---\n"));
                }
            }
            Err(ToolExecutionError::ExecutionFailed("Telemetry stream closed".to_string()))
        }).await {
            Ok(res) => res,
            Err(_) => Err(ToolExecutionError::ExecutionFailed("Local memory search timed out.".to_string()))
        }
    }
}

// Metadata: [visual_tools]

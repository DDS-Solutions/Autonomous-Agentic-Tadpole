/*
@docs ARCHITECTURE:SovereignKernel

### AI Assist Note
**🛡️ Tadpole OS: Transport**
Core system module providing specialized functionality for the agent swarm.

### 🔍 Debugging & Observability
- **Failure Path**: Unexpected execution drift or type compatibility issues.
- **Telemetry Link**: Traced via active system logging channels.
*/

//! MCP SSE Transport
//! Provides the Server-Sent Events (SSE) bridge to expose TadpoleOS internal tools
//! to external MCP clients (e.g. Claude Desktop, Cursor).

use crate::agent::mcp::McpResult;
use crate::error::AppError;
use crate::state::AppState;
use axum::{
    extract::{Query, State},
    response::sse::{Event, Sse},
    Json,
};
use dashmap::DashMap;
use futures::stream::Stream;
use once_cell::sync::Lazy;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{convert::Infallible, sync::Arc};
use tokio::sync::mpsc;
use tracing::{debug, error, info};

// Global session registry for SSE connections
static SSE_SESSIONS: Lazy<DashMap<String, mpsc::Sender<Event>>> = Lazy::new(DashMap::new);

#[derive(Deserialize)]
pub struct SessionQuery {
    pub session_id: String,
}

/// GET /v1/mcp/sse
/// Establishes an SSE connection for an external MCP client.
pub async fn mcp_sse_handler(
    State(_state): State<Arc<AppState>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let session_id = uuid::Uuid::new_v4().to_string();
    let (tx, rx) = mpsc::channel(100);

    SSE_SESSIONS.insert(session_id.clone(), tx.clone());
    info!("🔗 [MCP Bridge] New SSE connection established: {}", session_id);

    // Send the endpoint event as required by MCP spec
    let endpoint_url = format!("/v1/mcp/message?session_id={}", session_id);
    let _ = tx.send(Event::default().event("endpoint").data(endpoint_url)).await;

    let stream = futures::stream::unfold(rx, |mut rx| async move {
        match rx.recv().await {
            Some(event) => Some((Ok::<_, Infallible>(event), rx)),
            None => None,
        }
    });
    Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::new())
}

/// POST /v1/mcp/message
/// Receives JSON-RPC messages from external MCP clients.
pub async fn mcp_message_handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SessionQuery>,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, AppError> {
    let session_id = query.session_id;
    let sender = SSE_SESSIONS.get(&session_id).ok_or_else(|| {
        AppError::BadRequest("Invalid or expired session_id".to_string())
    })?;

    let method = payload.get("method").and_then(|v| v.as_str()).unwrap_or("");
    let id = payload.get("id").cloned();
    let params = payload.get("params").cloned().unwrap_or(json!({}));

    debug!("📥 [MCP Bridge] Received '{}' from {}", method, session_id);

    let response = match method {
        "initialize" => {
            json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "protocolVersion": "2024-11-05",
                    "capabilities": {
                        "tools": { "listChanged": true }
                    },
                    "serverInfo": {
                        "name": "tadpole-os",
                        "version": env!("CARGO_PKG_VERSION")
                    }
                }
            })
        }
        "notifications/initialized" => {
            // Nothing to return for a notification
            return Ok(Json(json!({ "status": "accepted" })));
        }
        "tools/list" => {
            let snapshot = state.registry.skills.snapshot();
            let all_agent_skills: Vec<String> = snapshot.skills.iter().map(|kv| kv.key().clone()).collect();
            
            // Generate MCP-compliant tool list
            let internal_tools = state.registry.mcp_host.list_tools(&all_agent_skills, &snapshot.skills).await;
            
            let mcp_tools: Vec<Value> = internal_tools.into_iter().map(|t| {
                json!({
                    "name": t.name,
                    "description": t.description,
                    "inputSchema": t.input_schema
                })
            }).collect();

            json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": { "tools": mcp_tools }
            })
        }
        "tools/call" => {
            let tool_name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let arguments = params.get("arguments").cloned().unwrap_or(json!({}));
            
            // Define standard external debug workspace
            let workspace_root = std::path::PathBuf::from("data/workspaces/api_debug");
            if !workspace_root.exists() {
                let _ = std::fs::create_dir_all(&workspace_root);
            }

            let snapshot = state.registry.skills.snapshot();
            
            let tool_ctx = crate::agent::types::ToolContext {
                mission_id: "mcp-bridge".to_string(),
                agent_id: "mcp-client".to_string(),
                workspace_root: workspace_root.clone(),
                fs_adapter: crate::adapter::filesystem::FilesystemAdapter::new(workspace_root.clone()),
                state: state.clone(),
                trace_id: uuid::Uuid::new_v4().to_string(),
                budget_usd: 0.0,
                budget_limit_usd: 10.0,
                security_policy: serde_json::json!({}),
                active_node_id: None,
            };

            match state.registry.mcp_host.call_tool(tool_name, arguments, &tool_ctx, &snapshot.skills).await {
                Ok(McpResult::Raw(output)) => {
                    json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "content": [
                                { "type": "text", "text": output }
                            ]
                        }
                    })
                }
                Ok(McpResult::SystemDelegate(sys_name, _)) => {
                    json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": {
                            "code": -32601,
                            "message": format!("System tool '{}' requires an internal TadpoleOS agent context.", sys_name)
                        }
                    })
                }
                Err(e) => {
                    json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": {
                            "code": -32000,
                            "message": e.to_string()
                        }
                    })
                }
            }
        }
        _ => {
            json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {
                    "code": -32601,
                    "message": format!("Method not found: {}", method)
                }
            })
        }
    };

    // Send the JSON-RPC response back over the SSE channel
    let event = Event::default().event("message").data(response.to_string());
    if let Err(e) = sender.send(event).await {
        error!("🚨 [MCP Bridge] Failed to send SSE message: {}", e);
    }

    Ok(Json(json!({ "status": "accepted" })))
}

// Metadata: [transport]

// Metadata: [transport]

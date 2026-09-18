//! @docs ARCHITECTURE:Registry
//!
//! ### AI Context Alignment
//! - **Subsystem**: Sovereign Engine / MCP / IPC Bridge
//! - **Primary Entrypoints**: `IpcBridge`, `JsonRpcRequest`, `JsonRpcResponse`
//!
//! ### ⚠️ Invariants & Non-Negotiables
//! - `[Structural]` Zero external dependencies — uses only tokio + serde_json.
//! - `[Structural]` Named pipe path is deterministic and scoped to the workspace.
//! - `[Structural]` Framed JSON-RPC 2.0 over newline-delimited JSON (NDJSON).
//!
//! ### 🔍 Debugging & Observability
//! - **Local Errors**: `IPC_BRIDGE_001` (bind failure), `IPC_BRIDGE_002` (frame parse error)
//! - **Telemetry Targets**: none declared
//! - **Witness Tests**: `test_jsonrpc_request_parsing`, `test_dispatch_ping`

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Notify;

/// JSON-RPC 2.0 Request (subset).
#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: serde_json::Value,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

/// JSON-RPC 2.0 Response.
#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl JsonRpcResponse {
    pub fn success(id: serde_json::Value, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(id: serde_json::Value, code: i32, message: String) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message,
                data: None,
            }),
        }
    }
}

/// IPC Bridge server — exposes tool registry to local Python scripts via Named Pipe (Windows)
/// or Unix Domain Socket (Linux/macOS) using newline-delimited JSON-RPC 2.0.
pub struct IpcBridge {
    pipe_path: PathBuf,
    workspace_root: PathBuf,
    tool_registry: Arc<crate::agent::runner::tools::registry::ToolRegistry>,
    shutdown: Arc<Notify>,
}

impl IpcBridge {
    /// Deterministic pipe path: `\\.\pipe\tadpoleos-ipc-{workspace_hash}` (Windows)
    /// or `/tmp/tadpoleos-ipc-{workspace_hash}.sock` (Unix).
    pub fn pipe_path_for(workspace_root: &std::path::Path) -> PathBuf {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(workspace_root.to_string_lossy().as_bytes());
        let hash = hex::encode(hasher.finalize());
        let short_hash = &hash[..16];

        #[cfg(windows)]
        {
            PathBuf::from(format!(r"\\.\pipe\tadpoleos-ipc-{}", short_hash))
        }

        #[cfg(not(windows))]
        {
            PathBuf::from(format!("/tmp/tadpoleos-ipc-{}.sock", short_hash))
        }
    }

    pub fn new(
        workspace_root: &std::path::Path,
        tool_registry: Arc<crate::agent::runner::tools::registry::ToolRegistry>,
    ) -> Self {
        let pipe_path = Self::pipe_path_for(workspace_root);

        // Write discovery file for client auto-connect
        let discovery_dir = workspace_root.join(".tmp");
        let _ = std::fs::create_dir_all(&discovery_dir);
        let _ = std::fs::write(
            discovery_dir.join("ipc_bridge_path.txt"),
            pipe_path.to_string_lossy().as_bytes(),
        );

        Self {
            pipe_path,
            workspace_root: workspace_root.to_path_buf(),
            tool_registry,
            shutdown: Arc::new(Notify::new()),
        }
    }

    /// Returns the pipe path for external callers to connect to.
    pub fn path(&self) -> &std::path::Path {
        &self.pipe_path
    }

    /// Starts the IPC listener. Returns a JoinHandle for the accept loop.
    pub fn start(self: &Arc<Self>) -> tokio::task::JoinHandle<()> {
        let bridge = Arc::clone(self);
        tokio::spawn(async move {
            if let Err(e) = bridge.run_accept_loop().await {
                tracing::error!(target: "ipc_bridge", "[IPC_BRIDGE_001] Accept loop error: {}", e);
            }
        })
    }

    /// Signal the accept loop to stop.
    pub fn shutdown(&self) {
        self.shutdown.notify_waiters();
    }

    #[cfg(windows)]
    async fn run_accept_loop(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        use tokio::net::windows::named_pipe::{PipeMode, ServerOptions};

        tracing::info!(target: "ipc_bridge", "🔌 IPC Bridge listening on: {}", self.pipe_path.display());

        loop {
            let pipe = ServerOptions::new()
                .first_pipe_instance(false)
                .pipe_mode(PipeMode::Byte)
                .create(&self.pipe_path)?;

            tokio::select! {
                result = pipe.connect() => {
                    match result {
                        Ok(()) => {
                            let registry = Arc::clone(&self.tool_registry);
                            let root = self.workspace_root.clone();
                            tokio::spawn(async move {
                                if let Err(e) = Self::handle_connection(pipe, registry, root).await {
                                    tracing::warn!(target: "ipc_bridge", "Client connection error: {}", e);
                                }
                            });
                        }
                        Err(e) => {
                            tracing::warn!(target: "ipc_bridge", "Pipe connect error: {}", e);
                        }
                    }
                }
                _ = self.shutdown.notified() => {
                    tracing::info!(target: "ipc_bridge", "IPC Bridge shutting down");
                    break;
                }
            }
        }
        Ok(())
    }

    #[cfg(not(windows))]
    async fn run_accept_loop(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        use tokio::net::UnixListener;

        let _ = std::fs::remove_file(&self.pipe_path);
        let listener = UnixListener::bind(&self.pipe_path)?;
        tracing::info!(target: "ipc_bridge", "🔌 IPC Bridge listening on: {}", self.pipe_path.display());

        loop {
            tokio::select! {
                result = listener.accept() => {
                    match result {
                        Ok((stream, _)) => {
                            let registry = Arc::clone(&self.tool_registry);
                            let root = self.workspace_root.clone();
                            tokio::spawn(async move {
                                if let Err(e) = Self::handle_connection(stream, registry, root).await {
                                    tracing::warn!(target: "ipc_bridge", "Client connection error: {}", e);
                                }
                            });
                        }
                        Err(e) => {
                            tracing::warn!(target: "ipc_bridge", "Accept error: {}", e);
                        }
                    }
                }
                _ = self.shutdown.notified() => {
                    tracing::info!(target: "ipc_bridge", "IPC Bridge shutting down");
                    let _ = std::fs::remove_file(&self.pipe_path);
                    break;
                }
            }
        }
        Ok(())
    }

    async fn handle_connection<S>(
        stream: S,
        registry: Arc<crate::agent::runner::tools::registry::ToolRegistry>,
        workspace_root: PathBuf,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
    {
        let (reader, mut writer) = tokio::io::split(stream);
        let mut lines = BufReader::new(reader).lines();

        while let Some(line) = lines.next_line().await? {
            let line = line.trim().to_string();
            if line.is_empty() {
                continue;
            }

            let response = match serde_json::from_str::<JsonRpcRequest>(&line) {
                Ok(req) => Self::dispatch(&req, &registry, &workspace_root).await,
                Err(e) => {
                    tracing::debug!(target: "ipc_bridge", "[IPC_BRIDGE_002] Parse error: {}", e);
                    JsonRpcResponse::error(
                        serde_json::Value::Null,
                        -32700,
                        format!("Parse error: {}", e),
                    )
                }
            };

            let mut out = serde_json::to_string(&response)?;
            out.push('\n');
            writer.write_all(out.as_bytes()).await?;
            writer.flush().await?;
        }

        Ok(())
    }

    async fn dispatch(
        req: &JsonRpcRequest,
        registry: &crate::agent::runner::tools::registry::ToolRegistry,
        workspace_root: &std::path::Path,
    ) -> JsonRpcResponse {
        match req.method.as_str() {
            "list_tools" => {
                let tools: Vec<serde_json::Value> = registry
                    .list_tools()
                    .iter()
                    .map(|t| {
                        serde_json::json!({
                            "name": t.name,
                            "description": t.description,
                            "parameters": t.parameters,
                        })
                    })
                    .collect();

                JsonRpcResponse::success(req.id.clone(), serde_json::json!(tools))
            }

            "get_tool_schema" => {
                let tool_name = req
                    .params
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                match registry.get(tool_name) {
                    Some(tool) => {
                        let meta = tool.metadata();
                        JsonRpcResponse::success(
                            req.id.clone(),
                            serde_json::json!({
                                "name": meta.name,
                                "description": meta.description,
                                "parameters": meta.parameters,
                            }),
                        )
                    }
                    None => JsonRpcResponse::error(
                        req.id.clone(),
                        -32601,
                        format!("Tool '{}' not found", tool_name),
                    ),
                }
            }

            "ping" => JsonRpcResponse::success(req.id.clone(), serde_json::json!("pong")),

            "call_tool" => {
                let tool_name = req
                    .params
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let args = req
                    .params
                    .get("arguments")
                    .cloned()
                    .unwrap_or(serde_json::json!({}));

                match registry.get(tool_name) {
                    Some(tool) => {
                        let mock_state = Arc::new(crate::state::AppState::new_minimal_mock().await);
                        let tool_ctx = crate::agent::types::ToolContext {
                            mission_id: "ipc-mission".to_string(),
                            agent_id: "ipc-client".to_string(),
                            workspace_root: workspace_root.to_path_buf(),
                            fs_adapter: crate::adapter::filesystem::FilesystemAdapter::new(workspace_root.to_path_buf()),
                            state: mock_state,
                            trace_id: uuid::Uuid::new_v4().to_string(),
                            budget_usd: 0.0,
                            budget_limit_usd: 10.0,
                            security_policy: serde_json::json!({}),
                            active_node_id: None,
                        };
                        let mut usage = None;
                        match tool.execute(&tool_ctx, args, &mut usage).await {
                            Ok(output) => {
                                JsonRpcResponse::success(req.id.clone(), serde_json::json!({ "output": output }))
                            }
                            Err(e) => {
                                JsonRpcResponse::error(
                                    req.id.clone(),
                                    -32000,
                                    format!("Tool execution error: {}", e),
                                )
                            }
                        }
                    }
                    None => JsonRpcResponse::error(
                        req.id.clone(),
                        -32601,
                        format!("Tool '{}' not found", tool_name),
                    ),
                }
            }

            _ => JsonRpcResponse::error(
                req.id.clone(),
                -32601,
                format!(
                    "Method '{}' not found. Available: list_tools, get_tool_schema, ping, call_tool",
                    req.method
                ),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipe_path_deterministic() {
        let p1 = IpcBridge::pipe_path_for(std::path::Path::new("/workspace/project"));
        let p2 = IpcBridge::pipe_path_for(std::path::Path::new("/workspace/project"));
        assert_eq!(p1, p2, "Same workspace root should produce same pipe path");
    }

    #[test]
    fn test_pipe_path_different_workspaces() {
        let p1 = IpcBridge::pipe_path_for(std::path::Path::new("/workspace/project-a"));
        let p2 = IpcBridge::pipe_path_for(std::path::Path::new("/workspace/project-b"));
        assert_ne!(
            p1, p2,
            "Different workspace roots should produce different pipe paths"
        );
    }

    #[test]
    fn test_jsonrpc_response_success_serialization() {
        let resp = JsonRpcResponse::success(
            serde_json::json!(1),
            serde_json::json!({"tools": ["read_file"]}),
        );
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"jsonrpc\":\"2.0\""));
        assert!(json.contains("\"result\""));
        assert!(!json.contains("\"error\""));
    }

    #[test]
    fn test_jsonrpc_response_error_serialization() {
        let resp = JsonRpcResponse::error(
            serde_json::json!(2),
            -32601,
            "Method not found".to_string(),
        );
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"jsonrpc\":\"2.0\""));
        assert!(json.contains("\"error\""));
        assert!(json.contains("\"code\":-32601"));
        assert!(!json.contains("\"result\""));
    }

    #[tokio::test]
    async fn test_dispatch_ping() {
        let registry = crate::agent::runner::tools::registry::ToolRegistry::new();
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: serde_json::json!(10),
            method: "ping".to_string(),
            params: serde_json::Value::Null,
        };
        let resp = IpcBridge::dispatch(&req, &registry, std::path::Path::new(".")).await;
        assert_eq!(resp.result, Some(serde_json::json!("pong")));
        assert!(resp.error.is_none());
    }

    #[tokio::test]
    async fn test_dispatch_call_tool_not_found() {
        let registry = crate::agent::runner::tools::registry::ToolRegistry::new();
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: serde_json::json!(11),
            method: "call_tool".to_string(),
            params: serde_json::json!({
                "name": "nonexistent_tool",
                "arguments": {}
            }),
        };
        let resp = IpcBridge::dispatch(&req, &registry, std::path::Path::new(".")).await;
        assert!(resp.result.is_none());
        assert!(resp.error.is_some());
        assert_eq!(resp.error.unwrap().code, -32601);
    }
}

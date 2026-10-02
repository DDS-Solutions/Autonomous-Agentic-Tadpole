//! MCP Client Implementation - Low-level JSON-RPC Bridge
//!
//! @docs ARCHITECTURE:Registry:Mcp
//!
//! ### AI Assist Note
//! **Module: client.rs**
//! Provides the raw stdio-based JSON-RPC bridge for communicating with
//! Model Context Protocol (MCP) servers.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Zombie processes, broken pipes, or JSON-RPC protocol mismatches.
//! - **Telemetry Link**: Search `[client]` in tracing logs.
//! - **Trace Scope**: `server-rs::agent::mcp::client`
//!

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tracing::{debug, info};
use crate::error::AppError;

#[derive(Debug, Serialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Value,
    pub method: String,
    pub params: Value,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub result: Option<Value>,
    pub error: Option<Value>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct JsonRpcNotification {
    pub jsonrpc: String,
    pub method: String,
    pub params: Option<Value>,
}

pub struct McpClient {
    #[allow(dead_code)]
    child: Child,
    pub(crate) stdin: ChildStdin,
    pub(crate) stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl McpClient {
    /// Spawns an MCP server from structured program command, args, and optional env.
    /// Preserves argument boundaries, avoiding shell parsing and whitespace splitting.
    pub async fn spawn(
        program: &str,
        args: &[String],
        env: Option<&HashMap<String, String>>,
    ) -> Result<Self, AppError> {
        let trimmed_program = program.trim();
        if trimmed_program.is_empty() {
            return Err(AppError::BadRequest("Empty command".to_string()));
        }
        info!("🚀 [client] [MCP] Spawning server: {}", trimmed_program);

        let mut command = Command::new(trimmed_program);
        command.args(args);
        command.kill_on_drop(true);

        // SEC: Isolate child process from host parent secrets (INV-SEC-003)
        // Clear ambient environment and forward only safe system loader variables plus explicit server config env.
        command.env_clear();
        const SAFE_SYSTEM_VARS: &[&str] = &[
            "PATH", "PATHEXT", "SYSTEMROOT", "COMSPEC", "TEMP", "TMP", "USERPROFILE",
            "HOME", "TMPDIR", "USER", "SHELL", "LANG", "LC_ALL",
        ];
        for key in SAFE_SYSTEM_VARS {
            if let Ok(val) = std::env::var(key) {
                command.env(key, val);
            }
        }
        if let Some(env_map) = env {
            command.envs(env_map);
        }

        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit()) // Log stderr to the console
            .spawn()
            .map_err(AppError::Io)?;

        let stdin = child.stdin.take().ok_or_else(|| AppError::InternalServerError("Failed to open stdin".to_string()))?;
        let stdout = child.stdout.take().ok_or_else(|| AppError::InternalServerError("Failed to open stdout".to_string()))?;

        Ok(Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            next_id: 1,
        })
    }

    pub async fn call(&mut self, method: &str, params: Value) -> Result<Value, AppError> {
        let id = self.next_id;
        self.next_id += 1;

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: json!(id),
            method: method.to_string(),
            params,
        };

        let request_str = serde_json::to_string(&request).map_err(|e| AppError::InternalServerError(e.to_string()))? + "\n";
        debug!(">> [MCP] Sending: {}", request_str.trim());
        self.stdin.write_all(request_str.as_bytes()).await.map_err(AppError::Io)?;
        self.stdin.flush().await.map_err(AppError::Io)?;

        let mut response_line = String::new();
        self.stdout.read_line(&mut response_line).await.map_err(AppError::Io)?;
        debug!("<< [MCP] Received: {}", response_line.trim());

        if response_line.is_empty() {
            return Err(AppError::InfrastructureError {
                provider_id: "mcp_client".to_string(),
                detail: "MCP server closed connection".to_string(),
                help_link: None,
            });
        }

        let response: JsonRpcResponse = serde_json::from_str(&response_line).map_err(|e| AppError::InternalServerError(e.to_string()))?;
        
        if let Some(error) = response.error {
            return Err(AppError::InfrastructureError {
                provider_id: "mcp_client".to_string(),
                detail: format!("MCP Error: {}", error),
                help_link: None,
            });
        }

        response.result.ok_or_else(|| AppError::InfrastructureError {
            provider_id: "mcp_client".to_string(),
            detail: "Missing result in MCP response".to_string(),
            help_link: None,
        })
    }

    pub async fn initialize(&mut self) -> Result<(), AppError> {
        let params = json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "tadpole-os-engine",
                "version": "1.1.0"
            }
        });

        self.call("initialize", params).await?;
        
        // Notify initialized
        let notification = json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        });
        let notif_str = serde_json::to_string(&notification).map_err(|e| AppError::InternalServerError(e.to_string()))? + "\n";
        self.stdin.write_all(notif_str.as_bytes()).await.map_err(AppError::Io)?;
        self.stdin.flush().await.map_err(AppError::Io)?;

        Ok(())
    }

    #[allow(dead_code)]
    pub async fn list_tools(&mut self) -> Result<Vec<Value>, AppError> {
        let result = self.call("tools/list", json!({})).await?;
        let tools = result.get("tools")
            .and_then(|t| t.as_array())
            .cloned()
            .unwrap_or_default();
        Ok(tools)
    }

    pub async fn call_tool(&mut self, name: &str, arguments: Value) -> Result<Value, AppError> {
        let params = json!({
            "name": name,
            "arguments": arguments
        });
        self.call("tools/call", params).await
    }
}

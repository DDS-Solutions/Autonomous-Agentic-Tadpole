//! @docs ARCHITECTURE:Agent
//!
//! ### AI Assist Note
//! **External MCP Server & Legacy Skill Bridge**: Handles `mcp_config.json` mtime-cached
//! loading, prefixed external tool resolution, agent capability allow-list checks,
//! double-checked external client spawning, bounded JSON-RPC formatting, and sandboxed
//! legacy skill execution.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: External MCP spawn error, disallowed command/arg token, or unauthorized agent skill call.
//! - **Telemetry Link**: Search `[external]` or `[McpHost]` in tracing logs.

use super::client;
use super::env::{
    floor_utf8_char_boundary, resolve_mcp_env, truncate_mcp_reply, McpConfig, MAX_MCP_REPLY_BYTES,
};
use super::{McpHost, McpResult};
use crate::agent::script_skills::SkillDefinition;
use crate::error::AppError;
use std::sync::Arc;
use tokio::sync::Mutex;

impl McpHost {
    /// Loads and caches `mcp_config.json` anchored to `workspace_root` with mtime validation and size bounds.
    pub async fn get_config(&self) -> Result<Arc<McpConfig>, AppError> {
        let config_path = self
            .mcp_config_path
            .as_ref()
            .ok_or_else(|| AppError::NotFound("MCP config path not configured".to_string()))?;

        let safe_path = crate::utils::security::validate_path(
            &self.workspace_root,
            &config_path.to_string_lossy(),
        )
        .map_err(|e| {
            tracing::warn!("⚠️ [McpHost] MCP config path outside workspace: {}", e);
            AppError::Forbidden(format!("MCP config path outside workspace: {}", e))
        })?;

        let metadata = tokio::fs::metadata(&safe_path).await.map_err(|e| {
            tracing::warn!(
                "⚠️ [McpHost] Failed to stat MCP config file at {:?}: {}",
                safe_path,
                e
            );
            AppError::Io(e)
        })?;

        let mtime = metadata
            .modified()
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);

        {
            let cache = self.cached_config.read();
            if let Some((cached_mtime, ref cfg)) = *cache {
                if cached_mtime == mtime {
                    return Ok(cfg.clone());
                }
            }
        }

        // Bounded read (1 MB max)
        if metadata.len() > 1_000_000 {
            return Err(AppError::BadRequest(
                "MCP config file exceeds 1MB limit".to_string(),
            ));
        }

        let content = tokio::fs::read_to_string(&safe_path).await.map_err(|e| {
            tracing::error!(
                "❌ [McpHost] Failed to read MCP config at {:?}: {}",
                safe_path,
                e
            );
            AppError::Io(e)
        })?;

        let config: McpConfig = serde_json::from_str(&content).map_err(|e| {
            tracing::error!(
                "❌ [McpHost] Malformed MCP config JSON at {:?}: {}",
                safe_path,
                e
            );
            AppError::BadRequest(format!("Malformed MCP config: {}", e))
        })?;

        let config_arc = Arc::new(config);
        {
            let mut cache = self.cached_config.write();
            *cache = Some((mtime, config_arc.clone()));
        }

        Ok(config_arc)
    }

    /// Resolves a prefixed external MCP tool name (`server::tool`, `server:tool`, or `mcp_server_tool`)
    /// purely against the configured server names without spawning subprocesses.
    pub(crate) fn match_prefixed_external_tool(
        tool_name: &str,
        config: &McpConfig,
    ) -> Option<(String, String)> {
        if let Some((srv, tool)) = tool_name
            .split_once("::")
            .or_else(|| tool_name.split_once(':'))
        {
            if config.mcp_servers.contains_key(srv) && !tool.is_empty() {
                return Some((srv.to_string(), tool.to_string()));
            }
        } else if let Some(rest) = tool_name.strip_prefix("mcp_") {
            for srv in config.mcp_servers.keys() {
                let prefix = format!("{}_", srv);
                if let Some(actual) = rest.strip_prefix(&prefix) {
                    if !actual.is_empty() {
                        return Some((srv.clone(), actual.to_string()));
                    }
                }
            }
        }
        None
    }

    /// Resolves a tool name to `(server_name, actual_tool_name)` across configured MCP servers.
    pub(super) async fn resolve_external_mcp_tool(&self, tool_name: &str) -> Option<(String, String)> {
        let config = self.get_config().await.ok()?;
        if let Some(matched) = Self::match_prefixed_external_tool(tool_name, &config) {
            return Some(matched);
        }

        for srv in config.mcp_servers.keys() {
            if let Ok(client) = self.get_or_spawn_client(srv).await {
                let mut client_lock = client.lock().await;
                if let Ok(tools) = client_lock.list_tools().await {
                    if tools
                        .iter()
                        .any(|t| t.get("name").and_then(|v| v.as_str()) == Some(tool_name))
                    {
                        return Some((srv.clone(), tool_name.to_string()));
                    }
                }
            }
        }
        None
    }

    /// Checks whether the calling agent's capability allow-list grants access to an external MCP tool.
    pub(super) fn is_agent_authorized_for_external_tool(
        ctx: &crate::agent::types::ToolContext,
        tool_name: &str,
        server_name: &str,
        actual_tool_name: &str,
    ) -> bool {
        if ctx.agent_id == "system" || ctx.agent_id == "admin" {
            return true;
        }
        let Some(agent_entry) = ctx.state.registry.agents.get(&ctx.agent_id) else {
            return true;
        };
        let granted = &agent_entry.capabilities.skills;
        let namespaced_colon = format!("{}:{}", server_name, actual_tool_name);
        let namespaced_double_colon = format!("{}::{}", server_name, actual_tool_name);
        let legacy_mcp = format!("mcp_{}_{}", server_name, actual_tool_name);

        granted.iter().any(|s| {
            s == tool_name
                || s == actual_tool_name
                || s == &namespaced_colon
                || s == &namespaced_double_colon
                || s == &legacy_mcp
        })
    }

    /// Formats a raw JSON-RPC MCP tool result into a bounded `McpResult::Raw` payload.
    pub(super) fn format_external_mcp_result(result: &serde_json::Value) -> Result<McpResult, AppError> {
        if let Some(content) = result.get("content").and_then(|c| c.as_array()) {
            let mut output = String::new();
            for item in content {
                if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                    if output.len().saturating_add(text.len()) > MAX_MCP_REPLY_BYTES {
                        let remaining = MAX_MCP_REPLY_BYTES.saturating_sub(output.len());
                        let safe_end = floor_utf8_char_boundary(text, remaining);
                        output.push_str(&text[..safe_end]);
                        output.push_str("\n\n[TRUNCATED: MCP response exceeded 2MB limit]");
                        break;
                    } else {
                        output.push_str(text);
                    }
                }
            }
            return Ok(McpResult::Raw(output));
        }

        let serialized = serde_json::to_string_pretty(result)
            .map_err(|e| AppError::InternalServerError(e.to_string()))?;
        Ok(McpResult::Raw(truncate_mcp_reply(
            &serialized,
            MAX_MCP_REPLY_BYTES,
            "MCP response",
        )))
    }

    pub(super) async fn get_or_spawn_client(
        &self,
        server_name: &str,
    ) -> Result<Arc<Mutex<client::McpClient>>, AppError> {
        // Double-checked locking fast path: return cached client if already live
        {
            let clients = self.clients.lock().await;
            if let Some(client) = clients.get(server_name) {
                return Ok(client.clone());
            }
        }

        let config = self.get_config().await?;
        let server_config = config.mcp_servers.get(server_name).ok_or_else(|| {
            AppError::NotFound(format!("MCP server '{}' not found in config", server_name))
        })?;

        // Validate command binary and all configured arguments against zero-trust policy
        crate::utils::security::validate_shell_command(&server_config.command)?;
        crate::utils::security::validate_tokenized_command(
            &server_config.command,
            &server_config.args,
        )?;

        let resolved_env = server_config
            .env
            .as_ref()
            .map(resolve_mcp_env)
            .transpose()?;

        // Slow path: spawn and initialize outside the clients lock
        let mut client = client::McpClient::spawn(
            &server_config.command,
            &server_config.args,
            resolved_env.as_ref(),
        )
        .await
        .map_err(|e| AppError::InfrastructureError {
            provider_id: format!("mcp:{}", server_name),
            detail: format!("Failed to spawn MCP server: {}", e),
            help_link: None,
        })?;

        client
            .initialize()
            .await
            .map_err(|e| AppError::InfrastructureError {
                provider_id: format!("mcp:{}", server_name),
                detail: format!("Failed to initialize MCP client: {}", e),
                help_link: None,
            })?;

        let client_arc = Arc::new(Mutex::new(client));
        {
            let mut clients = self.clients.lock().await;
            if let Some(existing) = clients.get(server_name) {
                return Ok(existing.clone());
            }
            clients.insert(server_name.to_string(), client_arc.clone());
        }

        Ok(client_arc)
    }

    pub(super) async fn execute_legacy_skill(
        &self,
        skill: &SkillDefinition,
        arguments: serde_json::Value,
        workspace_root: std::path::PathBuf,
    ) -> Result<String, AppError> {
        let args_json = serde_json::to_string(&arguments).unwrap_or_default();

        let config = crate::security::sandbox::SandboxConfig::default();
        let raw_output = crate::security::sandbox::execute_sandboxed(
            &skill.execution_command,
            &args_json,
            &workspace_root,
            &config,
        )
        .await?;

        // SEC-04: Cap response payload with UTF-8 boundary safety and proactively scrub stdout
        let bounded_output = truncate_mcp_reply(&raw_output, MAX_MCP_REPLY_BYTES, "Skill reply");
        let scrubbed_stdout = self.redactor.scrub(&bounded_output);
        Ok(scrubbed_stdout)
    }
}

// Metadata: [external]

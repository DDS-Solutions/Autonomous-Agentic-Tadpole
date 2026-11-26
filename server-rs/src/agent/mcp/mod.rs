//! @docs ARCHITECTURE:Agent
//!
//! ### AI Assist Note
//! **Model Context Protocol (MCP) Host**: Orchestrates sovereign tool execution,
//! permission policy gates, telemetry stats/pulses, and secret scrubbing across
//! native Hydra-RS tools (`builtins`), legacy sandboxed skills, and external MCP
//! servers (`external`, `env`).
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Permission denied due to `Deny` policy mode,
//!   timeout during external MCP server execution, or malformed
//!   JSON-RPC tool arguments causing parse failures.
//! - **Telemetry Link**: Search for `[mod]` or `engine:mcp_pulse` in
//!   `tracing` logs for real-time tool performance tracking.
//! - **Trace Scope**: `server-rs::agent::mcp`

pub mod builtins;
pub mod client;
pub mod env;
pub mod external;
pub mod ipc_bridge;
pub mod registry;
pub mod transport;

#[cfg(test)]
mod tests;

pub use builtins::{
    get_blast_radius, get_current_time, get_symbol_body, list_file_symbols, recruit_specialist,
    run_integrity_check, GetBlastRadiusHandler, GetCurrentTimeHandler, GetSymbolBodyHandler,
    InspectEngineHealthHandler, ListFileSymbolsHandler, RecruitSpecialistHandler,
    RunIntegrityCheckHandler,
};
pub use env::{
    floor_utf8_char_boundary, resolve_mcp_env, resolve_mcp_env_with, truncate_mcp_reply,
    McpConfig, McpServerConfig, MAX_MCP_REPLY_BYTES,
};
pub use ipc_bridge::IpcBridge;

use self::registry::McpRegistry;
use crate::agent::script_skills::SkillDefinition;
use crate::error::AppError;
use crate::secret_redactor::SecretRedactor;
use crate::security::permissions::{PermissionMode, PermissionPolicy, PermissionPrompter};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};

/// Operational statistics for a specific tool.
#[derive(Debug, Clone, Serialize, Deserialize, Default, Type)]
pub struct McpToolStats {
    pub invocations: u64,
    pub success_count: u64,
    pub failure_count: u64,
    pub avg_latency_ms: u64,
}

/// A structured tool definition registered within the MCP ecosystem.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct McpToolHub {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    pub source: String,
    pub stats: McpToolStats,
    pub category: String,
}

impl From<SkillDefinition> for McpToolHub {
    fn from(skill: SkillDefinition) -> Self {
        Self {
            name: skill.name,
            description: skill.description,
            input_schema: skill.schema,
            source: "legacy".to_string(),
            stats: McpToolStats::default(),
            category: skill.category,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "type", content = "data")]
pub enum McpResult {
    Raw(String),
    SystemDelegate(String, serde_json::Value),
}

type CachedConfigEntry = Option<(std::time::SystemTime, Arc<McpConfig>)>;

/// The primary host for managing tool registration and execution.
pub struct McpHost {
    pub registry: Arc<Mutex<McpRegistry>>,
    pub stats: Arc<DashMap<String, McpToolStats>>,
    event_tx: broadcast::Sender<serde_json::Value>,
    pub(super) mcp_config_path: Option<PathBuf>,
    pub workspace_root: PathBuf,
    pub(super) cached_config: Arc<parking_lot::RwLock<CachedConfigEntry>>,
    pub policy: Arc<PermissionPolicy>,
    pub prompter: Arc<parking_lot::RwLock<Option<Arc<dyn PermissionPrompter>>>>,
    pub clients: Arc<Mutex<HashMap<String, Arc<Mutex<client::McpClient>>>>>,
    pub redactor: Arc<SecretRedactor>,
}

impl McpHost {
    pub fn new(
        event_tx: broadcast::Sender<serde_json::Value>,
        mcp_config_path: Option<PathBuf>,
        policy: Arc<PermissionPolicy>,
        redactor: Arc<SecretRedactor>,
    ) -> Self {
        let workspace_root = std::env::var("WORKSPACE_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                let cur = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
                if cur.ends_with("server-rs") {
                    cur.parent().map(PathBuf::from).unwrap_or(cur)
                } else {
                    cur
                }
            });

        let mut registry = McpRegistry::new();
        let stats = Arc::new(DashMap::new());
        let clients = Arc::new(Mutex::new(HashMap::new()));

        builtins::register_native_tools(&mut registry, stats.clone());

        Self {
            registry: Arc::new(Mutex::new(registry)),
            stats,
            event_tx,
            mcp_config_path,
            workspace_root,
            cached_config: Arc::new(parking_lot::RwLock::new(None)),
            policy,
            prompter: Arc::new(parking_lot::RwLock::new(None)),
            clients,
            redactor,
        }
    }

    pub fn set_prompter(&self, prompter: Arc<dyn PermissionPrompter>) {
        *self.prompter.write() = Some(prompter);
    }

    pub fn with_prompter(self, prompter: Arc<dyn PermissionPrompter>) -> Self {
        *self.prompter.write() = Some(prompter);
        self
    }

    pub async fn list_tools(
        &self,
        agent_skills: &[String],
        all_skills: &DashMap<String, SkillDefinition>,
    ) -> Vec<McpToolHub> {
        let mut tools: Vec<McpToolHub> = Vec::new();
        let mut seen_names = std::collections::HashSet::new();

        // 1. Native tools (highest precedence)
        {
            let registry = self.registry.lock().await;
            for mut t in registry.list_all() {
                if let Some(s) = self.stats.get(&t.name) {
                    t.stats = s.clone();
                }
                seen_names.insert(t.name.clone());
                tools.push(t);
            }
        }

        // 2. Granted skills (legacy/custom skills)
        for skill_name in agent_skills {
            if let Some(skill) = all_skills.get(skill_name) {
                if seen_names.insert(skill.name.clone()) {
                    let mut hub = McpToolHub::from(skill.clone());
                    if let Some(s) = self.stats.get(&hub.name) {
                        hub.stats = s.clone();
                    }
                    tools.push(hub);
                }
            }
        }

        // 3. External MCP tools
        if let Ok(config) = self.get_config().await {
            let skill_set: std::collections::HashSet<&str> =
                agent_skills.iter().map(|s| s.as_str()).collect();

            for server_name in config.mcp_servers.keys() {
                if let Ok(client) = self.get_or_spawn_client(server_name).await {
                    let mut client_lock = client.lock().await;
                    if let Ok(mcp_tools) = client_lock.list_tools().await {
                        for t_val in mcp_tools {
                            if let (Some(name), Some(desc)) = (
                                t_val.get("name").and_then(|v| v.as_str()),
                                t_val.get("description").and_then(|v| v.as_str()),
                            ) {
                                let namespaced = format!("{}:{}", server_name, name);
                                let canonical_mcp = format!("mcp_{}_{}", server_name, name);

                                if (skill_set.contains(name)
                                    || skill_set.contains(namespaced.as_str())
                                    || skill_set.contains(canonical_mcp.as_str()))
                                    && seen_names.insert(name.to_string())
                                {
                                    let schema = t_val
                                        .get("inputSchema")
                                        .cloned()
                                        .unwrap_or(serde_json::json!({}));
                                    let stats =
                                        self.stats.get(name).map(|s| s.clone()).unwrap_or_default();
                                    tools.push(McpToolHub {
                                        name: name.to_string(),
                                        description: desc.to_string(),
                                        input_schema: schema,
                                        source: format!("mcp:{}", server_name),
                                        stats,
                                        category: "external".to_string(),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        tools
    }

    async fn tool_exists(
        &self,
        tool_name: &str,
        all_skills: &DashMap<String, SkillDefinition>,
    ) -> bool {
        if self.registry.lock().await.has(tool_name) {
            return true;
        }
        if all_skills.contains_key(tool_name) {
            return true;
        }
        self.resolve_external_mcp_tool(tool_name).await.is_some()
    }

    /// Evaluates the sovereign `PermissionPolicy` gate for a tool invocation.
    async fn check_permission_gate(
        &self,
        clean_name: &str,
        arguments: &serde_json::Value,
        ctx: &crate::agent::types::ToolContext,
    ) -> Result<(), AppError> {
        let mode = self.policy.get_mode(clean_name, &ctx.agent_id).await;
        match mode {
            PermissionMode::Deny => Err(AppError::Forbidden(format!(
                "Permission denied: Tool {} is explicitly blocked for agent {} by policy.",
                clean_name, ctx.agent_id
            ))),
            PermissionMode::Prompt => {
                let prompter_guard = self.prompter.read();
                if let Some(prompter) = prompter_guard.as_ref() {
                    let decision = prompter
                        .prompt_user(clean_name, &arguments.to_string())
                        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
                    if decision != PermissionMode::Allow {
                        return Err(AppError::Forbidden(
                            "User rejected tool execution".to_string(),
                        ));
                    }
                    Ok(())
                } else if ctx.execution_plane
                    == crate::agent::types::ExecutionPlane::EngineMaintenance
                    && matches!(
                        clean_name,
                        "parity_guard"
                            | "security_scan"
                            | "verify_ai_context"
                            | "verify_telemetry"
                            | "resource_check"
                            | "parse_errors"
                            | "mission_diagnose"
                            | "tool_loop_guard"
                            | "system_telemetry"
                            | "greeting_skill"
                            | "research_orchestrator"
                    )
                {
                    tracing::info!(
                        tool = %clean_name,
                        agent_id = %ctx.agent_id,
                        "🛡️ [MCP Headless] Auto-approving diagnostic tool under Plane 1 (EngineMaintenance)"
                    );
                    Ok(())
                } else {
                    tracing::error!(
                        tool = %clean_name,
                        "PermissionMode::Prompt configured but no prompter is registered"
                    );
                    Err(AppError::Forbidden(format!(
                        "Tool '{}' requires interactive approval, but no prompter is configured.",
                        clean_name
                    )))
                }
            }
            PermissionMode::Allow => Ok(()),
        }
    }

    pub async fn call_tool(
        &self,
        tool_name: &str,
        arguments: serde_json::Value,
        ctx: &crate::agent::types::ToolContext,
        all_skills: &DashMap<String, SkillDefinition>,
    ) -> Result<McpResult, AppError> {
        let start_time = std::time::Instant::now();

        // 1. Sanitize tool name
        let clean_name = tool_name.trim();
        if clean_name.is_empty() || clean_name.len() > 128 || clean_name.contains('\0') {
            return Err(AppError::BadRequest(format!(
                "Invalid tool name '{}'",
                tool_name
            )));
        }

        // 2. Fast-fail if tool is not registered across native, skill, or configured MCP servers
        if !self.tool_exists(clean_name, all_skills).await {
            return Err(AppError::NotFound(format!("Tool '{}' not found", tool_name)));
        }

        // 3. Permission policy gate (fail closed)
        self.check_permission_gate(clean_name, &arguments, ctx)
            .await?;

        let result = self
            .execute_tool_internal(clean_name, arguments, ctx, all_skills)
            .await;

        let latency = start_time.elapsed().as_millis() as u64;
        self.update_stats(clean_name, result.is_ok(), latency);
        self.emit_pulse(clean_name, result.is_ok(), latency);

        // SEC-04: Enforce maximum payload cap (2MB) with UTF-8 boundary safety and secret redaction
        match result {
            Ok(McpResult::Raw(ref raw)) => {
                let bounded_raw = truncate_mcp_reply(raw, MAX_MCP_REPLY_BYTES, "Tool response");
                Ok(McpResult::Raw(self.redactor.scrub(&bounded_raw)))
            }
            Ok(McpResult::SystemDelegate(k, mut v)) => {
                self.scrub_json_value(&mut v);
                Ok(McpResult::SystemDelegate(k, v))
            }
            Err(AppError::InfrastructureError {
                provider_id,
                detail,
                help_link,
            }) => Err(AppError::InfrastructureError {
                provider_id,
                detail: self.redactor.scrub(&detail),
                help_link,
            }),
            Err(e) => Err(e),
        }
    }

    pub(super) fn scrub_json_value(&self, val: &mut serde_json::Value) {
        match val {
            serde_json::Value::String(s) => *s = self.redactor.scrub(s),
            serde_json::Value::Array(arr) => {
                for item in arr {
                    self.scrub_json_value(item);
                }
            }
            serde_json::Value::Object(map) => {
                for (_k, v) in map.iter_mut() {
                    self.scrub_json_value(v);
                }
            }
            _ => {}
        }
    }

    async fn execute_tool_internal(
        &self,
        tool_name: &str,
        arguments: serde_json::Value,
        ctx: &crate::agent::types::ToolContext,
        all_skills: &DashMap<String, SkillDefinition>,
    ) -> Result<McpResult, AppError> {
        // 1. Check native registry
        let handler = {
            let registry = self.registry.lock().await;
            registry.get(tool_name)
        };

        if let Some(h) = handler {
            return h.execute(arguments, ctx).await;
        }

        // 2. Check skill registry (clone out of DashMap before .await)
        let skill_opt = all_skills.get(tool_name).map(|s| s.value().clone());
        if let Some(skill) = skill_opt {
            let output = self
                .execute_legacy_skill(&skill, arguments, ctx.workspace_root.clone())
                .await?;
            return Ok(McpResult::Raw(output));
        }

        // 3. Resolve External MCP Server tool
        if let Some((server_name, actual_tool_name)) =
            self.resolve_external_mcp_tool(tool_name).await
        {
            if !actual_tool_name
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
            {
                return Err(AppError::BadRequest(format!(
                    "Invalid external tool identifier '{}'",
                    actual_tool_name
                )));
            }

            if !Self::is_agent_authorized_for_external_tool(
                ctx,
                tool_name,
                &server_name,
                &actual_tool_name,
            ) {
                tracing::warn!(
                    "🚫 [McpHost] Access denied: tool '{}:{}' is not in granted skills for agent '{}'",
                    server_name,
                    actual_tool_name,
                    ctx.agent_id
                );
                return Err(AppError::Forbidden(format!(
                    "External tool '{}:{}' is not granted to agent '{}'",
                    server_name, actual_tool_name, ctx.agent_id
                )));
            }

            let client = self.get_or_spawn_client(&server_name).await?;
            let mut client_lock = client.lock().await;

            let call_result = client_lock.call_tool(&actual_tool_name, arguments).await;
            let result = match call_result {
                Ok(r) => r,
                Err(e) => {
                    self.clients.lock().await.remove(&server_name);
                    return Err(AppError::InfrastructureError {
                        provider_id: format!("mcp:{}", server_name),
                        detail: format!("Failed to execute external tool: {}", e),
                        help_link: None,
                    });
                }
            };

            return Self::format_external_mcp_result(&result);
        }

        Err(AppError::NotFound(format!("Tool '{}' not found", tool_name)))
    }

    fn update_stats(&self, tool_name: &str, is_success: bool, latency: u64) {
        let mut entry = self.stats.entry(tool_name.to_string()).or_default();
        entry.invocations = entry.invocations.saturating_add(1);
        if is_success {
            entry.success_count = entry.success_count.saturating_add(1);
        } else {
            entry.failure_count = entry.failure_count.saturating_add(1);
        }
        if entry.invocations == 1 {
            entry.avg_latency_ms = latency;
        } else {
            // Exponentially weighted moving average (alpha = 0.2)
            entry.avg_latency_ms = ((entry.avg_latency_ms * 4) + latency) / 5;
        }
    }

    fn emit_pulse(&self, tool_name: &str, is_success: bool, latency: u64) {
        let pulse = serde_json::json!({
            "type": "engine:mcp_pulse",
            "tool": tool_name,
            "status": if is_success { "success" } else { "error" },
            "latency": latency
        });
        let _ = self.event_tx.send(pulse);
    }
}

// Metadata: [mod]

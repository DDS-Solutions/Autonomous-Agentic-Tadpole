//! @docs ARCHITECTURE:Agent
//!
//! ### AI Assist Note
//! **Native Hydra-RS MCP Tool Handlers**: Houses the built-in Rust tool implementations
//! registered into `McpRegistry` at `McpHost` construction (`recruit_specialist`,
//! `get_current_time`, `list_file_symbols`, `get_symbol_body`, `run_integrity_check`,
//! `inspect_engine_health`, `get_blast_radius`).
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Path traversal rejection, symbol lookup miss, or 60s integrity check timeout.
//! - **Telemetry Link**: Search `[builtins]` or `[McpHost]` in tracing logs.

use super::registry::{McpRegistry, ToolHandler};
use super::{McpResult, McpToolHub, McpToolStats};
use crate::error::AppError;
use crate::utils::parser::SymbolExtractor;
use dashmap::DashMap;
use server_rs_macros::agent_tool;
use std::sync::Arc;

/// Registers all native Hydra-RS tools into the provided MCP registry.
pub(super) fn register_native_tools(
    registry: &mut McpRegistry,
    stats: Arc<DashMap<String, McpToolStats>>,
) {
    registry.register(Arc::new(RecruitSpecialistHandler));
    registry.register(Arc::new(ListFileSymbolsHandler));
    registry.register(Arc::new(GetSymbolBodyHandler));
    registry.register(Arc::new(RunIntegrityCheckHandler));
    registry.register(Arc::new(GetCurrentTimeHandler));
    registry.register(Arc::new(GetBlastRadiusHandler));
    registry.register(Arc::new(InspectEngineHealthHandler { stats }));
}

#[agent_tool]
pub async fn recruit_specialist(
    args: serde_json::Value,
    _ctx: &crate::agent::types::ToolContext,
) -> Result<McpResult, AppError> {
    Ok(McpResult::SystemDelegate(
        "recruit_specialist".to_string(),
        args,
    ))
}

#[agent_tool]
pub async fn get_current_time(
    _args: serde_json::Value,
    _ctx: &crate::agent::types::ToolContext,
) -> Result<McpResult, AppError> {
    let now = chrono::Local::now();
    Ok(McpResult::Raw(format!(
        "Current System Time: {}",
        now.format("%Y-%m-%d %H:%M:%S")
    )))
}

#[agent_tool]
pub async fn list_file_symbols(
    args: serde_json::Value,
    ctx: &crate::agent::types::ToolContext,
) -> Result<McpResult, AppError> {
    let path_str = args
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::BadRequest("Missing 'path'".to_string()))?;
    let full_path = crate::utils::security::validate_path(&ctx.workspace_root, path_str)
        .map_err(|e| AppError::Forbidden(e.to_string()))?;
    let content = tokio::fs::read_to_string(&full_path)
        .await
        .map_err(AppError::Io)?;
    let mut extractor = SymbolExtractor::new();
    let symbols = extractor.extract_symbols(&full_path, &content);
    let outline: Vec<String> = symbols
        .iter()
        .map(|s| format!("{} {} -> {}", s.kind, s.name, s.signature))
        .collect();
    Ok(McpResult::Raw(outline.join("\n")))
}

#[agent_tool]
pub async fn get_symbol_body(
    args: serde_json::Value,
    ctx: &crate::agent::types::ToolContext,
) -> Result<McpResult, AppError> {
    let path_str = args
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::BadRequest("Missing 'path'".to_string()))?;
    let symbol_name = args
        .get("symbol_name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::BadRequest("Missing 'symbol_name'".to_string()))?;
    let full_path = crate::utils::security::validate_path(&ctx.workspace_root, path_str)
        .map_err(|e| AppError::Forbidden(e.to_string()))?;

    let meta = tokio::fs::metadata(&full_path)
        .await
        .map_err(AppError::Io)?;
    if meta.len() > 5_000_000 {
        return Err(AppError::BadRequest(
            "File exceeds 5MB size limit".to_string(),
        ));
    }

    let content = tokio::fs::read_to_string(&full_path)
        .await
        .map_err(AppError::Io)?;
    let mut extractor = SymbolExtractor::new();
    let symbols = extractor.extract_symbols(&full_path, &content);
    if let Some(symbol) = symbols.into_iter().find(|s| s.name == symbol_name) {
        Ok(McpResult::Raw(symbol.body))
    } else {
        Err(AppError::NotFound(format!(
            "Symbol '{}' not found",
            symbol_name
        )))
    }
}

#[agent_tool]
pub async fn run_integrity_check(
    _args: serde_json::Value,
    ctx: &crate::agent::types::ToolContext,
) -> Result<McpResult, AppError> {
    let script_path =
        crate::utils::security::validate_path(&ctx.workspace_root, "execution/self_audit_tool.py")
            .map_err(|e| AppError::Forbidden(e.to_string()))?;

    let mut cmd = tokio::process::Command::new("python");
    cmd.arg(script_path.as_path());
    cmd.current_dir(&ctx.workspace_root);
    cmd.kill_on_drop(true);

    let output = tokio::time::timeout(std::time::Duration::from_secs(60), cmd.output())
        .await
        .map_err(|_| AppError::InfrastructureError {
            provider_id: "integrity_check".to_string(),
            detail: "Integrity check timed out after 60s".to_string(),
            help_link: None,
        })?
        .map_err(AppError::Io)?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();

    if output.status.success() {
        Ok(McpResult::Raw(stdout))
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        Err(AppError::InfrastructureError {
            provider_id: "integrity_check".to_string(),
            detail: format!("Integrity Audit Failed: {}\n{}", stdout, stderr),
            help_link: None,
        })
    }
}

pub struct InspectEngineHealthHandler {
    pub stats: Arc<DashMap<String, McpToolStats>>,
}

#[async_trait::async_trait]
impl ToolHandler for InspectEngineHealthHandler {
    async fn execute(
        &self,
        _args: serde_json::Value,
        _ctx: &crate::agent::types::ToolContext,
    ) -> Result<McpResult, AppError> {
        let stats_vec: Vec<serde_json::Value> = self
            .stats
            .iter()
            .map(|kv| {
                let name = kv.key();
                let stats = kv.value();
                serde_json::json!({
                    "tool": name,
                    "invocations": stats.invocations,
                    "success_rate": if stats.invocations > 0 { stats.success_count as f64 / stats.invocations as f64 } else { 0.0 },
                    "avg_latency_ms": stats.avg_latency_ms
                })
            })
            .collect();

        Ok(McpResult::Raw(
            serde_json::to_string_pretty(&stats_vec)
                .map_err(|e| AppError::InternalServerError(e.to_string()))?,
        ))
    }

    fn metadata(&self) -> McpToolHub {
        McpToolHub {
            name: "inspect_engine_health".to_string(),
            description: "Retrieves real-time execution statistics for all registered MCP tools."
                .to_string(),
            input_schema: serde_json::json!({}),
            source: "native".to_string(),
            stats: McpToolStats::default(),
            category: "introspection".to_string(),
        }
    }
}

/// Calculates the "Blast Radius" of a symbol.
/// Identifies all symbols that directly or indirectly depend on the target.
#[agent_tool]
pub async fn get_blast_radius(
    args: serde_json::Value,
    ctx: &crate::agent::types::ToolContext,
) -> Result<McpResult, AppError> {
    let symbol_name = args
        .get("symbol_name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::BadRequest("Missing 'symbol_name'".to_string()))?;
    let path = args
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::BadRequest("Missing 'path'".to_string()))?;

    let graph_swap = ctx.state.resources.get_symbol_graph().await?;
    let graph = graph_swap.load();
    let affected = graph.calculate_blast_radius(symbol_name, path);

    let serialized = serde_json::to_string_pretty(&affected)
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(McpResult::Raw(serialized))
}

// Metadata: [builtins]

//! @docs ARCHITECTURE:Agent
//!
//! ### AI Assist Note
//! **MCP Configuration, Environment Interpolation & Reply Bounds**: Owns `McpConfig` /
//! `McpServerConfig` schemas, UTF-8 safe 2 MiB reply truncation (`truncate_mcp_reply`),
//! and `${VAR:-default}` environment placeholder resolution (`resolve_mcp_env_with`).
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Missing required environment variable, malformed `mcp_config.json`, or multi-byte UTF-8 boundary panic if truncated naively.
//! - **Telemetry Link**: Search `[env]` or `[McpHost]` in tracing logs.

use crate::error::AppError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const MAX_MCP_REPLY_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct McpConfig {
    #[serde(rename = "mcpServers", default)]
    pub mcp_servers: HashMap<String, McpServerConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct McpServerConfig {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: Option<HashMap<String, String>>,
}

/// Finds the largest valid UTF-8 character boundary `<= max_bytes`.
/// Prevents panics when truncating strings containing multi-byte UTF-8 sequences.
pub fn floor_utf8_char_boundary(s: &str, max_bytes: usize) -> usize {
    if max_bytes >= s.len() {
        return s.len();
    }
    let mut idx = max_bytes;
    while idx > 0 && !s.is_char_boundary(idx) {
        idx -= 1;
    }
    idx
}

/// Safely truncates an MCP/skill reply string at a valid UTF-8 character boundary.
pub fn truncate_mcp_reply(raw: &str, max_bytes: usize, label: &str) -> String {
    if raw.len() <= max_bytes {
        return raw.to_string();
    }
    let safe_end = floor_utf8_char_boundary(raw, max_bytes);
    format!(
        "{}\n\n[TRUNCATED: {} exceeded 2MB limit]",
        &raw[..safe_end],
        label
    )
}

/// Resolves documented `${NAME}` or `${NAME:-default}` placeholders in MCP environment values
/// using a caller-supplied lookup function (enabling race-free deterministic unit testing).
pub fn resolve_mcp_env_with<F>(
    env: &HashMap<String, String>,
    mut lookup: F,
) -> Result<HashMap<String, String>, AppError>
where
    F: FnMut(&str) -> Option<String>,
{
    let mut resolved_map = HashMap::new();

    for (key, value) in env {
        let mut resolved = String::new();
        let mut remainder = value.as_str();

        while let Some(start) = remainder.find("${") {
            resolved.push_str(&remainder[..start]);
            let placeholder_rest = &remainder[start + 2..];
            let Some(end) = placeholder_rest.find('}') else {
                resolved.push_str(&remainder[start..]);
                remainder = "";
                break;
            };

            let inner = &placeholder_rest[..end];
            if inner.is_empty() {
                resolved.push_str("${}");
                remainder = &placeholder_rest[end + 1..];
                continue;
            }

            // Support ${VAR:-default} syntax
            let (var_name, default_val) = if let Some((var, def)) = inner.split_once(":-") {
                (var, Some(def))
            } else {
                (inner, None)
            };

            if !var_name.is_empty()
                && var_name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_')
            {
                match lookup(var_name) {
                    Some(val) => resolved.push_str(&val),
                    None => {
                        if let Some(def) = default_val {
                            resolved.push_str(def);
                        } else {
                            return Err(AppError::BadRequest(format!(
                                "Missing required environment variable '{}' for MCP config key '{}'",
                                var_name, key
                            )));
                        }
                    }
                }
                remainder = &placeholder_rest[end + 1..];
            } else {
                resolved.push_str("${");
                remainder = placeholder_rest;
            }
        }

        resolved.push_str(remainder);
        resolved_map.insert(key.clone(), resolved);
    }

    Ok(resolved_map)
}

/// Resolves documented `${NAME}` or `${NAME:-default}` placeholders in MCP environment values.
/// Returns an explicit configuration error if a required placeholder (without a default) is not set.
pub fn resolve_mcp_env(env: &HashMap<String, String>) -> Result<HashMap<String, String>, AppError> {
    resolve_mcp_env_with(env, |var_name| std::env::var(var_name).ok())
}

// Metadata: [env]

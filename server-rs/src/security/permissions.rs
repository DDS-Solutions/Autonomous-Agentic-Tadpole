//! Sovereign Permission System - Granular Tool Authorization
//!
//! Provides a strict authorization layer ensuring the user retains final
//! control over destructive, sensitive, or high-cost tool executions.
//!
//! @docs ARCHITECTURE:SecurityModel
//!
//! ### AI Assist Note
//! **Sovereign Permission System**: Orchestrates the granular
//! **Tool Authorization** layer for the Tadpole OS engine. Enforces
//! the **Sovereign Safety** principle: any tool not explicitly
//! **Whitelisted** (`Allow`) or **Guardrailed** (`Prompt`) defaults
//! to a manual user approval cycle. AI agents must check
//! `PermissionMode` before attempting high-risk or high-cost
//! executions (filesystem writes, external network access) to
//! ensure the user retains final state control (PERM-01).
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Permission denial for unrecognized tools,
//!   UI-blocking during waiting-for-approval states, or
//!   misconfiguration of the internal tool whitelist.
//! - **Trace Scope**: `server-rs::security::permissions`

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Copy)]
pub enum PermissionMode {
    /// Tool is always allowed (e.g., read-only safe commands).
    Allow,
    /// Tool is always denied (e.g., restricted system access).
    Deny,
    /// Tool execution is paused until the user provides explicit approval.
    Prompt,
}

impl std::str::FromStr for PermissionMode {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "allow" => Ok(PermissionMode::Allow),
            "deny" => Ok(PermissionMode::Deny),
            "prompt" => Ok(PermissionMode::Prompt),
            _ => Err(anyhow::anyhow!("Invalid permission mode: {}", s)),
        }
    }
}

impl std::fmt::Display for PermissionMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PermissionMode::Allow => write!(f, "Allow"),
            PermissionMode::Deny => write!(f, "Deny"),
            PermissionMode::Prompt => write!(f, "Prompt"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct PermissionDecision {
    pub mode: PermissionMode,
    pub reason: Option<String>,
}

pub trait PermissionPrompter: Send + Sync {
    /// Prompts the user for a decision on a pending tool execution.
    /// This may be implemented via a Tauri modal or a CLI prompt.
    fn prompt_user(&self, tool_name: &str, arguments: &str) -> anyhow::Result<PermissionMode>;
}

pub struct PermissionPolicy {
    pool: sqlx::SqlitePool,
    cache: dashmap::DashMap<String, PermissionMode>,
}

impl PermissionPolicy {
    pub fn new(pool: sqlx::SqlitePool) -> Self {
        Self {
            pool,
            cache: dashmap::DashMap::new(),
        }
    }

    /// Reloads the policy cache from the database.
    pub async fn refresh_cache(&self) -> anyhow::Result<()> {
        let rows: Vec<(String, Option<String>, String)> =
            sqlx::query_as("SELECT tool_name, agent_id, mode FROM permission_policies")
                .fetch_all(&self.pool)
                .await?;

        self.cache.clear();
        for (name, agent_id, mode_str) in rows {
            if let Ok(mode) = mode_str.parse::<PermissionMode>() {
                let key = agent_id.map(|id| format!("{}:{}", id, name)).unwrap_or(name);
                self.cache.insert(key, mode);
            }
        }

        tracing::info!(
            "✅ [Security] Permission cache refreshed ({} entries).",
            self.cache.len()
        );
        Ok(())
    }

    /// Determines the permission mode for a tool, considering agent-specific overrides.
    /// Checks the cache first, falling back to database query if missing.
    #[tracing::instrument(skip(self), fields(tool = tool_name, agent = agent_id), name = "security::get_permission_mode")]
    pub async fn get_mode(&self, tool_name: &str, agent_id: &str) -> PermissionMode {
        // 1. Try Cache (Agent-Specific)
        let cache_key = format!("{}:{}", agent_id, tool_name);
        if let Some(mode) = self.cache.get(&cache_key) {
            return *mode;
        }

        // 2. Try Cache (Global)
        if let Some(mode) = self.cache.get(tool_name) {
            return *mode;
        }

        // 3. Try DB (Agent-Specific)
        let res: Result<(String,), sqlx::Error> =
            sqlx::query_as("SELECT mode FROM permission_policies WHERE tool_name = ? AND agent_id = ?")
                .bind(tool_name)
                .bind(agent_id)
                .fetch_one(&self.pool)
                .await;

        if let Ok((mode_str,)) = res {
            if let Ok(mode) = mode_str.parse::<PermissionMode>() {
                self.cache.insert(cache_key, mode);
                return mode;
            }
        }

        // 4. Try DB (Global)
        let res: Result<(String,), sqlx::Error> =
            sqlx::query_as("SELECT mode FROM permission_policies WHERE tool_name = ? AND agent_id IS NULL")
                .bind(tool_name)
                .fetch_one(&self.pool)
                .await;

        if let Ok((mode_str,)) = res {
            if let Ok(mode) = mode_str.parse::<PermissionMode>() {
                self.cache.insert(tool_name.to_string(), mode);
                return mode;
            }
        }

        // Default to prompt for unknown tools (Sovereign Safety First)
        PermissionMode::Prompt
    }

    /// Manually sets the permission mode for a tool (used for tests and admin updates).
    #[allow(dead_code)]
    pub async fn set_mode(&self, tool_name: &str, mode: PermissionMode) {
        let _ = sqlx::query("INSERT INTO permission_policies (tool_name, mode) VALUES (?, ?) ON CONFLICT(tool_name) DO UPDATE SET mode = excluded.mode")
            .bind(tool_name)
            .bind(mode.to_string())
            .execute(&self.pool)
            .await;
        self.cache.insert(tool_name.to_string(), mode);
    }
}

// Metadata: [permissions]

// Metadata: [permissions]

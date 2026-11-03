//! @docs ARCHITECTURE:Networking
//!
//! ### AI Assist Note
//! **Templates Data Models**: Request and response payloads, install receipts,
//! and inventory summaries for swarm template installation and lifecycle management.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Serialization or deserialization schema mismatch.
//! - **Telemetry Link**: Search `[templates_models]` in tracing logs.
//! - **Trace Scope**: `server-rs::routes::templates::models`

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
pub struct InstallTemplateRequest {
    pub repository_url: String,
    pub path: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InstallTemplateResponse {
    pub status: String,
    pub message: String,
    pub template_id: String,
    pub agents_installed: usize,
    pub agents_skipped: usize,
    pub workflows_copied: usize,
    pub skills_copied: usize,
    pub mcp_merged: bool,
    pub knowledge_copied: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InstallReceipt {
    pub template_id: String,
    pub template_path: String,
    pub installed_at: String,
    pub agents: Vec<String>,
    pub agent_ids: Vec<String>,
    pub workflows: Vec<String>,
    pub skills: Vec<String>,
    pub knowledge: Vec<String>,
    pub mcp_servers: Vec<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub file_digests: HashMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InstalledTemplateSummary {
    pub id: String,
    pub path: String,
    pub name: Option<String>,
    pub installed_at: Option<String>,
    pub agents_count: usize,
    pub workflows_count: usize,
    pub skills_count: usize,
    pub knowledge_count: usize,
    pub mcp_servers_count: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UninstallTemplateResponse {
    pub status: String,
    pub message: String,
    pub uninstalled_id: String,
}

// Metadata: [templates_models]


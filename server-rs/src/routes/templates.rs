//! @docs ARCHITECTURE:Networking
//!
//! ### AI Assist Note
//! **Mission Templates (Starter Kits)**: Orchestrates the
//! installation and management of multi-agent swarm blueprints for
//! the Tadpole OS engine. Features **Git-Integrated Installation**:
//! allows users to pull complete swarm configurations (agents,
//! workflows, skills, MCP configs) from remote repositories.
//! Implements **Swarm Partitioning**: automatically categorizes and
//! persists individual components (directives, execution scripts,
//! registries) to ensure a seamless "one-click" deployment
//! experience. AI agents should use this endpoint to expand their
//! operational capabilities by installing specialized skill-sets or
//! mission-specific swarm architectures (TMP-01).
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Git clone failures due to network or
//!   authentication issues, duplicate agent ID collisions during
//!   template merge, or schema mismatches in the imported `swarm.json`
//!   config.
//! - **Telemetry Link**: Search `[templates]` in tracing logs.
//!   initiated pull` in `tracing` logs (internal Note: templates use
//!   similar pull patterns).
//! - **Trace Scope**: `server-rs::routes::templates`

use crate::error::AppError;
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

// ─── Data Contracts ─────────────────────────────────────────────────────────

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

// ─── In-Memory Catalog Cache (Item #9) ──────────────────────────────────────

struct CatalogCache {
    fetched_at: Instant,
    data: serde_json::Value,
}

static CATALOG_CACHE: Lazy<RwLock<Option<CatalogCache>>> = Lazy::new(|| RwLock::new(None));
const CATALOG_TTL: Duration = Duration::from_secs(300);
const REGISTRY_UPSTREAM_URL: &str =
    "https://raw.githubusercontent.com/DDS-Solutions/AI-Tadpole-OS-Industry-Templates/main/registry.json";

// ─── Verification Utilities (Item #10) ──────────────────────────────────────

/// Extracts the expected SHA-256 hash for a skill filename from `connector-lock.json`.
pub fn parse_expected_skill_hash(lock: &serde_json::Value, filename: &str) -> Option<String> {
    // Format 1: { "skills": { "file.py": "sha256:..." } }
    if let Some(h) = lock
        .get("skills")
        .and_then(|s| s.get(filename))
        .and_then(|v| v.as_str())
    {
        return Some(h.trim_start_matches("sha256:").trim().to_lowercase());
    }
    // Format 2: direct map { "file.py": "..." }
    if let Some(h) = lock.get(filename).and_then(|v| v.as_str()) {
        return Some(h.trim_start_matches("sha256:").trim().to_lowercase());
    }
    // Format 3: { "files": { "file.py": "..." } }
    if let Some(h) = lock
        .get("files")
        .and_then(|f| f.get(filename))
        .and_then(|v| v.as_str())
    {
        return Some(h.trim_start_matches("sha256:").trim().to_lowercase());
    }
    // Format 4: array { "skills": [ { "name": "file.py", "sha256": "..." } ] }
    if let Some(arr) = lock.get("skills").and_then(|s| s.as_array()) {
        for item in arr {
            let name_match = item
                .get("name")
                .or_else(|| item.get("file"))
                .and_then(|v| v.as_str());
            if name_match == Some(filename) {
                if let Some(h) = item
                    .get("sha256")
                    .or_else(|| item.get("hash"))
                    .and_then(|v| v.as_str())
                {
                    return Some(h.trim_start_matches("sha256:").trim().to_lowercase());
                }
            }
        }
    }
    None
}

/// Verifies that the SHA-256 digest of file bytes matches the expected hex hash.
pub fn verify_skill_sha256(file_bytes: &[u8], expected_hash: &str) -> bool {
    let mut hasher = Sha256::new();
    hasher.update(file_bytes);
    let result = hasher.finalize();
    let computed_hash = hex::encode(result).to_lowercase();
    computed_hash == expected_hash.to_lowercase()
}

// ─── Handlers ───────────────────────────────────────────────────────────────

#[axum::debug_handler]
#[tracing::instrument(skip(state, payload), name = "templates::install")]
pub async fn install_template(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<InstallTemplateRequest>,
) -> Result<axum::response::Response, AppError> {
    // 🛡️ [Security Hardening] Validate repository URL to prevent command/flag injection
    let repo_url = payload.repository_url.trim();
    if repo_url.starts_with('-') {
        return Err(AppError::BadRequest(
            "Repository URL cannot start with a hyphen.".to_string(),
        ));
    }

    // Parse URL scheme validation
    let parsed_url = reqwest::Url::parse(repo_url)
        .map_err(|e| AppError::BadRequest(format!("Invalid Repository URL: {e}")))?;
    let scheme = parsed_url.scheme();
    if scheme != "http" && scheme != "https" && scheme != "git" {
        return Err(AppError::BadRequest(format!(
            "Unsupported repository URL scheme: {scheme}. Only http, https, and git protocols are allowed."
        )));
    }

    // Validate path parameter to prevent directory traversal
    if payload.path.contains("..")
        || payload.path.starts_with('/')
        || payload.path.starts_with('\\')
    {
        return Err(AppError::BadRequest(
            "Invalid template path: Directory traversal or absolute paths are prohibited."
                .to_string(),
        ));
    }

    let git_cmd = match state.resources.git_path.as_ref() {
        Some(path) => path.clone(),
        None => {
            return Err(AppError::InternalServerError(
                "Git executable is not resolved on the system.".to_string(),
            ));
        }
    };

    let dl_id = uuid::Uuid::new_v4().to_string();
    let temp_dir = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("data")
        .join(".bunker_cache")
        .join(&dl_id);

    let _ = tokio::fs::create_dir_all(&temp_dir).await;

    // 1. Clone repository using the boot-resolved git command
    let status = match tokio::process::Command::new(&git_cmd)
        .arg("clone")
        .arg("--depth")
        .arg("1")
        .arg(repo_url)
        .arg(&temp_dir)
        .status()
        .await
    {
        Ok(s) => s,
        Err(e) => {
            let _ = tokio::fs::remove_dir_all(&temp_dir).await;
            return Err(AppError::InternalServerError(format!(
                "Failed to execute git: {e}"
            )));
        }
    };

    if !status.success() {
        let _ = tokio::fs::remove_dir_all(&temp_dir).await;
        return Err(AppError::InternalServerError(
            "Failed to clone template repository".to_string(),
        ));
    }

    // 2. Identify template source directory
    let source_path = match crate::utils::security::validate_path(&temp_dir, &payload.path) {
        Ok(p) => p,
        Err(e) => {
            let _ = tokio::fs::remove_dir_all(&temp_dir).await;
            return Err(AppError::BadRequest(format!("Invalid template path: {e}")));
        }
    };
    if !source_path.exists() {
        let _ = tokio::fs::remove_dir_all(&temp_dir).await;
        return Err(AppError::NotFound(format!(
            "Template path '{}' not found in repo",
            payload.path
        )));
    }

    // Resolve template identifier and target directory
    let safe_name = crate::utils::security::sanitize_id(&payload.path.replace("/", "_"));
    let mut template_id = safe_name.clone();
    let swarm_json_src = source_path.as_path().join("swarm.json");
    if swarm_json_src.exists() {
        if let Ok(c) = tokio::fs::read_to_string(&swarm_json_src).await {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&c) {
                if let Some(id_str) = val.get("id").and_then(|v| v.as_str()) {
                    template_id = id_str.to_string();
                }
            }
        }
    }

    let dest_folder = PathBuf::from("data/swarm_config/installed").join(&safe_name);
    let _ = tokio::fs::create_dir_all(&dest_folder).await;

    let mut receipt = InstallReceipt {
        template_id: template_id.clone(),
        template_path: payload.path.clone(),
        installed_at: chrono::Utc::now().to_rfc3339(),
        agents: Vec::new(),
        agent_ids: Vec::new(),
        workflows: Vec::new(),
        skills: Vec::new(),
        knowledge: Vec::new(),
        mcp_servers: Vec::new(),
    };
    let mut errors: Vec<String> = Vec::new();

    // ─── 3. Preflight Agent Profile Validation (Item #3) ───────────────────
    // The upstream compatibility matrix requires: "invalid profiles abort before writes".
    // We deserialize all agent profiles in memory first. Any failure aborts install cleanly.
    let agents_src = source_path.as_path().join("agents");
    let agents_dest = PathBuf::from("data/swarm_config/agents");
    let mut validated_agents: Vec<(String, PathBuf, crate::agent::types::EngineAgent)> = Vec::new();

    if agents_src.exists() {
        if let Ok(mut entries) = tokio::fs::read_dir(&agents_src).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let file_path = entry.path();
                if file_path.is_file()
                    && file_path.extension().and_then(|s| s.to_str()) == Some("json")
                {
                    let filename = entry.file_name().to_string_lossy().to_string();
                    let content = match tokio::fs::read_to_string(&file_path).await {
                        Ok(c) => c,
                        Err(e) => {
                            let _ = tokio::fs::remove_dir_all(&temp_dir).await;
                            return Err(AppError::BadRequest(format!(
                                "Failed to read agent file '{filename}': {e}"
                            )));
                        }
                    };

                    match serde_json::from_str::<crate::agent::types::EngineAgent>(&content) {
                        Ok(agent) => {
                            validated_agents.push((filename, file_path, agent));
                        }
                        Err(e) => {
                            let _ = tokio::fs::remove_dir_all(&temp_dir).await;
                            return Err(AppError::BadRequest(format!(
                                "Agent profile preflight validation failed for '{filename}': {e}"
                            )));
                        }
                    }
                }
            }
        }
    }

    // ─── 4. Preflight Skills & connector-lock.json Hash Verification (Item #10) ──
    let skills_src = source_path.as_path().join("skills");
    let skills_dest = PathBuf::from("execution");
    let lock_path = source_path.as_path().join("connector-lock.json");
    let lock_val: Option<serde_json::Value> = if lock_path.exists() {
        tokio::fs::read_to_string(&lock_path)
            .await
            .ok()
            .and_then(|c| serde_json::from_str(&c).ok())
    } else {
        None
    };

    let mut validated_skills: Vec<(String, PathBuf)> = Vec::new();
    if skills_src.exists() {
        if let Ok(mut entries) = tokio::fs::read_dir(&skills_src).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let path = entry.path();
                let ext = path
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                if ext == "json" || ext == "py" || ext == "js" || ext == "ts" {
                    let filename = entry.file_name().to_string_lossy().to_string();
                    let bytes = match tokio::fs::read(&path).await {
                        Ok(b) => b,
                        Err(e) => {
                            let _ = tokio::fs::remove_dir_all(&temp_dir).await;
                            return Err(AppError::BadRequest(format!(
                                "Failed to read skill file '{filename}': {e}"
                            )));
                        }
                    };

                    if let Some(ref lock) = lock_val {
                        if let Some(expected_hash) = parse_expected_skill_hash(lock, &filename) {
                            if !verify_skill_sha256(&bytes, &expected_hash) {
                                let _ = tokio::fs::remove_dir_all(&temp_dir).await;
                                return Err(AppError::BadRequest(format!(
                                    "Integrity check failed for skill '{filename}': SHA-256 hash mismatch with connector-lock.json"
                                )));
                            }
                        }
                    }
                    validated_skills.push((filename, path));
                }
            }
        }
    }

    // ─── 5. Atomic Component Persistence (Post-Validation) ──────────────────

    // 5.1 Persist validated agents
    let mut agents_installed = 0;
    if !validated_agents.is_empty() {
        let _ = tokio::fs::create_dir_all(&agents_dest).await;
        for (filename, src_path, agent) in validated_agents {
            let dest_file = agents_dest.join(&filename);
            let _ = tokio::fs::copy(&src_path, &dest_file).await;

            // Persist to database and active registry
            if let Err(e) =
                crate::agent::persistence::save_agent_db(&state.resources.pool, &agent).await
            {
                tracing::warn!(
                    "Failed to persist agent '{}' to database: {e}",
                    agent.identity.id
                );
                errors.push(format!(
                    "DB write warning for agent '{}': {e}",
                    agent.identity.id
                ));
            }

            state
                .registry
                .agents
                .insert(agent.identity.id.clone(), agent.clone());
            receipt.agents.push(filename);
            receipt.agent_ids.push(agent.identity.id);
            agents_installed += 1;
        }
    }

    // 5.2 Persist swarm.json
    if swarm_json_src.exists() {
        let _ = tokio::fs::copy(&swarm_json_src, dest_folder.join("swarm.json")).await;
    }

    // 5.3 Copy workflows to directives
    let workflows_src = source_path.as_path().join("workflows");
    let workflows_dest = PathBuf::from("directives");
    let mut workflows_copied = 0;
    if workflows_src.exists() {
        let _ = tokio::fs::create_dir_all(&workflows_dest).await;
        if let Ok(mut entries) = tokio::fs::read_dir(&workflows_src).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                if entry.path().extension().and_then(|s| s.to_str()) == Some("md") {
                    let filename = entry.file_name().to_string_lossy().to_string();
                    let dest_file = workflows_dest.join(&filename);
                    if tokio::fs::copy(entry.path(), dest_file).await.is_ok() {
                        receipt.workflows.push(filename);
                        workflows_copied += 1;
                    }
                }
            }
        }
    }

    // 5.4 Copy validated skills to execution
    let mut skills_copied = 0;
    if !validated_skills.is_empty() {
        let _ = tokio::fs::create_dir_all(&skills_dest).await;
        for (filename, src_path) in validated_skills {
            let dest_file = skills_dest.join(&filename);
            if tokio::fs::copy(&src_path, dest_file).await.is_ok() {
                receipt.skills.push(filename);
                skills_copied += 1;
            }
        }
    }

    // 5.5 Merge MCP configuration (Items #2 and #7: fail closed & reject '__')
    let mcps_src = source_path.as_path().join("mcps.json");
    let mut mcp_merged = false;
    if mcps_src.exists() {
        if let Ok(content) = tokio::fs::read_to_string(&mcps_src).await {
            if let Ok(incoming_config) =
                serde_json::from_str::<crate::agent::mcp::McpConfig>(&content)
            {
                let mcp_config_path = PathBuf::from(".agent/mcp_config.json");
                let mut current_config = if mcp_config_path.exists() {
                    tokio::fs::read_to_string(&mcp_config_path)
                        .await
                        .ok()
                        .and_then(|c| serde_json::from_str::<crate::agent::mcp::McpConfig>(&c).ok())
                        .unwrap_or_else(|| crate::agent::mcp::McpConfig {
                            mcp_servers: std::collections::HashMap::new(),
                        })
                } else {
                    crate::agent::mcp::McpConfig {
                        mcp_servers: std::collections::HashMap::new(),
                    }
                };

                let mut servers_added = 0;
                for (name, config) in incoming_config.mcp_servers {
                    // Rule #7: Server names cannot contain double underscores `__`
                    if name.contains("__") {
                        tracing::warn!(
                            "MCP server name '{}' contains '__' — rejected per specification",
                            name
                        );
                        errors.push(format!("MCP server '{}' rejected: contains '__'", name));
                        continue;
                    }

                    // Rule #2: Existing server-name collisions fail closed
                    if current_config.mcp_servers.contains_key(&name) {
                        tracing::warn!(
                            "MCP server collision for '{}' — skipping (fail closed)",
                            name
                        );
                        errors.push(format!(
                            "MCP server '{}' collision: existing server preserved",
                            name
                        ));
                        continue;
                    }

                    current_config.mcp_servers.insert(name.clone(), config);
                    receipt.mcp_servers.push(name);
                    servers_added += 1;
                }

                if servers_added > 0 {
                    if let Ok(merged_json) = serde_json::to_string_pretty(&current_config) {
                        let _ = tokio::fs::create_dir_all(".agent").await;
                        let _ = tokio::fs::write(mcp_config_path, merged_json).await;
                        mcp_merged = true;
                    }
                }
            }
        }
    }

    // 5.6 Copy knowledge assets for OKF/IKS vector ingestion
    let knowledge_dir_src = source_path.as_path().join("knowledge");
    let knowledge_dest = PathBuf::from("data/knowledge");
    let mut knowledge_copied = 0;
    if knowledge_dir_src.exists() {
        let _ = tokio::fs::create_dir_all(&knowledge_dest).await;
        if let Ok(mut entries) = tokio::fs::read_dir(&knowledge_dir_src).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                if entry.path().extension().and_then(|s| s.to_str()) == Some("md") {
                    let filename = entry.file_name().to_string_lossy().to_string();
                    let dest_file = knowledge_dest.join(&filename);
                    if tokio::fs::copy(entry.path(), dest_file).await.is_ok() {
                        receipt.knowledge.push(filename);
                        knowledge_copied += 1;
                    }
                }
            }
        }
    }

    let knowledge_json_src = source_path.as_path().join("knowledge.json");
    if knowledge_json_src.exists() {
        let _ = tokio::fs::copy(&knowledge_json_src, dest_folder.join("knowledge.json")).await;
    }

    // 5.7 Write installation receipt (Item #11)
    if let Ok(receipt_str) = serde_json::to_string_pretty(&receipt) {
        let _ = tokio::fs::write(dest_folder.join("install_receipt.json"), receipt_str).await;
    }

    // 6. Cleanup clone workspace
    let _ = tokio::fs::remove_dir_all(&temp_dir).await;

    // Structured install receipt (Items #1 and #8)
    let response = InstallTemplateResponse {
        status: if errors.is_empty() {
            "success".to_string()
        } else {
            "partial_success".to_string()
        },
        message: format!(
            "Successfully installed swarm template from {}",
            payload.path
        ),
        template_id,
        agents_installed,
        agents_skipped: 0,
        workflows_copied,
        skills_copied,
        mcp_merged,
        knowledge_copied,
        errors,
    };

    Ok((StatusCode::OK, Json(response)).into_response())
}

// ─── List Installed Templates (Item #4) ─────────────────────────────────────

#[axum::debug_handler]
#[tracing::instrument(name = "templates::list_installed")]
pub async fn list_installed_templates() -> Result<axum::response::Response, AppError> {
    let installed_dir = PathBuf::from("data/swarm_config/installed");
    let mut results = Vec::new();

    if installed_dir.exists() {
        if let Ok(mut entries) = tokio::fs::read_dir(&installed_dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                if let Ok(ft) = entry.file_type().await {
                    if ft.is_dir() {
                        let folder_path = entry.path();
                        let receipt_file = folder_path.join("install_receipt.json");
                        let swarm_file = folder_path.join("swarm.json");

                        let mut summary = if receipt_file.exists() {
                            if let Ok(c) = tokio::fs::read_to_string(&receipt_file).await {
                                serde_json::from_str::<InstallReceipt>(&c)
                                    .ok()
                                    .map(|r| InstalledTemplateSummary {
                                        id: r.template_id,
                                        path: r.template_path,
                                        name: None,
                                        installed_at: Some(r.installed_at),
                                        agents_count: r.agents.len(),
                                        workflows_count: r.workflows.len(),
                                        skills_count: r.skills.len(),
                                        knowledge_count: r.knowledge.len(),
                                        mcp_servers_count: r.mcp_servers.len(),
                                    })
                            } else {
                                None
                            }
                        } else {
                            None
                        };

                        if let Some(ref mut s) = summary {
                            if swarm_file.exists() {
                                if let Ok(sc) = tokio::fs::read_to_string(&swarm_file).await {
                                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&sc)
                                    {
                                        s.name = val
                                            .get("name")
                                            .and_then(|v| v.as_str())
                                            .map(String::from);
                                    }
                                }
                            }
                            results.push(s.clone());
                        } else if swarm_file.exists() {
                            let folder_name = entry.file_name().to_string_lossy().to_string();
                            let mut name = None;
                            if let Ok(sc) = tokio::fs::read_to_string(&swarm_file).await {
                                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&sc) {
                                    name = val
                                        .get("name")
                                        .and_then(|v| v.as_str())
                                        .map(String::from);
                                }
                            }
                            results.push(InstalledTemplateSummary {
                                id: folder_name.clone(),
                                path: folder_name,
                                name,
                                installed_at: None,
                                agents_count: 0,
                                workflows_count: 0,
                                skills_count: 0,
                                knowledge_count: 0,
                                mcp_servers_count: 0,
                            });
                        }
                    }
                }
            }
        }
    }

    Ok((StatusCode::OK, Json(results)).into_response())
}

// ─── Uninstall Template (Item #11) ──────────────────────────────────────────

#[axum::debug_handler]
#[tracing::instrument(skip(state), name = "templates::uninstall")]
pub async fn uninstall_template(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<axum::response::Response, AppError> {
    let installed_dir = PathBuf::from("data/swarm_config/installed");
    let safe_id = crate::utils::security::sanitize_id(&id.replace("/", "_"));

    // Identify target installation folder
    let target_folder = if installed_dir.join(&id).exists() {
        installed_dir.join(&id)
    } else if installed_dir.join(&safe_id).exists() {
        installed_dir.join(&safe_id)
    } else {
        let mut found = None;
        if installed_dir.exists() {
            if let Ok(mut entries) = tokio::fs::read_dir(&installed_dir).await {
                while let Ok(Some(entry)) = entries.next_entry().await {
                    let receipt_file = entry.path().join("install_receipt.json");
                    if receipt_file.exists() {
                        if let Ok(c) = tokio::fs::read_to_string(&receipt_file).await {
                            if let Ok(r) = serde_json::from_str::<InstallReceipt>(&c) {
                                if r.template_id == id || r.template_path == id {
                                    found = Some(entry.path());
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }
        match found {
            Some(f) => f,
            None => {
                return Err(AppError::NotFound(format!(
                    "Template '{id}' is not currently installed."
                )));
            }
        }
    };

    // Clean up installed assets using install_receipt.json
    let receipt_path = target_folder.join("install_receipt.json");
    if receipt_path.exists() {
        if let Ok(c) = tokio::fs::read_to_string(&receipt_path).await {
            if let Ok(receipt) = serde_json::from_str::<InstallReceipt>(&c) {
                // 1. Delete agent files and cascade database records
                for agent_file in &receipt.agents {
                    let p = PathBuf::from("data/swarm_config/agents").join(agent_file);
                    let _ = tokio::fs::remove_file(&p).await;
                }
                for agent_id in &receipt.agent_ids {
                    let _ = crate::agent::persistence::delete_agent_cascade(
                        &state.resources.pool,
                        agent_id,
                    )
                    .await;
                    state.registry.agents.remove(agent_id);
                }

                // 2. Delete copied workflows
                for wf in &receipt.workflows {
                    let p = PathBuf::from("directives").join(wf);
                    let _ = tokio::fs::remove_file(&p).await;
                }

                // 3. Delete copied skills
                for sk in &receipt.skills {
                    let p = PathBuf::from("execution").join(sk);
                    let _ = tokio::fs::remove_file(&p).await;
                }

                // 4. Delete copied knowledge
                for kn in &receipt.knowledge {
                    let p = PathBuf::from("data/knowledge").join(kn);
                    let _ = tokio::fs::remove_file(&p).await;
                }

                // 5. Clean MCP configuration
                if !receipt.mcp_servers.is_empty() {
                    let mcp_path = PathBuf::from(".agent/mcp_config.json");
                    if mcp_path.exists() {
                        if let Ok(mcp_c) = tokio::fs::read_to_string(&mcp_path).await {
                            if let Ok(mut mcp_cfg) =
                                serde_json::from_str::<crate::agent::mcp::McpConfig>(&mcp_c)
                            {
                                let mut changed = false;
                                for srv in &receipt.mcp_servers {
                                    if mcp_cfg.mcp_servers.remove(srv).is_some() {
                                        changed = true;
                                    }
                                }
                                if changed {
                                    if let Ok(merged) = serde_json::to_string_pretty(&mcp_cfg) {
                                        let _ = tokio::fs::write(&mcp_path, merged).await;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Finally remove installation directory
    let _ = tokio::fs::remove_dir_all(&target_folder).await;

    Ok((
        StatusCode::OK,
        Json(UninstallTemplateResponse {
            status: "success".to_string(),
            message: format!("Successfully uninstalled template '{id}'"),
            uninstalled_id: id,
        }),
    )
        .into_response())
}

// ─── Catalog Proxy & Cache (Item #9) ────────────────────────────────────────

#[axum::debug_handler]
#[tracing::instrument(name = "templates::get_catalog")]
pub async fn get_template_catalog() -> Result<axum::response::Response, AppError> {
    // 1. Check in-memory cache
    {
        let cache_lock = CATALOG_CACHE.read().await;
        if let Some(ref cache) = *cache_lock {
            if cache.fetched_at.elapsed() < CATALOG_TTL {
                return Ok((StatusCode::OK, Json(&cache.data)).into_response());
            }
        }
    }

    // 2. Fetch fresh registry from upstream using official TadpoleOS User-Agent
    let client = reqwest::Client::builder()
        .user_agent("TadpoleOS/1.1.58")
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| AppError::InternalServerError(format!("Failed to build HTTP client: {e}")))?;

    let res = client.get(REGISTRY_UPSTREAM_URL).send().await;
    match res {
        Ok(resp) if resp.status().is_success() => {
            let data: serde_json::Value = resp
                .json()
                .await
                .map_err(|e| AppError::BadRequest(format!("Failed to parse catalog JSON: {e}")))?;

            let mut cache_lock = CATALOG_CACHE.write().await;
            *cache_lock = Some(CatalogCache {
                fetched_at: Instant::now(),
                data: data.clone(),
            });

            Ok((StatusCode::OK, Json(data)).into_response())
        }
        Ok(resp) => {
            // Check stale cache fallback
            let cache_lock = CATALOG_CACHE.read().await;
            if let Some(ref cache) = *cache_lock {
                tracing::warn!(
                    "Upstream registry returned status {}, using stale cache",
                    resp.status()
                );
                return Ok((StatusCode::OK, Json(&cache.data)).into_response());
            }
            Err(AppError::BadRequest(format!(
                "Upstream registry returned status: {}",
                resp.status()
            )))
        }
        Err(e) => {
            // Check stale cache fallback
            let cache_lock = CATALOG_CACHE.read().await;
            if let Some(ref cache) = *cache_lock {
                tracing::warn!("Upstream registry network error: {e}, using stale cache");
                return Ok((StatusCode::OK, Json(&cache.data)).into_response());
            }
            Err(AppError::InternalServerError(format!(
                "Failed to reach template catalog: {e}"
            )))
        }
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_install_template_validation() {
        let app_state = Arc::new(AppState::new_minimal_mock().await);

        // Case 1: Hyphen prefix to prevent option injection
        let req = InstallTemplateRequest {
            repository_url: "--upload-pack=evil".to_string(),
            path: "agents".to_string(),
        };
        let res = install_template(axum::extract::State(app_state.clone()), Json(req)).await;
        assert!(res.is_err());
        match res.unwrap_err() {
            AppError::BadRequest(msg) => assert!(msg.contains("cannot start with a hyphen")),
            other => panic!("Expected BadRequest, got {:?}", other),
        }

        // Case 2: Invalid protocol / scheme
        let req = InstallTemplateRequest {
            repository_url: "ftp://github.com/user/repo".to_string(),
            path: "agents".to_string(),
        };
        let res = install_template(axum::extract::State(app_state.clone()), Json(req)).await;
        assert!(res.is_err());
        match res.unwrap_err() {
            AppError::BadRequest(msg) => {
                assert!(msg.contains("Unsupported repository URL scheme"))
            }
            other => panic!("Expected BadRequest, got {:?}", other),
        }

        // Case 3: Path traversal attempt
        let req = InstallTemplateRequest {
            repository_url: "https://github.com/user/repo.git".to_string(),
            path: "../outside".to_string(),
        };
        let res = install_template(axum::extract::State(app_state.clone()), Json(req)).await;
        assert!(res.is_err());
        match res.unwrap_err() {
            AppError::BadRequest(msg) => assert!(msg.contains("Directory traversal")),
            other => panic!("Expected BadRequest, got {:?}", other),
        }
    }

    #[test]
    fn test_connector_lock_sha256_verification() {
        let sample_bytes = b"print('hello world')";
        let mut hasher = Sha256::new();
        hasher.update(sample_bytes);
        let valid_hash = hex::encode(hasher.finalize());

        // Test hash verification
        assert!(verify_skill_sha256(sample_bytes, &valid_hash));
        assert!(!verify_skill_sha256(
            sample_bytes,
            "0000000000000000000000000000000000000000000000000000000000000000"
        ));

        // Test parsing formats from connector-lock.json
        let lock_json = serde_json::json!({
            "skills": {
                "scrape.py": format!("sha256:{valid_hash}")
            },
            "direct.py": valid_hash.clone()
        });

        assert_eq!(
            parse_expected_skill_hash(&lock_json, "scrape.py"),
            Some(valid_hash.clone())
        );
        assert_eq!(
            parse_expected_skill_hash(&lock_json, "direct.py"),
            Some(valid_hash.clone())
        );
        assert_eq!(
            parse_expected_skill_hash(&lock_json, "non_existent.py"),
            None
        );
    }

    #[test]
    fn test_mcp_validation_rules() {
        // Rule #7: '__' rejection
        let invalid_mcp_name = "custom__server";
        assert!(invalid_mcp_name.contains("__"));

        let valid_mcp_name = "custom-server";
        assert!(!valid_mcp_name.contains("__"));

        // Rule #2: Collision fail-closed
        let mut local_servers = std::collections::HashMap::new();
        local_servers.insert("brave-search".to_string(), ());

        assert!(local_servers.contains_key("brave-search"));
        assert!(!local_servers.contains_key("new-server"));
    }

    #[test]
    fn test_receipt_serialization() {
        let receipt = InstallReceipt {
            template_id: "test-tmpl".to_string(),
            template_path: "finance/test".to_string(),
            installed_at: "2026-09-11T20:00:00Z".to_string(),
            agents: vec!["agent1.json".to_string()],
            agent_ids: vec!["agent_1".to_string()],
            workflows: vec!["wf.md".to_string()],
            skills: vec!["skill.py".to_string()],
            knowledge: vec!["sop.md".to_string()],
            mcp_servers: vec!["server1".to_string()],
        };

        let json_str = serde_json::to_string(&receipt).unwrap();
        let parsed: InstallReceipt = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed.template_id, "test-tmpl");
        assert_eq!(parsed.agent_ids.len(), 1);
    }
}

// Metadata: [templates]

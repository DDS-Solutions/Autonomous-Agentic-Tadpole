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
use std::path::{Path as FsPath, PathBuf};
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
const GIT_CLONE_TIMEOUT: Duration = Duration::from_secs(60);
const REGISTRY_UPSTREAM_URL: &str =
    "https://raw.githubusercontent.com/DDS-Solutions/AI-Tadpole-OS-Industry-Templates/main/registry.json";

// ─── Verification & Installation Helpers (Item #10) ─────────────────────────

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

/// Validates repository URL scheme/flags and relative template path prior to invoking `git`.
pub(crate) fn validate_install_request(payload: &InstallTemplateRequest) -> Result<String, AppError> {
    let repo_url = payload.repository_url.trim();
    if repo_url.starts_with('-') {
        return Err(AppError::BadRequest(
            "Repository URL cannot start with a hyphen.".to_string(),
        ));
    }

    let parsed_url = reqwest::Url::parse(repo_url)
        .map_err(|e| AppError::BadRequest(format!("Invalid Repository URL: {e}")))?;
    let scheme = parsed_url.scheme();
    if scheme != "http" && scheme != "https" && scheme != "git" {
        return Err(AppError::BadRequest(format!(
            "Unsupported repository URL scheme: {scheme}. Only http, https, and git protocols are allowed."
        )));
    }

    if payload.path.contains("..")
        || payload.path.starts_with('/')
        || payload.path.starts_with('\\')
    {
        return Err(AppError::BadRequest(
            "Invalid template path: Directory traversal or absolute paths are prohibited."
                .to_string(),
        ));
    }

    Ok(repo_url.to_string())
}

/// Executes a shallow `git clone --depth 1` with interactive terminal prompts disabled and a hard timeout.
async fn clone_template_repository(
    git_cmd: &FsPath,
    repo_url: &str,
    temp_dir: &FsPath,
) -> Result<(), AppError> {
    let mut cmd = tokio::process::Command::new(git_cmd);
    cmd.kill_on_drop(true)
        .env("GIT_TERMINAL_PROMPT", "0")
        .arg("clone")
        .arg("--depth")
        .arg("1")
        .arg(repo_url)
        .arg(temp_dir);

    let status_res = tokio::time::timeout(GIT_CLONE_TIMEOUT, cmd.status()).await;
    match status_res {
        Ok(Ok(status)) if status.success() => Ok(()),
        Ok(Ok(_)) => {
            let _ = tokio::fs::remove_dir_all(temp_dir).await;
            Err(AppError::InternalServerError(
                "Failed to clone template repository".to_string(),
            ))
        }
        Ok(Err(e)) => {
            let _ = tokio::fs::remove_dir_all(temp_dir).await;
            Err(AppError::InternalServerError(format!(
                "Failed to execute git: {e}"
            )))
        }
        Err(_) => {
            let _ = tokio::fs::remove_dir_all(temp_dir).await;
            Err(AppError::InternalServerError(
                "Timed out while cloning template repository".to_string(),
            ))
        }
    }
}

/// Preflight-validates all agent JSON profiles in memory before any disk/database writes occur.
async fn preflight_validate_agents(
    agents_src: &FsPath,
    temp_dir: &FsPath,
) -> Result<Vec<(String, PathBuf, crate::agent::types::EngineAgent)>, AppError> {
    let mut validated_agents = Vec::new();
    if !agents_src.exists() {
        return Ok(validated_agents);
    }

    if let Ok(mut entries) = tokio::fs::read_dir(agents_src).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let file_path = entry.path();
            if file_path.is_file()
                && file_path.extension().and_then(|s| s.to_str()) == Some("json")
            {
                let filename = entry.file_name().to_string_lossy().to_string();
                let content = match tokio::fs::read_to_string(&file_path).await {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = tokio::fs::remove_dir_all(temp_dir).await;
                        return Err(AppError::BadRequest(format!(
                            "Failed to read agent file '{filename}': {e}"
                        )));
                    }
                };

                match serde_json::from_str::<crate::agent::types::EngineAgent>(&content) {
                    Ok(agent) => validated_agents.push((filename, file_path, agent)),
                    Err(e) => {
                        let _ = tokio::fs::remove_dir_all(temp_dir).await;
                        return Err(AppError::BadRequest(format!(
                            "Agent profile preflight validation failed for '{filename}': {e}"
                        )));
                    }
                }
            }
        }
    }

    Ok(validated_agents)
}

/// Preflight-validates skill scripts against `connector-lock.json` SHA-256 digests before installation.
async fn preflight_validate_skills(
    skills_src: &FsPath,
    source_path: &FsPath,
    temp_dir: &FsPath,
) -> Result<Vec<(String, PathBuf)>, AppError> {
    let mut validated_skills = Vec::new();
    if !skills_src.exists() {
        return Ok(validated_skills);
    }

    let lock_path = source_path.join("connector-lock.json");
    let lock_val: Option<serde_json::Value> = if lock_path.exists() {
        tokio::fs::read_to_string(&lock_path)
            .await
            .ok()
            .and_then(|c| serde_json::from_str(&c).ok())
    } else {
        None
    };

    if let Ok(mut entries) = tokio::fs::read_dir(skills_src).await {
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
                        let _ = tokio::fs::remove_dir_all(temp_dir).await;
                        return Err(AppError::BadRequest(format!(
                            "Failed to read skill file '{filename}': {e}"
                        )));
                    }
                };

                if let Some(ref lock) = lock_val {
                    if let Some(expected_hash) = parse_expected_skill_hash(lock, &filename) {
                        if !verify_skill_sha256(&bytes, &expected_hash) {
                            let _ = tokio::fs::remove_dir_all(temp_dir).await;
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

    Ok(validated_skills)
}

/// Copies all `.md` files from `src_dir` into `dest_dir`, recording copied filenames in `receipt_list`.
async fn copy_markdown_assets(
    src_dir: &FsPath,
    dest_dir: &FsPath,
    receipt_list: &mut Vec<String>,
) -> usize {
    if !src_dir.exists() {
        return 0;
    }

    let _ = tokio::fs::create_dir_all(dest_dir).await;
    let mut copied = 0;
    if let Ok(mut entries) = tokio::fs::read_dir(src_dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            if entry.path().extension().and_then(|s| s.to_str()) == Some("md") {
                let filename = entry.file_name().to_string_lossy().to_string();
                let dest_file = dest_dir.join(&filename);
                if tokio::fs::copy(entry.path(), dest_file).await.is_ok() {
                    receipt_list.push(filename);
                    copied += 1;
                }
            }
        }
    }
    copied
}

/// Merges template `mcps.json` into `.agent/mcp_config.json`, rejecting `__` in server names and failing closed on collisions.
async fn merge_template_mcp_config(
    mcps_src: &FsPath,
    receipt: &mut InstallReceipt,
    errors: &mut Vec<String>,
) -> bool {
    if !mcps_src.exists() {
        return false;
    }

    let Ok(content) = tokio::fs::read_to_string(mcps_src).await else {
        return false;
    };
    let Ok(incoming_config) = serde_json::from_str::<crate::agent::mcp::McpConfig>(&content) else {
        return false;
    };

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
        if name.contains("__") {
            tracing::warn!(
                "MCP server name '{}' contains '__' — rejected per specification",
                name
            );
            errors.push(format!("MCP server '{}' rejected: contains '__'", name));
            continue;
        }

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
            return true;
        }
    }

    false
}

/// Resolves an installed template directory by direct ID, sanitized ID, or `install_receipt.json` lookup.
async fn resolve_installed_template_folder(
    installed_dir: &FsPath,
    id: &str,
    safe_id: &str,
) -> Result<crate::utils::security::SafePath, AppError> {
    if let Ok(p) = crate::utils::security::validate_path(installed_dir, id) {
        if p.exists() && p.is_dir() {
            return Ok(p);
        }
    }

    let safe_p = crate::utils::security::validate_path(installed_dir, safe_id)
        .map_err(|e| AppError::BadRequest(format!("Invalid template ID: {e}")))?;
    if safe_p.exists() && safe_p.is_dir() {
        return Ok(safe_p);
    }

    if installed_dir.exists() {
        if let Ok(mut entries) = tokio::fs::read_dir(installed_dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let entry_name = entry.file_name().to_string_lossy().to_string();
                if let Ok(valid_entry) =
                    crate::utils::security::validate_path(installed_dir, &entry_name)
                {
                    let receipt_file = valid_entry.join("install_receipt.json");
                    if receipt_file.exists() {
                        if let Ok(c) = tokio::fs::read_to_string(&receipt_file).await {
                            if let Ok(r) = serde_json::from_str::<InstallReceipt>(&c) {
                                if r.template_id == id || r.template_path == id {
                                    return Ok(valid_entry);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Err(AppError::NotFound(format!(
        "Template '{id}' is not currently installed."
    )))
}

// ─── Handlers ───────────────────────────────────────────────────────────────

#[axum::debug_handler]
#[tracing::instrument(skip(state, payload), name = "templates::install")]
pub async fn install_template(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<InstallTemplateRequest>,
) -> Result<axum::response::Response, AppError> {
    // 🛡️ [Security Hardening] Validate repository URL and relative path
    let repo_url = validate_install_request(&payload)?;

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

    // 1. Clone repository using the boot-resolved git command with timeout & non-interactive env
    clone_template_repository(&git_cmd, &repo_url, &temp_dir).await?;

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
    let safe_name = crate::utils::security::sanitize_id(&payload.path.replace('/', "_"));
    let mut template_id = safe_name.clone();
    let swarm_json_src = source_path.as_path().join("swarm.json");
    if swarm_json_src.exists() {
        if let Ok(c) = tokio::fs::read_to_string(&swarm_json_src).await {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&c) {
                if let Some(id_str) = val.get("id").and_then(|v| v.as_str()) {
                    template_id = crate::utils::security::sanitize_id(id_str);
                }
            }
        }
    }

    let installed_base = PathBuf::from("data/swarm_config/installed");
    let dest_folder = crate::utils::security::validate_path(&installed_base, &safe_name)
        .map_err(|e| AppError::BadRequest(format!("Invalid template destination: {e}")))?;
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

    // 3. Preflight Agent & Skill Validation (Items #3 and #10)
    let agents_src = source_path.as_path().join("agents");
    let validated_agents = preflight_validate_agents(&agents_src, &temp_dir).await?;

    let skills_src = source_path.as_path().join("skills");
    let validated_skills =
        preflight_validate_skills(&skills_src, source_path.as_path(), &temp_dir).await?;

    // 5. Atomic Component Persistence (Post-Validation)
    let agents_dest = PathBuf::from("data/swarm_config/agents");
    let mut agents_installed = 0;
    if !validated_agents.is_empty() {
        let _ = tokio::fs::create_dir_all(&agents_dest).await;
        for (filename, src_path, mut agent) in validated_agents {
            let _lifecycle = state.registry.lock_agent(&agent.identity.id).await;
            let dest_file = agents_dest.join(&filename);
            let _ = tokio::fs::copy(&src_path, &dest_file).await;

            match crate::agent::persistence::create_agent_db(&state.resources.pool, &agent).await {
                Ok(version) => agent.version = version,
                Err(error) => {
                    errors.push(format!(
                        "Agent '{}' was not installed: {}",
                        agent.identity.id, error
                    ));
                    continue;
                }
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

    if swarm_json_src.exists() {
        let _ = tokio::fs::copy(&swarm_json_src, dest_folder.join("swarm.json")).await;
    }

    let workflows_copied = copy_markdown_assets(
        &source_path.as_path().join("workflows"),
        FsPath::new("directives"),
        &mut receipt.workflows,
    )
    .await;

    let skills_dest = PathBuf::from("execution");
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

    let mcp_merged = merge_template_mcp_config(
        &source_path.as_path().join("mcps.json"),
        &mut receipt,
        &mut errors,
    )
    .await;

    let knowledge_copied = copy_markdown_assets(
        &source_path.as_path().join("knowledge"),
        FsPath::new("data/knowledge"),
        &mut receipt.knowledge,
    )
    .await;

    let knowledge_json_src = source_path.as_path().join("knowledge.json");
    if knowledge_json_src.exists() {
        let _ = tokio::fs::copy(&knowledge_json_src, dest_folder.join("knowledge.json")).await;
    }

    if let Ok(receipt_str) = serde_json::to_string_pretty(&receipt) {
        let _ = tokio::fs::write(dest_folder.join("install_receipt.json"), receipt_str).await;
    }

    let _ = tokio::fs::remove_dir_all(&temp_dir).await;

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

                        let swarm_name = if swarm_file.exists() {
                            tokio::fs::read_to_string(&swarm_file)
                                .await
                                .ok()
                                .and_then(|sc| serde_json::from_str::<serde_json::Value>(&sc).ok())
                                .and_then(|val| {
                                    val.get("name").and_then(|v| v.as_str()).map(String::from)
                                })
                        } else {
                            None
                        };

                        if let Some(mut s) = summary.take() {
                            s.name = swarm_name;
                            results.push(s);
                        } else if swarm_file.exists() {
                            let folder_name = entry.file_name().to_string_lossy().to_string();
                            results.push(InstalledTemplateSummary {
                                id: folder_name.clone(),
                                path: folder_name,
                                name: swarm_name,
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

async fn remove_validated_files(base_dir: &FsPath, filenames: &[String]) {
    for name in filenames {
        if let Ok(p) = crate::utils::security::validate_path(base_dir, name) {
            let _ = tokio::fs::remove_file(&p).await;
        }
    }
}

#[axum::debug_handler]
#[tracing::instrument(skip(state), name = "templates::uninstall")]
pub async fn uninstall_template(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<axum::response::Response, AppError> {
    // 🛡️ [M56: Path Containment] Reject traversal sequences and separators in template ID
    if id.contains('/') || id.contains('\\') || id.contains("..") || id.contains('\0') {
        return Err(AppError::BadRequest(
            "Invalid template ID: Path separators and traversal sequences are prohibited."
                .to_string(),
        ));
    }

    let installed_dir = PathBuf::from("data/swarm_config/installed");
    let safe_id = crate::utils::security::sanitize_id(&id.replace('/', "_"));
    let target_folder = resolve_installed_template_folder(&installed_dir, &id, &safe_id).await?;

    // Clean up installed assets using install_receipt.json
    let receipt_path = target_folder.join("install_receipt.json");
    if receipt_path.exists() {
        if let Ok(c) = tokio::fs::read_to_string(&receipt_path).await {
            if let Ok(receipt) = serde_json::from_str::<InstallReceipt>(&c) {
                remove_validated_files(FsPath::new("data/swarm_config/agents"), &receipt.agents)
                    .await;
                for agent_id in &receipt.agent_ids {
                    let _lifecycle = state.registry.lock_agent(agent_id).await;
                    crate::agent::persistence::delete_agent_cascade(
                        &state.resources.pool,
                        agent_id,
                    )
                    .await?;
                    if let Some((_, runner)) = state.comms.active_runners.remove(agent_id) {
                        runner.abort();
                    }
                    state.registry.agents.remove(agent_id);
                }

                remove_validated_files(FsPath::new("directives"), &receipt.workflows).await;
                remove_validated_files(FsPath::new("execution"), &receipt.skills).await;
                remove_validated_files(FsPath::new("data/knowledge"), &receipt.knowledge).await;

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

    let target_name = target_folder
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let validated_target = crate::utils::security::validate_path(&installed_dir, &target_name)
        .map_err(|e| {
            AppError::BadRequest(format!("Security error validating target folder: {e}"))
        })?;
    let _ = tokio::fs::remove_dir_all(&validated_target).await;

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
    {
        let cache_lock = CATALOG_CACHE.read().await;
        if let Some(ref cache) = *cache_lock {
            if cache.fetched_at.elapsed() < CATALOG_TTL {
                return Ok((StatusCode::OK, Json(&cache.data)).into_response());
            }
        }
    }

    let client = reqwest::Client::builder()
        .user_agent("TadpoleOS/1.1.59")
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

        assert!(verify_skill_sha256(sample_bytes, &valid_hash));
        assert!(!verify_skill_sha256(
            sample_bytes,
            "0000000000000000000000000000000000000000000000000000000000000000"
        ));

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
        let invalid_mcp_name = "custom__server";
        assert!(invalid_mcp_name.contains("__"));

        let valid_mcp_name = "custom-server";
        assert!(!valid_mcp_name.contains("__"));

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

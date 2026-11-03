//! @docs ARCHITECTURE:Networking
//!
//! ### AI Assist Note
//! **Template Installation Workflow**: Handles cloning remote repositories with transport restrictions,
//! preflight agent and skill verification, atomic asset deployment, and Merkle audit logging.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Git clone failure, timeout, agent reserved ID collision, or digest mismatch.
//! - **Telemetry Link**: Search `[templates_install]` in tracing logs.
//! - **Trace Scope**: `server-rs::routes::templates::install`

use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use once_cell::sync::Lazy;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path as FsPath, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use crate::error::AppError;
use crate::state::AppState;
use super::models::{InstallReceipt, InstallTemplateRequest, InstallTemplateResponse};
use super::principal::TemplatePrincipal;
use super::security::{
    parse_expected_skill_hash, redact_url, validate_install_request_async,
    validate_mcp_server_config, verify_skill_sha256, RESERVED_AGENT_IDS,
};

pub const GIT_CLONE_TIMEOUT: Duration = Duration::from_secs(60);
pub const MAX_AGENT_CONFIG_BYTES: u64 = 1 * 1024 * 1024; // 1 MiB
pub const MAX_SKILL_SCRIPT_BYTES: u64 = 5 * 1024 * 1024; // 5 MiB

/// Concurrency limiter for repository clones to prevent resource exhaustion attacks.
static CLONE_SEMAPHORE: Lazy<tokio::sync::Semaphore> = Lazy::new(|| tokio::sync::Semaphore::new(2));

/// Hardened shallow git clone preventing transport bypass, symlink creation, and credential leakage.
pub async fn clone_template_repository(
    git_cmd: &FsPath,
    repo_url: &str,
    temp_dir: &FsPath,
) -> Result<(), AppError> {
    let _permit = CLONE_SEMAPHORE
        .acquire()
        .await
        .map_err(|_| AppError::InternalServerError("Clone semaphore closed".to_string()))?;

    let dev_null = if cfg!(windows) { "NUL" } else { "/dev/null" };

    let mut cmd = tokio::process::Command::new(git_cmd);
    cmd.kill_on_drop(true)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", dev_null)
        .env("GIT_ASKPASS", "")
        .arg("clone")
        .arg("--depth")
        .arg("1")
        .arg("-c")
        .arg("core.symlinks=false")
        .arg("-c")
        .arg("protocol.allow=never")
        .arg("-c")
        .arg("protocol.https.allow=always")
        .arg("-c")
        .arg("protocol.http.allow=always")
        .arg("-c")
        .arg("credential.helper=")
        .arg("--")
        .arg(repo_url)
        .arg(temp_dir);

    let status_res = tokio::time::timeout(GIT_CLONE_TIMEOUT, cmd.status()).await;
    match status_res {
        Ok(Ok(status)) if status.success() => Ok(()),
        Ok(Ok(exit)) => {
            let _ = tokio::fs::remove_dir_all(temp_dir).await;
            Err(AppError::InternalServerError(format!(
                "Git clone failed with exit code: {}",
                exit
            )))
        }
        Ok(Err(e)) => {
            let _ = tokio::fs::remove_dir_all(temp_dir).await;
            Err(AppError::InternalServerError(format!(
                "Failed to execute git process: {e}"
            )))
        }
        Err(_) => {
            let _ = tokio::fs::remove_dir_all(temp_dir).await;
            Err(AppError::InternalServerError(
                "Timed out while cloning template repository (60s limit)".to_string(),
            ))
        }
    }
}

/// Preflight-validates all agent JSON profiles in memory, verifying size, schema, and reserved ID bounds.
pub async fn preflight_validate_agents(
    agents_src: &FsPath,
    temp_dir: &FsPath,
) -> Result<Vec<(String, PathBuf, crate::agent::types::EngineAgent)>, AppError> {
    let mut validated_agents = Vec::new();
    if !agents_src.exists() {
        return Ok(validated_agents);
    }

    let mut entries = tokio::fs::read_dir(agents_src).await.map_err(|e| {
        AppError::BadRequest(format!("Failed to read agents source directory: {e}"))
    })?;

    while let Some(entry) = entries.next_entry().await.map_err(|e| {
        AppError::BadRequest(format!("Failed to iterate agents directory: {e}"))
    })? {
        let file_path = entry.path();

        if let Ok(symlink_meta) = tokio::fs::symlink_metadata(&file_path).await {
            if symlink_meta.file_type().is_symlink() {
                let _ = tokio::fs::remove_dir_all(temp_dir).await;
                return Err(AppError::BadRequest(format!(
                    "Security violation: agent file '{}' is a symbolic link.",
                    entry.file_name().to_string_lossy()
                )));
            }
            if symlink_meta.len() > MAX_AGENT_CONFIG_BYTES {
                let _ = tokio::fs::remove_dir_all(temp_dir).await;
                return Err(AppError::BadRequest(format!(
                    "Agent configuration file '{}' exceeds maximum allowed size (1 MiB).",
                    entry.file_name().to_string_lossy()
                )));
            }
        }

        if file_path.is_file() && file_path.extension().and_then(|s| s.to_str()) == Some("json") {
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
                Ok(agent) => {
                    let agent_id_lower = agent.identity.id.trim().to_ascii_lowercase();
                    if RESERVED_AGENT_IDS.contains(&agent_id_lower.as_str()) {
                        let _ = tokio::fs::remove_dir_all(temp_dir).await;
                        return Err(AppError::BadRequest(format!(
                            "Agent profile '{filename}' rejected: ID '{}' is reserved by system architecture.",
                            agent.identity.id
                        )));
                    }
                    validated_agents.push((filename, file_path, agent));
                }
                Err(e) => {
                    let _ = tokio::fs::remove_dir_all(temp_dir).await;
                    return Err(AppError::BadRequest(format!(
                        "Agent profile preflight validation failed for '{filename}': {e}"
                    )));
                }
            }
        }
    }

    Ok(validated_agents)
}

/// Preflight-validates skill scripts against `connector-lock.json` SHA-256 digests before installation.
pub async fn preflight_validate_skills(
    skills_src: &FsPath,
    source_path: &FsPath,
    temp_dir: &FsPath,
) -> Result<Vec<(String, PathBuf, String)>, AppError> {
    let mut validated_skills = Vec::new();
    if !skills_src.exists() {
        return Ok(validated_skills);
    }

    let lock_path = source_path.join("connector-lock.json");
    let lock_val: Option<serde_json::Value> = if lock_path.exists() {
        let content = tokio::fs::read_to_string(&lock_path).await.map_err(|e| {
            AppError::BadRequest(format!("Failed to read connector-lock.json: {e}"))
        })?;
        let parsed = serde_json::from_str::<serde_json::Value>(&content).map_err(|e| {
            AppError::BadRequest(format!("Failed to parse connector-lock.json: {e}"))
        })?;
        Some(parsed)
    } else {
        None
    };

    let mut entries = tokio::fs::read_dir(skills_src).await.map_err(|e| {
        AppError::BadRequest(format!("Failed to read skills source directory: {e}"))
    })?;

    while let Some(entry) = entries.next_entry().await.map_err(|e| {
        AppError::BadRequest(format!("Failed to iterate skills directory: {e}"))
    })? {
        let path = entry.path();
        if let Ok(symlink_meta) = tokio::fs::symlink_metadata(&path).await {
            if symlink_meta.file_type().is_symlink() {
                let _ = tokio::fs::remove_dir_all(temp_dir).await;
                return Err(AppError::BadRequest(format!(
                    "Security violation: skill file '{}' is a symbolic link.",
                    entry.file_name().to_string_lossy()
                )));
            }
            if symlink_meta.len() > MAX_SKILL_SCRIPT_BYTES {
                let _ = tokio::fs::remove_dir_all(temp_dir).await;
                return Err(AppError::BadRequest(format!(
                    "Skill file '{}' exceeds maximum allowed size (5 MiB).",
                    entry.file_name().to_string_lossy()
                )));
            }
        }

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

            let mut hasher = Sha256::new();
            hasher.update(&bytes);
            let computed_digest = hex::encode(hasher.finalize()).to_lowercase();

            if let Some(ref lock) = lock_val {
                if let Some(expected_hash) = parse_expected_skill_hash(lock, &filename) {
                    if !verify_skill_sha256(&bytes, &expected_hash) {
                        let _ = tokio::fs::remove_dir_all(temp_dir).await;
                        return Err(AppError::BadRequest(format!(
                            "Integrity check failed for skill '{filename}': SHA-256 mismatch with connector-lock.json"
                        )));
                    }
                } else {
                    let allow_unverified = std::env::var("ALLOW_UNVERIFIED_TEMPLATES")
                        .map(|v| v == "1" || v == "true")
                        .unwrap_or(false);
                    if !allow_unverified {
                        let _ = tokio::fs::remove_dir_all(temp_dir).await;
                        return Err(AppError::BadRequest(format!(
                            "Integrity check failed for skill '{filename}': missing expected hash in connector-lock.json (fail-closed)"
                        )));
                    }
                }
            } else {
                let allow_unverified = std::env::var("ALLOW_UNVERIFIED_TEMPLATES")
                    .map(|v| v == "1" || v == "true")
                    .unwrap_or(false);
                if !allow_unverified {
                    let _ = tokio::fs::remove_dir_all(temp_dir).await;
                    return Err(AppError::BadRequest(format!(
                        "Integrity check failed for skill '{filename}': connector-lock.json missing from template (fail-closed)"
                    )));
                }
            }

            validated_skills.push((filename, path, computed_digest));
        }
    }

    Ok(validated_skills)
}

/// Copies all `.md` files from `src_dir` into `dest_dir`, recording copied filenames and digests.
pub async fn copy_markdown_assets(
    src_dir: &FsPath,
    dest_dir: &FsPath,
    receipt_list: &mut Vec<String>,
    file_digests: &mut HashMap<String, String>,
) -> Result<usize, AppError> {
    if !src_dir.exists() {
        return Ok(0);
    }

    tokio::fs::create_dir_all(dest_dir).await.map_err(|e| {
        AppError::InternalServerError(format!(
            "Failed to create asset directory '{}': {e}",
            dest_dir.display()
        ))
    })?;

    let mut copied = 0;
    let mut entries = tokio::fs::read_dir(src_dir).await.map_err(|e| {
        AppError::BadRequest(format!("Failed to read asset source directory: {e}"))
    })?;

    while let Some(entry) = entries.next_entry().await.map_err(|e| {
        AppError::BadRequest(format!("Failed to iterate asset directory: {e}"))
    })? {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("md") {
            let filename = entry.file_name().to_string_lossy().to_string();
            let dest_file = dest_dir.join(&filename);
            let bytes = tokio::fs::read(&path).await.map_err(|e| {
                AppError::BadRequest(format!("Failed to read markdown file '{filename}': {e}"))
            })?;

            let mut hasher = Sha256::new();
            hasher.update(&bytes);
            let digest = hex::encode(hasher.finalize()).to_lowercase();

            tokio::fs::write(&dest_file, &bytes).await.map_err(|e| {
                AppError::InternalServerError(format!(
                    "Failed to write markdown asset to '{}': {e}",
                    dest_file.display()
                ))
            })?;

            file_digests.insert(filename.clone(), digest);
            receipt_list.push(filename);
            copied += 1;
        }
    }
    Ok(copied)
}

/// Atomically merges template `mcps.json` into `.agent/mcp_config.json`, enforcing launcher allow-lists.
pub async fn merge_template_mcp_config(
    mcps_src: &FsPath,
    mcp_config_path: &FsPath,
    receipt: &mut InstallReceipt,
    errors: &mut Vec<String>,
) -> bool {
    if !mcps_src.exists() {
        return false;
    }

    let Ok(content) = tokio::fs::read_to_string(mcps_src).await else {
        errors.push("Failed to read template mcps.json".to_string());
        return false;
    };
    let Ok(incoming_config) = serde_json::from_str::<crate::agent::mcp::McpConfig>(&content) else {
        errors.push("Failed to parse template mcps.json: invalid schema".to_string());
        return false;
    };

    let mut current_config = if mcp_config_path.exists() {
        tokio::fs::read_to_string(mcp_config_path)
            .await
            .ok()
            .and_then(|c| serde_json::from_str::<crate::agent::mcp::McpConfig>(&c).ok())
            .unwrap_or_else(|| crate::agent::mcp::McpConfig {
                mcp_servers: HashMap::new(),
            })
    } else {
        crate::agent::mcp::McpConfig {
            mcp_servers: HashMap::new(),
        }
    };

    let mut servers_added = 0;
    for (name, config) in incoming_config.mcp_servers {
        if let Err(reason) = validate_mcp_server_config(&name, &config) {
            tracing::warn!(server = %name, error = %reason, "MCP server definition rejected");
            errors.push(reason);
            continue;
        }

        if current_config.mcp_servers.contains_key(&name) {
            tracing::warn!("MCP server collision for '{}' — skipping (fail closed)", name);
            errors.push(format!("MCP server '{}' collision: existing server preserved", name));
            continue;
        }

        current_config.mcp_servers.insert(name.clone(), config);
        receipt.mcp_servers.push(name);
        servers_added += 1;
    }

    if servers_added > 0 {
        if let Ok(merged_json) = serde_json::to_string_pretty(&current_config) {
            if let Some(parent) = mcp_config_path.parent() {
                let _ = tokio::fs::create_dir_all(parent).await;
            }
            let tmp_mcp = mcp_config_path.with_extension(format!("tmp.{}", uuid::Uuid::new_v4()));
            if tokio::fs::write(&tmp_mcp, merged_json).await.is_ok() {
                if tokio::fs::rename(&tmp_mcp, mcp_config_path).await.is_ok() {
                    return true;
                }
            }
        }
    }

    false
}

/// POST /engine/templates/install
///
/// Clones, verifies, and installs an authenticated multi-agent swarm template.
#[tracing::instrument(skip(principal, state, payload), fields(operator_id = %principal.id), name = "templates::install")]
pub async fn install_template(
    principal: TemplatePrincipal,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<InstallTemplateRequest>,
) -> Result<impl IntoResponse, AppError> {
    // 🛡️ [Security Hardening] Validate repository URL (SSRF & traversal checks)
    let repo_url = validate_install_request_async(&payload).await?;

    let git_cmd = match state.resources.git_path.as_ref() {
        Some(path) => path.clone(),
        None => {
            return Err(AppError::InternalServerError(
                "Git executable is not resolved on the system.".to_string(),
            ));
        }
    };

    let dl_id = uuid::Uuid::new_v4().to_string();
    let temp_dir = state
        .base_dir
        .join("data")
        .join(".bunker_cache")
        .join(&dl_id);

    tokio::fs::create_dir_all(&temp_dir).await.map_err(|e| {
        AppError::InternalServerError(format!("Failed to create temporary staging directory: {e}"))
    })?;

    // 1. Clone repository using hardened git parameters & timeout
    clone_template_repository(&git_cmd, &repo_url, &temp_dir).await?;

    // 2. Identify template source directory with containment validation
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
            "Template path '{}' not found in cloned repository",
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
                    let sanitized = crate::utils::security::sanitize_id(id_str);
                    if !sanitized.is_empty() {
                        template_id = sanitized;
                    }
                }
            }
        }
    }

    let installed_base = state.base_dir.join("data/swarm_config/installed");
    let dest_folder = crate::utils::security::validate_path(&installed_base, &safe_name)
        .map_err(|e| AppError::BadRequest(format!("Invalid template destination: {e}")))?;
    tokio::fs::create_dir_all(&dest_folder).await.map_err(|e| {
        AppError::InternalServerError(format!("Failed to create template directory: {e}"))
    })?;

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
        file_digests: HashMap::new(),
    };
    let mut errors: Vec<String> = Vec::new();

    // 3. Preflight Agent & Skill Validation
    let agents_src = source_path.as_path().join("agents");
    let validated_agents = preflight_validate_agents(&agents_src, &temp_dir).await?;
    let total_agents_preflight = validated_agents.len();

    let skills_src = source_path.as_path().join("skills");
    let validated_skills =
        preflight_validate_skills(&skills_src, source_path.as_path(), &temp_dir).await?;

    // 4. Persistence of Agents (DB First -> Version Assignment -> Canonical File Write)
    let agents_dest = state.base_dir.join("data/swarm_config/agents");
    let mut agents_installed = 0;
    if !validated_agents.is_empty() {
        let _ = tokio::fs::create_dir_all(&agents_dest).await;
        for (filename, _src_path, mut agent) in validated_agents {
            if state.comms.active_runners.contains_key(&agent.identity.id) {
                errors.push(format!(
                    "Agent '{}' is currently running; installation skipped to prevent execution interruption.",
                    agent.identity.id
                ));
                continue;
            }

            let _lifecycle = state.registry.lock_agent(&agent.identity.id).await;

            match crate::agent::persistence::create_agent_db(&state.resources.pool, &agent).await {
                Ok(version) => agent.version = version,
                Err(error) => {
                    errors.push(format!(
                        "Agent '{}' database insertion failed: {}",
                        agent.identity.id, error
                    ));
                    continue;
                }
            }

            let dest_file = agents_dest.join(&filename);
            let canonical_json = serde_json::to_string_pretty(&agent).map_err(|e| {
                AppError::InternalServerError(format!("Failed to serialize agent JSON: {e}"))
            })?;
            if let Err(e) = tokio::fs::write(&dest_file, &canonical_json).await {
                errors.push(format!("Failed to write agent file '{filename}': {e}"));
                continue;
            }

            let mut hasher = Sha256::new();
            hasher.update(canonical_json.as_bytes());
            receipt.file_digests.insert(filename.clone(), hex::encode(hasher.finalize()).to_lowercase());

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

    // 5. Directives/Workflows Installation
    let directives_dest = state.base_dir.join("directives");
    let workflows_copied = copy_markdown_assets(
        &source_path.as_path().join("workflows"),
        &directives_dest,
        &mut receipt.workflows,
        &mut receipt.file_digests,
    )
    .await?;

    // 6. Skills Installation (Namespaced per Template)
    let skills_dest = state.base_dir.join("execution");
    let mut skills_copied = 0;
    if !validated_skills.is_empty() {
        let _ = tokio::fs::create_dir_all(&skills_dest).await;
        for (filename, src_path, digest) in validated_skills {
            let dest_file = skills_dest.join(&filename);
            if tokio::fs::copy(&src_path, &dest_file).await.is_ok() {
                receipt.file_digests.insert(filename.clone(), digest);
                receipt.skills.push(filename);
                skills_copied += 1;
            } else {
                errors.push(format!("Failed to copy validated skill '{filename}'"));
            }
        }
    }

    // 7. MCP Server Manifest Merging (Validated Launchers)
    let mcp_config_path = state.base_dir.join(".agent/mcp_config.json");
    let mcp_merged = merge_template_mcp_config(
        &source_path.as_path().join("mcps.json"),
        &mcp_config_path,
        &mut receipt,
        &mut errors,
    )
    .await;

    // 8. Knowledge Assets Installation
    let knowledge_dest = state.base_dir.join("data/knowledge");
    let knowledge_copied = copy_markdown_assets(
        &source_path.as_path().join("knowledge"),
        &knowledge_dest,
        &mut receipt.knowledge,
        &mut receipt.file_digests,
    )
    .await?;

    let knowledge_json_src = source_path.as_path().join("knowledge.json");
    if knowledge_json_src.exists() {
        let _ = tokio::fs::copy(&knowledge_json_src, dest_folder.join("knowledge.json")).await;
    }

    // 9. Persist Install Receipt
    if let Ok(receipt_str) = serde_json::to_string_pretty(&receipt) {
        let _ = tokio::fs::write(dest_folder.join("install_receipt.json"), receipt_str).await;
    }

    // 10. Clean Up Staging Cache
    let _ = tokio::fs::remove_dir_all(&temp_dir).await;

    // 11. Record in Immutable Merkle Audit Trail
    let audit_params = serde_json::to_string(&serde_json::json!({
        "repository_url": redact_url(&payload.repository_url),
        "path": &payload.path,
        "template_id": &template_id,
        "agents_installed": agents_installed,
        "skills_copied": skills_copied,
        "workflows_copied": workflows_copied,
        "mcp_merged": mcp_merged,
    }))
    .unwrap_or_default();

    let _ = state
        .security
        .audit_trail
        .record(
            "template",
            None,
            Some(&principal.id),
            "template_install",
            &audit_params,
        )
        .await;

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
        agents_skipped: total_agents_preflight.saturating_sub(agents_installed),
        workflows_copied,
        skills_copied,
        mcp_merged,
        knowledge_copied,
        errors,
    };

    let status_code = if response.status == "success" {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };

    Ok((status_code, Json(response)))
}

// Metadata: [templates_install]


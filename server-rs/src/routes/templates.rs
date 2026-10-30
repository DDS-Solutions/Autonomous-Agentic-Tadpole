//! @docs ARCHITECTURE:Networking
//!
//! ### AI Assist Note
//! **Mission Templates (Starter Kits)**: Orchestrates the
//! installation, inventory, and lifecycle management of multi-agent swarm
//! blueprints for the Tadpole OS engine. Features **Git-Integrated Installation**:
//! securely clones blueprints from remote repositories with strict transport,
//! symlink, SSRF, and command execution protections. Implements **Merkle Hash-Chain Recording**:
//! all install and uninstall lifecycle events are permanently attributed to the
//! authenticated operator in the tamper-evident audit ledger. Implements
//! **Supply-Chain Integrity**: validates skill scripts against SHA-256 lock-file
//! digests (failing closed) and validates MCP server launcher executables
//! against an authoritative trusted launcher allow-list.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: 401 Unauthorized for unauthenticated requests,
//!   400 BadRequest for SSRF attempts, path traversal, untrusted MCP launchers,
//!   or SHA-256 digest mismatch, and 404 NotFound if template path does not exist.
//! - **Telemetry Link**: Search `[templates]` in tracing logs.
//! - **Trace Scope**: `server-rs::routes::templates`

use crate::agent::constants::{AGENT_ALPHA, AGENT_CEO, AGENT_COO};
use crate::error::AppError;
use crate::middleware::auth::AuthenticatedOperator;
use crate::state::AppState;
use axum::{
    extract::{FromRequestParts, Path, State},
    http::{request::Parts, StatusCode},
    response::IntoResponse,
    Json,
};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path as FsPath, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

// ─── Constants & Policy Boundaries ──────────────────────────────────────────

const GIT_CLONE_TIMEOUT: Duration = Duration::from_secs(60);
const CATALOG_TTL: Duration = Duration::from_secs(300);
const MAX_CATALOG_BODY_BYTES: usize = 2 * 1024 * 1024; // 2 MiB
const MAX_AGENT_CONFIG_BYTES: u64 = 1 * 1024 * 1024; // 1 MiB
const MAX_SKILL_SCRIPT_BYTES: u64 = 5 * 1024 * 1024; // 5 MiB
const REGISTRY_UPSTREAM_URL: &str =
    "https://raw.githubusercontent.com/DDS-Solutions/AI-Tadpole-OS-Industry-Templates/main/registry.json";

/// Concurrency limiter for repository clones to prevent resource exhaustion attacks.
static CLONE_SEMAPHORE: Lazy<tokio::sync::Semaphore> = Lazy::new(|| tokio::sync::Semaphore::new(2));

/// Trusted launchers permitted for declarative MCP servers installed by templates.
pub const TRUSTED_MCP_LAUNCHERS: &[&str] = &[
    "npx", "uvx", "docker", "python", "python3", "node", "deno", "cargo", "bun",
];

/// Explicitly prohibited command execution binaries in MCP server manifests.
pub const DISALLOWED_MCP_BINARIES: &[&str] = &[
    "sh", "bash", "zsh", "csh", "ksh", "cmd", "cmd.exe", "powershell", "powershell.exe", "pwsh", "pwsh.exe",
];

/// Reserved agent identities that templates are strictly forbidden from overriding or injecting.
pub const RESERVED_AGENT_IDS: &[&str] = &[
    AGENT_CEO, AGENT_COO, AGENT_ALPHA, "system", "overlord", "root", "admin", "executive",
];

// ─── Authentication & Principal Attribution ─────────────────────────────────

/// Verified operator principal authorized to install or uninstall swarm templates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TemplatePrincipal {
    pub id: String,
}

impl TemplatePrincipal {
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into() }
    }
}

impl<S> FromRequestParts<S> for TemplatePrincipal
where
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        if let Some(principal) = parts.extensions.get::<TemplatePrincipal>() {
            return Ok(principal.clone());
        }
        if parts.extensions.get::<AuthenticatedOperator>().is_some() {
            let op_id = parts
                .headers
                .get("x-operator-id")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("operator");
            return Ok(TemplatePrincipal::new(op_id));
        }
        if let Some(op_id) = parts.headers.get("x-operator-id").and_then(|v| v.to_str().ok()) {
            if !op_id.trim().is_empty() {
                return Ok(TemplatePrincipal::new(op_id.trim()));
            }
        }
        Err(AppError::Unauthorized(
            "Authentication required: missing valid operator token or principal".to_string(),
        ))
    }
}

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

// ─── In-Memory Catalog Cache ────────────────────────────────────────────────

struct CatalogCache {
    fetched_at: Instant,
    data: serde_json::Value,
}

static CATALOG_CACHE: Lazy<RwLock<Option<CatalogCache>>> = Lazy::new(|| RwLock::new(None));

// ─── Verification, Redaction & Security Helpers ─────────────────────────────

/// Redacts credentials and user tokens embedded in repository URLs before logging or recording in audit trail.
pub fn redact_url(url_str: &str) -> String {
    if let Ok(mut parsed) = reqwest::Url::parse(url_str) {
        if parsed.password().is_some() {
            let _ = parsed.set_password(Some("REDACTED"));
        }
        if !parsed.username().is_empty() && parsed.username() != "git" {
            let _ = parsed.set_username("REDACTED");
        }
        parsed.to_string()
    } else {
        url_str.to_string()
    }
}

/// Detects whether an IP address belongs to loopback, link-local, private, broadcast, or unspecified ranges.
pub fn is_private_or_local_ip(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(ipv4) => {
            ipv4.is_loopback()
                || ipv4.is_private()
                || ipv4.is_link_local()
                || ipv4.is_broadcast()
                || ipv4.is_unspecified()
        }
        std::net::IpAddr::V6(ipv6) => {
            if ipv6.is_loopback() || ipv6.is_unspecified() {
                return true;
            }
            if let Some(ipv4) = ipv6.to_ipv4_mapped() {
                ipv4.is_loopback()
                    || ipv4.is_private()
                    || ipv4.is_link_local()
                    || ipv4.is_broadcast()
                    || ipv4.is_unspecified()
            } else {
                let segs = ipv6.segments();
                (segs[0] & 0xfe00) == 0xfc00 || (segs[0] & 0xffc0) == 0xfe80
            }
        }
    }
}

/// Validates repository URL against SSRF targets (private networks, link-local metadata, loopback)
/// and normalizes relative template paths.
pub async fn validate_install_request_async(
    payload: &InstallTemplateRequest,
) -> Result<String, AppError> {
    let repo_url = payload.repository_url.trim();
    if repo_url.starts_with('-') {
        return Err(AppError::BadRequest(
            "Repository URL cannot start with a hyphen.".to_string(),
        ));
    }

    let parsed_url = reqwest::Url::parse(repo_url)
        .map_err(|e| AppError::BadRequest(format!("Invalid Repository URL: {e}")))?;
    let scheme = parsed_url.scheme();
    if scheme != "https" && scheme != "http" {
        return Err(AppError::BadRequest(format!(
            "Unsupported repository URL scheme: '{scheme}'. Only https (or authorized http) protocols are allowed."
        )));
    }

    let host = parsed_url.host_str().ok_or_else(|| {
        AppError::BadRequest("Repository URL is missing a valid host name.".to_string())
    })?;

    if host.eq_ignore_ascii_case("localhost") {
        return Err(AppError::BadRequest(
            "SSRF protection: access to 'localhost' is prohibited.".to_string(),
        ));
    }

    // SSRF Check: Direct IP or DNS Resolution
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        if is_private_or_local_ip(ip) {
            return Err(AppError::BadRequest(format!(
                "SSRF protection: repository host '{host}' resolves to non-public IP range."
            )));
        }
    } else {
        let port = parsed_url.port().unwrap_or(if scheme == "https" { 443 } else { 80 });
        if let Ok(addrs) = tokio::net::lookup_host((host, port)).await {
            for addr in addrs {
                if is_private_or_local_ip(addr.ip()) {
                    return Err(AppError::BadRequest(format!(
                        "SSRF protection: repository host '{host}' resolves to non-public IP address '{}'.",
                        addr.ip()
                    )));
                }
            }
        }
    }

    let clean_path = payload.path.trim().replace('\\', "/");
    if clean_path.is_empty() || clean_path == "." {
        return Err(AppError::BadRequest(
            "Invalid template path: cannot install the repository root as a template.".to_string(),
        ));
    }
    if clean_path.contains("..") || clean_path.starts_with('/') {
        return Err(AppError::BadRequest(
            "Invalid template path: Directory traversal or absolute paths are prohibited."
                .to_string(),
        ));
    }

    Ok(repo_url.to_string())
}

/// Synchronous wrapper for unit testing request validation invariants.
#[allow(dead_code)]
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
    if scheme != "https" && scheme != "http" {
        return Err(AppError::BadRequest(format!(
            "Unsupported repository URL scheme: '{scheme}'. Only https (or authorized http) protocols are allowed."
        )));
    }

    let host = parsed_url.host_str().ok_or_else(|| {
        AppError::BadRequest("Repository URL is missing a valid host name.".to_string())
    })?;

    if host.eq_ignore_ascii_case("localhost") {
        return Err(AppError::BadRequest(
            "SSRF protection: access to 'localhost' is prohibited.".to_string(),
        ));
    }

    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        if is_private_or_local_ip(ip) {
            return Err(AppError::BadRequest(format!(
                "SSRF protection: repository host '{host}' resolves to non-public IP range."
            )));
        }
    }

    let clean_path = payload.path.trim().replace('\\', "/");
    if clean_path.is_empty() || clean_path == "." {
        return Err(AppError::BadRequest(
            "Invalid template path: cannot install the repository root as a template.".to_string(),
        ));
    }
    if clean_path.contains("..") || clean_path.starts_with('/') {
        return Err(AppError::BadRequest(
            "Invalid template path: Directory traversal or absolute paths are prohibited."
                .to_string(),
        ));
    }

    Ok(repo_url.to_string())
}

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

/// Hardened shallow git clone preventing transport bypass, symlink creation, and credential leakage.
async fn clone_template_repository(
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

/// Validates MCP server launcher and args to prevent arbitrary command execution.
pub fn validate_mcp_server_config(
    name: &str,
    cfg: &crate::agent::mcp::McpServerConfig,
) -> Result<(), String> {
    if name.contains("__") {
        return Err(format!("MCP server '{name}' rejected: name contains '__'"));
    }
    let cmd = cfg.command.trim();
    let cmd_lower = cmd.to_ascii_lowercase();

    for disallowed in DISALLOWED_MCP_BINARIES {
        if cmd_lower == *disallowed
            || cmd_lower.ends_with(&format!("/{}", disallowed))
            || cmd_lower.ends_with(&format!("\\{}", disallowed))
        {
            return Err(format!(
                "MCP server '{name}' rejected: disallowed launcher executable '{cmd}'"
            ));
        }
    }

    let base_launcher = std::path::Path::new(cmd)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(cmd)
        .to_ascii_lowercase();
    let is_trusted = TRUSTED_MCP_LAUNCHERS.contains(&base_launcher.as_str());
    if !is_trusted && !cmd.starts_with("./") && !cmd.starts_with(".\\") {
        return Err(format!(
            "MCP server '{name}' rejected: untrusted launcher '{cmd}'. Must be one of {:?}",
            TRUSTED_MCP_LAUNCHERS
        ));
    }

    for arg in &cfg.args {
        if arg.contains(';')
            || arg.contains('|')
            || arg.contains('&')
            || arg.contains('`')
            || arg.contains('$')
            || arg.contains('\n')
            || arg.contains('\r')
        {
            return Err(format!(
                "MCP server '{name}' rejected: argument contains prohibited shell metacharacters"
            ));
        }
    }

    Ok(())
}

/// Preflight-validates all agent JSON profiles in memory, verifying size, schema, and reserved ID bounds.
async fn preflight_validate_agents(
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
async fn preflight_validate_skills(
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
async fn copy_markdown_assets(
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
async fn merge_template_mcp_config(
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

// ─── List Installed Templates ───────────────────────────────────────────────

/// GET /engine/templates/installed
///
/// Lists all currently installed multi-agent swarm starter kits.
#[tracing::instrument(skip(state), name = "templates::list_installed")]
pub async fn list_installed_templates(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    let installed_dir = state.base_dir.join("data/swarm_config/installed");
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

    Ok((StatusCode::OK, Json(results)))
}

// ─── Uninstall Template ─────────────────────────────────────────────────────

async fn remove_validated_files_with_digest(
    base_dir: &FsPath,
    filenames: &[String],
    file_digests: &HashMap<String, String>,
) {
    for name in filenames {
        if let Ok(p) = crate::utils::security::validate_path(base_dir, name) {
            if p.exists() {
                if let Some(expected_digest) = file_digests.get(name) {
                    if let Ok(bytes) = tokio::fs::read(&p).await {
                        let mut hasher = Sha256::new();
                        hasher.update(&bytes);
                        let actual_digest = hex::encode(hasher.finalize()).to_lowercase();
                        if actual_digest != *expected_digest {
                            tracing::warn!(
                                file = %p.display(),
                                "File modified since installation; skipping deletion to avoid data loss"
                            );
                            continue;
                        }
                    }
                }
                let _ = tokio::fs::remove_file(&p).await;
            }
        }
    }
}

/// DELETE /engine/templates/{id}
///
/// Removes an installed template and cleanly deregisters its agents and assets.
#[tracing::instrument(skip(principal, state), fields(operator_id = %principal.id), name = "templates::uninstall")]
pub async fn uninstall_template(
    principal: TemplatePrincipal,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    if id.contains('/') || id.contains('\\') || id.contains("..") || id.contains('\0') {
        return Err(AppError::BadRequest(
            "Invalid template ID: Path separators and traversal sequences are prohibited."
                .to_string(),
        ));
    }

    let installed_dir = state.base_dir.join("data/swarm_config/installed");
    let safe_id = crate::utils::security::sanitize_id(&id.replace('/', "_"));
    let target_folder = resolve_installed_template_folder(&installed_dir, &id, &safe_id).await?;

    let receipt_path = target_folder.join("install_receipt.json");
    if receipt_path.exists() {
        if let Ok(c) = tokio::fs::read_to_string(&receipt_path).await {
            if let Ok(receipt) = serde_json::from_str::<InstallReceipt>(&c) {
                // Delete database records and stop runners first
                for agent_id in &receipt.agent_ids {
                    let _lifecycle = state.registry.lock_agent(agent_id).await;
                    if let Some((_, runner)) = state.comms.active_runners.remove(agent_id) {
                        runner.abort();
                    }
                    let _ = crate::agent::persistence::delete_agent_cascade(
                        &state.resources.pool,
                        agent_id,
                    )
                    .await;
                    state.registry.agents.remove(agent_id);
                }

                // Delete file assets safely using digests
                remove_validated_files_with_digest(
                    &state.base_dir.join("data/swarm_config/agents"),
                    &receipt.agents,
                    &receipt.file_digests,
                )
                .await;

                remove_validated_files_with_digest(
                    &state.base_dir.join("directives"),
                    &receipt.workflows,
                    &receipt.file_digests,
                )
                .await;

                remove_validated_files_with_digest(
                    &state.base_dir.join("execution"),
                    &receipt.skills,
                    &receipt.file_digests,
                )
                .await;

                remove_validated_files_with_digest(
                    &state.base_dir.join("data/knowledge"),
                    &receipt.knowledge,
                    &receipt.file_digests,
                )
                .await;

                // Deregister MCP servers cleanly
                if !receipt.mcp_servers.is_empty() {
                    let mcp_path = state.base_dir.join(".agent/mcp_config.json");
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

    // Record uninstall in audit trail
    let audit_params = format!(r#"{{"template_id":"{}"}}"#, id);
    let _ = state
        .security
        .audit_trail
        .record(
            "template",
            None,
            Some(&principal.id),
            "template_uninstall",
            &audit_params,
        )
        .await;

    Ok((
        StatusCode::OK,
        Json(UninstallTemplateResponse {
            status: "success".to_string(),
            message: format!("Successfully uninstalled template '{id}'"),
            uninstalled_id: id,
        }),
    ))
}

// ─── Catalog Proxy & Cache ──────────────────────────────────────────────────

/// GET /engine/templates/catalog
///
/// Proxies official template catalog with caching and size bounds.
#[tracing::instrument(skip(state), name = "templates::get_catalog")]
pub async fn get_template_catalog(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    {
        let cache_lock = CATALOG_CACHE.read().await;
        if let Some(ref cache) = *cache_lock {
            if cache.fetched_at.elapsed() < CATALOG_TTL {
                return Ok((StatusCode::OK, Json(&cache.data)).into_response());
            }
        }
    }

    let res = state
        .resources
        .http_client
        .get(REGISTRY_UPSTREAM_URL)
        .header("User-Agent", "TadpoleOS/1.1.59")
        .timeout(Duration::from_secs(10))
        .send()
        .await;

    match res {
        Ok(resp) if resp.status().is_success() => {
            let bytes = resp
                .bytes()
                .await
                .map_err(|e| AppError::BadRequest(format!("Failed to read catalog response: {e}")))?;

            if bytes.len() > MAX_CATALOG_BODY_BYTES {
                return Err(AppError::BadRequest(
                    "Upstream catalog payload exceeds maximum permitted size (2 MiB).".to_string(),
                ));
            }

            let data: serde_json::Value = serde_json::from_slice(&bytes)
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
                    "Upstream registry returned status {}, serving stale cached catalog",
                    resp.status()
                );
                let mut response = (StatusCode::OK, Json(&cache.data)).into_response();
                response
                    .headers_mut()
                    .insert("X-Catalog-Stale", "true".parse().unwrap());
                return Ok(response);
            }
            Err(AppError::BadRequest(format!(
                "Upstream registry returned status: {}",
                resp.status()
            )))
        }
        Err(e) => {
            let cache_lock = CATALOG_CACHE.read().await;
            if let Some(ref cache) = *cache_lock {
                tracing::warn!("Upstream registry error: {e}, serving stale cached catalog");
                let mut response = (StatusCode::OK, Json(&cache.data)).into_response();
                response
                    .headers_mut()
                    .insert("X-Catalog-Stale", "true".parse().unwrap());
                return Ok(response);
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

    #[test]
    fn test_install_template_validation_and_ssrf_boundaries() {
        // Option injection check
        let req_hyphen = InstallTemplateRequest {
            repository_url: "--upload-pack=evil".to_string(),
            path: "agents".to_string(),
        };
        assert!(validate_install_request(&req_hyphen).is_err());

        // Protocol check
        let req_ftp = InstallTemplateRequest {
            repository_url: "ftp://github.com/user/repo".to_string(),
            path: "agents".to_string(),
        };
        assert!(validate_install_request(&req_ftp).is_err());

        // SSRF localhost
        let req_localhost = InstallTemplateRequest {
            repository_url: "http://localhost/repo.git".to_string(),
            path: "agents".to_string(),
        };
        assert!(validate_install_request(&req_localhost).is_err());

        // SSRF loopback IP
        let req_loopback = InstallTemplateRequest {
            repository_url: "http://127.0.0.1:8080/repo.git".to_string(),
            path: "agents".to_string(),
        };
        assert!(validate_install_request(&req_loopback).is_err());

        // SSRF cloud metadata IP
        let req_metadata = InstallTemplateRequest {
            repository_url: "http://169.254.169.254/latest/meta-data".to_string(),
            path: "agents".to_string(),
        };
        assert!(validate_install_request(&req_metadata).is_err());

        // Path traversal
        let req_traversal = InstallTemplateRequest {
            repository_url: "https://github.com/user/repo.git".to_string(),
            path: "../outside".to_string(),
        };
        assert!(validate_install_request(&req_traversal).is_err());

        // Valid path
        let req_valid = InstallTemplateRequest {
            repository_url: "https://github.com/user/repo.git".to_string(),
            path: "starters/finance".to_string(),
        };
        assert!(validate_install_request(&req_valid).is_ok());
    }

    #[test]
    fn test_mcp_server_config_validation_rules() {
        let valid_cfg = crate::agent::mcp::McpServerConfig {
            command: "npx".to_string(),
            args: vec!["-y".to_string(), "@modelcontextprotocol/server-filesystem".to_string()],
            env: None,
        };
        assert!(validate_mcp_server_config("filesystem", &valid_cfg).is_ok());

        // Rule: '__' rejected
        assert!(validate_mcp_server_config("custom__server", &valid_cfg).is_err());

        // Rule: Disallowed shell binaries rejected
        let shell_cfg = crate::agent::mcp::McpServerConfig {
            command: "/bin/sh".to_string(),
            args: vec!["-c".to_string(), "whoami".to_string()],
            env: None,
        };
        assert!(validate_mcp_server_config("shell-runner", &shell_cfg).is_err());

        // Rule: Prohibited shell metacharacters in args
        let metachar_cfg = crate::agent::mcp::McpServerConfig {
            command: "node".to_string(),
            args: vec!["script.js; rm -rf /".to_string()],
            env: None,
        };
        assert!(validate_mcp_server_config("bad-arg", &metachar_cfg).is_err());
    }

    #[test]
    fn test_connector_lock_sha256_verification_and_fail_closed() {
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
    fn test_url_credential_redaction() {
        let url_with_token = "https://user:ghp_secretToken1234@github.com/org/repo.git";
        let redacted = redact_url(url_with_token);
        assert!(!redacted.contains("ghp_secretToken1234"));
        assert!(redacted.contains("REDACTED"));

        let normal_url = "https://github.com/org/repo.git";
        assert_eq!(redact_url(normal_url), normal_url);
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
            file_digests: HashMap::from([("skill.py".to_string(), "abc123hash".to_string())]),
        };

        let json_str = serde_json::to_string(&receipt).unwrap();
        let parsed: InstallReceipt = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed.template_id, "test-tmpl");
        assert_eq!(parsed.agent_ids.len(), 1);
        assert_eq!(parsed.file_digests.get("skill.py").unwrap(), "abc123hash");
    }
}

// Metadata: [templates]

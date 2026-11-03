//! @docs ARCHITECTURE:Networking
//!
//! ### AI Assist Note
//! **Templates Security & Verification**: SSRF protection, URL credential redaction,
//! SHA-256 skill checksum verification, and declarative MCP launcher validation.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: SSRF target match, directory traversal, checksum mismatch, or untrusted MCP command.
//! - **Telemetry Link**: Search `[templates_security]` in tracing logs.
//! - **Trace Scope**: `server-rs::routes::templates::security`

use sha2::{Digest, Sha256};
use crate::agent::constants::{AGENT_ALPHA, AGENT_CEO, AGENT_COO};
use crate::error::AppError;
use super::models::InstallTemplateRequest;

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
    !crate::security::ssrf_guard::is_public_routable_ip(ip)
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

    if host.eq_ignore_ascii_case("localhost") || host.ends_with(".localhost") || host == "host.docker.internal" {
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
pub fn validate_install_request(payload: &InstallTemplateRequest) -> Result<String, AppError> {
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

    if host.eq_ignore_ascii_case("localhost") || host.ends_with(".localhost") || host == "host.docker.internal" {
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

// Metadata: [templates_security]


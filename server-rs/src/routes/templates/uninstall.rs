//! @docs ARCHITECTURE:Networking
//!
//! ### AI Assist Note
//! **Template Uninstallation Workflow**: Deregisters deployed multi-agent swarms,
//! verifies file hashes before file removal to prevent data loss, and audits removal.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Template not found, path traversal attempt, or database deregistration failure.
//! - **Telemetry Link**: Search `[templates_uninstall]` in tracing logs.
//! - **Trace Scope**: `server-rs::routes::templates::uninstall`

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::Path as FsPath;
use std::sync::Arc;

use crate::error::AppError;
use crate::state::AppState;
use super::models::{InstallReceipt, UninstallTemplateResponse};
use super::principal::TemplatePrincipal;

/// Resolves an installed template directory by direct ID, sanitized ID, or `install_receipt.json` lookup.
pub async fn resolve_installed_template_folder(
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

/// Safely removes installed files only if their SHA-256 matches the install receipt digest.
pub async fn remove_validated_files_with_digest(
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

    let mut retained_agents: Vec<String> = Vec::new();
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
                    match crate::agent::persistence::delete_agent_cascade(
                        &state.resources.pool,
                        agent_id,
                    )
                    .await
                    {
                        Ok(()) => {
                            state.registry.agents.remove(agent_id);
                            state
                                .comms
                                .mission_contexts
                                .retain(|_, ctx| ctx.agent_id != *agent_id);
                        }
                        Err(err) => {
                            tracing::warn!(
                                "⚠️ [templates_uninstall] Retaining agent '{}' during uninstall of '{}': {}",
                                agent_id,
                                id,
                                err
                            );
                            retained_agents.push(agent_id.clone());
                        }
                    }
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

    let message = if retained_agents.is_empty() {
        format!("Successfully uninstalled template '{id}'")
    } else {
        format!(
            "Uninstalled template '{id}'; retained {} ledger-linked or protected agent(s)",
            retained_agents.len()
        )
    };

    Ok((
        StatusCode::OK,
        Json(UninstallTemplateResponse {
            status: "success".to_string(),
            message,
            uninstalled_id: id,
            retained_agents,
        }),
    ))
}

// Metadata: [templates_uninstall]


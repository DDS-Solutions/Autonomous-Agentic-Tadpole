//! @docs ARCHITECTURE:Networking
//!
//! ### AI Assist Note
//! **System Deployment (Update Manager)**: Orchestrates the live
//! updating and deployment of engine components for the AI-Tadpole-OS
//! ecosystem. Features **Multi-Bunker Support**: allows
//! targeting specific deployment environments (Bunker 1/2) via the
//! `target` query parameter. Implements **Secure Pipeline
//! Execution**: triggers authenticated PowerShell deployment
//! scripts (`deploy-bunker-X.ps1`) with real-time stdout/stderr
//! capture. AI agents should use this endpoint to initiate
//! production releases or environment-wide state migrations after
//! passing all pre-flight audits (DEP-01).
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: 401 Unauthorized for invalid `NEURAL_TOKEN`,
//!   500 Internal Server Error for missing deployment scripts, or
//!   script timeouts during resource-intensive build processes.
//! - **Telemetry Link**: Search `[deploy]` in tracing logs.
//!   `✅ Deployment succeeded` in `tracing` logs for audit history.
//! - **Trace Scope**: `server-rs::routes::deploy`

use crate::error::AppError;
use crate::state::AppState;

use axum::extract::Query;
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Standardized response for engine deployment operations.
#[derive(Serialize)]
pub struct DeployResponse {
    /// Operational status (e.g., "success", "error", "unauthorized").
    pub status: String,
    /// Captured stdout from the deployment script.
    pub output: Option<String>,
    /// Captured stderr or internal error message.
    pub error: Option<String>,
}

/// Request parameters for targeting specific deployment bunkers.
#[derive(Deserialize, Debug)]
pub struct DeployParams {
    /// Deployment target index (1 or 2).
    pub target: Option<u8>,
}

/// POST /engine/deploy — Triggers the deployment pipeline.
///
/// **Query Params**: `target` (1 or 2). Defaults to 1.
///
/// **Security**: Requires a valid `Authorization: Bearer <NEURAL_TOKEN>` header.
#[tracing::instrument(skip(state, headers), name = "system::deploy")]
pub async fn trigger_deploy(
    State(state): State<Arc<AppState>>,
    Query(params): Query<DeployParams>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AppError> {
    // --- Authentication Gate ---
    let token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");

    if crate::middleware::auth::match_token(token, &state).is_none() {
        tracing::warn!("🚫 Unauthorized deploy attempt blocked.");
        return Ok((
            StatusCode::UNAUTHORIZED,
            Json(DeployResponse {
                status: "unauthorized".to_string(),
                output: None,
                error: Some("Missing or invalid Authorization header.".to_string()),
            }),
        ));
    }

    let target = params.target.unwrap_or(1);
    let script_file = match target {
        1 => "deploy-bunker-1.ps1",
        2 => "deploy-bunker-2.ps1",
        _ => {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(DeployResponse {
                    status: "error".to_string(),
                    output: None,
                    error: Some(format!(
                        "Invalid deployment target: {}. Must be 1 or 2.",
                        target
                    )),
                }),
            ));
        }
    };

    let (script_name, is_powershell) = if cfg!(windows) {
        (format!("deploy-bunker-{}.ps1", target), true)
    } else {
        // On Linux / Unix, prefer .sh script, but support .ps1 via pwsh if installed
        let sh_name = format!("deploy-bunker-{}.sh", target);
        let sh_exists = std::path::Path::new(&sh_name).exists()
            || std::path::Path::new("..").join(&sh_name).exists()
            || std::path::Path::new("scripts").join(&sh_name).exists()
            || std::path::Path::new("..").join("scripts").join(&sh_name).exists();
        if sh_exists {
            (sh_name, false)
        } else {
            (format!("deploy-bunker-{}.ps1", target), true)
        }
    };

    tracing::info!(
        "🚀 Authenticated deploy triggered for Bunker {} ({})...",
        target,
        script_name
    );

    // --- Path Resolution & Context Switching ---
    let mut project_root_dir = None;
    let resolved_script_path = if std::path::Path::new(&script_name).exists() {
        script_name.clone()
    } else if std::path::Path::new("..").join(&script_name).exists() {
        project_root_dir = Some("..");
        script_name.clone()
    } else if std::path::Path::new("scripts").join(&script_name).exists() {
        format!("scripts/{}", script_name)
    } else if std::path::Path::new("..").join("scripts").join(&script_name).exists() {
        project_root_dir = Some("..");
        format!("scripts/{}", script_name)
    } else {
        tracing::error!("❌ Deployment script not found in . or ..: {}", script_name);
        return Ok((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(DeployResponse {
                status: "error".to_string(),
                output: None,
                error: Some(format!("Deployment script not found: {}", script_name)),
            }),
        ));
    };

    let mut cmd = if is_powershell {
        if cfg!(windows) {
            let mut c = tokio::process::Command::new("powershell.exe");
            c.args(["-ExecutionPolicy", "Bypass", "-File", &resolved_script_path]);
            c
        } else {
            // Linux PowerShell: require pwsh
            if which::which("pwsh").is_err() {
                return Ok((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(DeployResponse {
                        status: "error".to_string(),
                        output: None,
                        error: Some(format!(
                            "PowerShell script '{}' requires 'pwsh' on Linux, which is not installed. Deploy via shell script (.sh) instead.",
                            script_name
                        )),
                    }),
                ));
            }
            let mut c = tokio::process::Command::new("pwsh");
            c.args(["-File", &resolved_script_path]);
            c
        }
    } else {
        let mut c = tokio::process::Command::new("sh");
        c.arg(&resolved_script_path);
        c
    };
    cmd.kill_on_drop(true);

    if let Some(dir) = project_root_dir {
        tracing::info!("📂 Script found in root/subpath. Switching to project root ({}) for deployment execution.", dir);
        cmd.current_dir(dir);
    }

    // --- Async Process Execution bounded within Tower TimeoutLayer (120s envelope) ---
    let result = tokio::time::timeout(std::time::Duration::from_secs(110), cmd.output()).await;

    match result {
        Ok(Ok(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();

            if output.status.success() {
                tracing::info!("✅ Deployment succeeded.");
                if !stderr.is_empty() {
                    tracing::warn!("⚠️ Deployment stderr:\n{}", stderr);
                }
                Ok((
                    StatusCode::OK,
                    Json(DeployResponse {
                        status: "success".to_string(),
                        output: Some(stdout),
                        error: None,
                    }),
                ))
            } else {
                let combined = format!("{}\n{}", stdout, stderr).trim().to_string();
                let error_msg = if combined.is_empty() {
                    format!(
                        "{} exited with code {:?}",
                        script_file,
                        output.status.code()
                    )
                } else {
                    combined
                };
                Ok((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(DeployResponse {
                        status: "error".to_string(),
                        output: Some(stdout),
                        error: Some(error_msg),
                    }),
                ))
            }
        }
        Ok(Err(e)) => {
            tracing::error!("❌ Failed to execute deployment script: {}", e);
            Ok((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(DeployResponse {
                    status: "error".to_string(),
                    output: None,
                    error: Some(format!("Failed to execute deployment script: {}", e)),
                }),
            ))
        }
        Err(_elapsed) => {
            tracing::error!("⏱️ Deployment script timed out after 300 seconds");
            Ok((
                StatusCode::GATEWAY_TIMEOUT,
                Json(DeployResponse {
                    status: "error".to_string(),
                    output: None,
                    error: Some("Deployment execution timed out after 300 seconds.".to_string()),
                }),
            ))
        }
    }
}

/// POST /engine/pre-pr — Triggers the Pre-PR Quality Gate checks.
///
/// **Security**: Requires a valid `Authorization: Bearer <NEURAL_TOKEN>` header.
#[tracing::instrument(skip(state, headers), name = "system::pre_pr")]
pub async fn trigger_pre_pr(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AppError> {
    // --- Authentication Gate ---
    let token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");

    if crate::middleware::auth::match_token(token, &state).is_none() {
        tracing::warn!("🚫 Unauthorized pre-pr attempt blocked.");
        return Ok((
            StatusCode::UNAUTHORIZED,
            Json(DeployResponse {
                status: "unauthorized".to_string(),
                output: None,
                error: Some("Missing or invalid Authorization header.".to_string()),
            }),
        ));
    }

    tracing::info!("🚀 Authenticated pre-pr triggered...");

    let mut cmd = tokio::process::Command::new("python");
    cmd.arg("execution/pre_pr.py");
    cmd.kill_on_drop(true);

    if !std::path::Path::new("execution/pre_pr.py").exists() {
        if std::path::Path::new("../execution/pre_pr.py").exists() {
            cmd.current_dir("..");
        } else {
            tracing::error!("❌ Pre-PR script not found in . or ..");
            return Ok((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(DeployResponse {
                    status: "error".to_string(),
                    output: None,
                    error: Some("Pre-PR Quality Gate script not found.".to_string()),
                }),
            ));
        }
    }

    let result = tokio::time::timeout(std::time::Duration::from_secs(110), cmd.output()).await;

    match result {
        Ok(Ok(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();

            if output.status.success() {
                tracing::info!("✅ Pre-PR Gate succeeded.");
                Ok((
                    StatusCode::OK,
                    Json(DeployResponse {
                        status: "success".to_string(),
                        output: Some(stdout),
                        error: None,
                    }),
                ))
            } else {
                let combined = format!("{}\n{}", stdout, stderr).trim().to_string();
                Ok((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(DeployResponse {
                        status: "error".to_string(),
                        output: Some(stdout),
                        error: Some(combined),
                    }),
                ))
            }
        }
        Ok(Err(e)) => {
            tracing::error!("❌ Failed to spawn Pre-PR process: {}", e);
            Ok((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(DeployResponse {
                    status: "error".to_string(),
                    output: None,
                    error: Some(e.to_string()),
                }),
            ))
        }
        Err(_) => {
            tracing::error!("❌ Pre-PR process timed out after 110s");
            Ok((
                StatusCode::GATEWAY_TIMEOUT,
                Json(DeployResponse {
                    status: "error".to_string(),
                    output: None,
                    error: Some("Pre-PR process timed out after 110s".to_string()),
                }),
            ))
        }
    }
}

// Metadata: [deploy]

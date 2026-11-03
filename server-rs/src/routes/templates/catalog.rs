//! @docs ARCHITECTURE:Networking
//!
//! ### AI Assist Note
//! **Template Catalog & Inventory**: Provides cached upstream catalog querying with
//! circuit-breaking stale fallbacks and local inventory listing of installed swarms.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Upstream registry timeout, payload exceeding 2 MiB, or malformed catalog JSON.
//! - **Telemetry Link**: Search `[templates_catalog]` in tracing logs.
//! - **Trace Scope**: `server-rs::routes::templates::catalog`

use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use once_cell::sync::Lazy;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::error::AppError;
use crate::state::AppState;
use super::models::{InstallReceipt, InstalledTemplateSummary};

pub const CATALOG_TTL: Duration = Duration::from_secs(3600); // 1 hour
pub const MAX_CATALOG_BODY_BYTES: usize = 2 * 1024 * 1024; // 2 MiB
pub const REGISTRY_UPSTREAM_URL: &str =
    "https://raw.githubusercontent.com/Autonomous-Agentic-Tadpole/templates-registry/main/catalog.json";

pub struct CatalogCache {
    pub fetched_at: Instant,
    pub data: serde_json::Value,
}

pub static CATALOG_CACHE: Lazy<tokio::sync::RwLock<Option<CatalogCache>>> =
    Lazy::new(|| tokio::sync::RwLock::new(None));

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

// Metadata: [templates_catalog]


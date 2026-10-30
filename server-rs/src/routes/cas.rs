//! @docs ARCHITECTURE:Gateways
//!
//! ### AI Assist Note
//! **cas**: Read-Only Content-Addressable Revision History Route for the Sovereign Observability Core (SOC).
//! Exposes `GET /v1/cas/history` for inspecting file revision timelines without exposing arbitrary disk overwrite vectors.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Path traversal rejection (`AppError::Forbidden` / `AppError::BadRequest`) or SQLite query failure.
//! - **Telemetry Link**: Search `[cas]` in tracing logs.
//! - **Trace Scope**: `server-rs::routes::cas`

use crate::{
    error::AppError,
    services::cas::{self, RevisionSummary},
    state::AppState,
};
use axum::{
    extract::{Query, State},
    Json,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug, Deserialize)]
pub struct HistoryQuery {
    pub workspace_root: Option<String>,
    pub file_path: String,
}

#[derive(Debug, Serialize)]
pub struct CasResponse<T> {
    pub success: bool,
    pub data: T,
}

fn resolve_ws_root(state: &AppState, ws: Option<&str>) -> Result<PathBuf, AppError> {
    match ws {
        Some(w) if !w.trim().is_empty() => {
            let safe_p = crate::utils::security::validate_path(&state.base_dir, w)?;
            Ok(safe_p.to_path_buf())
        }
        _ => Ok(state.base_dir.clone()),
    }
}

/// GET /v1/cas/history
/// Retrieves read-only revision history for a specific workspace file.
#[tracing::instrument(skip(state), name = "cas::get_history")]
pub async fn get_cas_history_handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<HistoryQuery>,
) -> Result<Json<CasResponse<Vec<RevisionSummary>>>, AppError> {
    if query.file_path.trim().is_empty() {
        return Err(AppError::BadRequest(
            "file_path parameter cannot be empty".to_string(),
        ));
    }

    let ws_root = resolve_ws_root(&state, query.workspace_root.as_deref())?;

    // 🛡️ [Path Traversal Defense] Validate file_path relative to ws_root
    let safe_file = crate::utils::security::validate_path(&ws_root, &query.file_path)?;
    let rel_file = safe_file
        .strip_prefix(&ws_root)
        .unwrap_or(safe_file.as_path())
        .to_string_lossy()
        .replace('\\', "/");

    let pool = &state.resources.pool;
    tracing::debug!("[cas] Retrieving revision history for '{}'", rel_file);
    let history = cas::get_file_history(pool, &ws_root, &rel_file).await?;

    Ok(Json(CasResponse {
        success: true,
        data: history,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cas_path_traversal_rejected() {
        let state = Arc::new(AppState::new_minimal_mock().await);
        let q = HistoryQuery {
            workspace_root: None,
            file_path: "../../etc/passwd".to_string(),
        };
        let res = get_cas_history_handler(State(state), Query(q)).await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn test_cas_empty_file_path_rejected() {
        let state = Arc::new(AppState::new_minimal_mock().await);
        let q = HistoryQuery {
            workspace_root: None,
            file_path: "   ".to_string(),
        };
        let res = get_cas_history_handler(State(state), Query(q)).await;
        assert!(matches!(res, Err(AppError::BadRequest(_))));
    }
}

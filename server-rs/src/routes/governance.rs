//! @docs ARCHITECTURE:Governance
//! 
//! ### AI Assist Note
//! **Governance Logic**: API handlers for managing Role Blueprints. 
//! Orchestrates the CRUD operations for standardized agent templates, ensuring 
//! that recruitment blueprints are synchronized with the primary database.
//! 
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Database connection timeouts during blueprint retrieval or unique constraint violations on save.
//! - **Telemetry Link**: Search `[governance]` in server traces.
//!

use crate::agent::types::RoleBlueprint;
use crate::error::AppError;
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use std::sync::Arc;

/// ### ⚖️ Governance: Blueprint Discovery
/// Returns a list of all registered Role Blueprints.
pub async fn list_blueprints(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    let blueprints = crate::agent::persistence::load_blueprints(&state.resources.pool).await?;
    Ok((StatusCode::OK, Json(blueprints)))
}

/// ### ⚖️ Governance: Promote to Role
/// Registers or updates a Role Blueprint in the persistence layer.
pub async fn save_blueprint(
    State(state): State<Arc<AppState>>,
    Json(blueprint): Json<RoleBlueprint>,
) -> Result<impl IntoResponse, AppError> {
    // Basic validation
    if blueprint.id.trim().is_empty() || blueprint.name.trim().is_empty() {
        return Err(AppError::BadRequest(
            "Blueprint ID and Name are required".to_string(),
        ));
    }

    crate::agent::persistence::save_blueprint(&state.resources.pool, &blueprint).await?;
    tracing::info!("✅ [Governance] Role Blueprint '{}' saved successfully", blueprint.id);
    Ok(StatusCode::OK)
}

/// ### ⚖️ Governance: Role Retirement
/// Deletes a Role Blueprint from the system.
pub async fn delete_blueprint(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let deleted = crate::agent::persistence::delete_blueprint(&state.resources.pool, &id).await?;
    if !deleted {
        return Err(AppError::NotFound(format!(
            "Role Blueprint '{}' not found",
            id
        )));
    }
    tracing::warn!("🗑️ [Governance] Role Blueprint '{}' retired.", id);
    Ok(StatusCode::NO_CONTENT)
}

/// ### ⚖️ Governance: System Manifest
/// Generates and returns the latest Sovereign State Manifest.
pub async fn get_sovereign_manifest(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    let manifest = crate::system::manifest::SovereignStateManifest::generate(&state).await;
    Ok((StatusCode::OK, Json(serde_json::json!({ "manifest": manifest }))))
}



// Metadata: [governance]

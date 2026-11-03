//! @docs ARCHITECTURE:Networking
//!
//! ### AI Assist Note
//! **Template Principal & Authentication**: Extracts and validates operator authority
//! for installing or removing multi-agent starter kits.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Missing or invalid operator token in request extensions or headers.
//! - **Telemetry Link**: Search `[templates_principal]` in tracing logs.
//! - **Trace Scope**: `server-rs::routes::templates::principal`

use axum::{extract::FromRequestParts, http::request::Parts};
use crate::error::AppError;
use crate::middleware::auth::AuthenticatedOperator;

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

// Metadata: [templates_principal]


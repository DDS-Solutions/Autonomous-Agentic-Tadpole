//! @docs ARCHITECTURE:Networking
//!
//! ### AI Assist Note
//! **Oversight Principal & Security Guardrails**: Provides server-verified operator principal
//! extraction, mission scoping authorization, input validation boundaries, and recursive redaction.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Operator authorization rejection, unauthorized mission scope, or invalid identifier.
//! - **Telemetry Link**: Search for `[principal]` in tracing logs.
//! - **Trace Scope**: `server-rs::routes::oversight::principal`

use axum::{extract::FromRequestParts, http::request::Parts};
use std::collections::HashSet;

use crate::error::AppError;
use crate::middleware::auth::AuthenticatedOperator;

/// Number of trailing audit records verified by `GET /v1/oversight/security/integrity`.
pub const INTEGRITY_VERIFY_WINDOW: usize = 50;

/// Score multiplier applied when the SHA-256 hash chain is intact but Ed25519 signing is not enabled.
pub const UNSIGNED_CHAIN_SCORE_MULTIPLIER: f64 = 0.5;

/// Upper-bound ceiling for `max_agents` governance setting.
pub const MAX_AGENTS_CEILING: u32 = 10_000;

/// Upper-bound ceiling for `max_clusters` governance setting.
pub const MAX_CLUSTERS_CEILING: u32 = 1_000;

/// Upper-bound ceiling for `max_swarm_depth` governance setting.
pub const MAX_SWARM_DEPTH_CEILING: u32 = 64;

/// Upper-bound ceiling for `max_task_length` governance setting (1 MiB).
pub const MAX_TASK_LENGTH_CEILING: usize = 1_048_576;

/// Upper-bound ceiling for `default_budget_usd` governance setting.
pub const MAX_DEFAULT_BUDGET_USD_CEILING: f64 = 1_000_000.0;

/// Failure count threshold below which a non-suspended agent is considered healthy.
pub const HEALTHY_FAILURE_LIMIT: u32 = 5;

/// Failure count threshold at or above which an agent is flagged as throttled.
pub const THROTTLED_FAILURE_THRESHOLD: u32 = 3;

/// Maximum length for path/body identifier strings (`entry_id`, `entity_id`, `cluster_id`, `tool_name`).
pub const MAX_IDENTIFIER_LEN: usize = 128;

/// Server-verified operator principal for mutating governance and oversight endpoints.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OversightPrincipal {
    /// Verified operator identifier recorded in `oversight_log.decided_by` and `audit_trail.user_id`.
    pub id: String,
    /// Optional mission/cluster scope whitelist. `None` grants global operator authority.
    pub allowed_missions: Option<HashSet<String>>,
}

impl OversightPrincipal {
    /// Constructs a global operator principal with unrestricted mission scope.
    pub fn global(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            allowed_missions: None,
        }
    }

    /// Constructs a mission-scoped principal restricted to specific `mission_id`s.
    pub fn scoped<I, S>(id: impl Into<String>, missions: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            id: id.into(),
            allowed_missions: Some(missions.into_iter().map(Into::into).collect()),
        }
    }

    pub fn ensure_authenticated(&self) -> Result<&str, AppError> {
        let trimmed = self.id.trim();
        if trimmed.is_empty() {
            return Err(AppError::Unauthorized(
                "Authentication required: operator principal ID cannot be empty".to_string(),
            ));
        }
        Ok(trimmed)
    }

    /// Verifies that the principal is authorized for global (cluster-wide) settings/policy mutations.
    pub fn authorize_global(&self) -> Result<&str, AppError> {
        let id = self.ensure_authenticated()?;
        if let Some(ref allowed) = self.allowed_missions {
            if !allowed.contains("*") {
                return Err(AppError::Forbidden(format!(
                    "Principal '{}' is mission-scoped and cannot mutate global governance policy",
                    id
                )));
            }
        }
        Ok(id)
    }

    /// Verifies that the principal has oversight authority for the target `mission_id`.
    pub fn authorize_mission(&self, mission_id: Option<&str>) -> Result<&str, AppError> {
        let id = self.ensure_authenticated()?;
        if let Some(ref allowed) = self.allowed_missions {
            if allowed.contains("*") {
                return Ok(id);
            }
            match mission_id.map(str::trim).filter(|m| !m.is_empty()) {
                Some(mid) if allowed.contains(mid) => Ok(id),
                Some(mid) => Err(AppError::Forbidden(format!(
                    "Principal '{}' is not authorized for mission '{}'",
                    id, mid
                ))),
                None => Err(AppError::Forbidden(format!(
                    "Principal '{}' is mission-scoped and cannot resolve global oversight entries",
                    id
                ))),
            }
        } else {
            Ok(id)
        }
    }
}

impl<S> FromRequestParts<S> for OversightPrincipal
where
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        if let Some(principal) = parts.extensions.get::<OversightPrincipal>() {
            principal.ensure_authenticated()?;
            return Ok(principal.clone());
        }
        if let Some(identity) = parts.extensions.get::<crate::middleware::auth::OperatorIdentity>() {
            if let Some(ref allowed) = identity.allowed_missions {
                return Ok(OversightPrincipal::scoped(&identity.name, allowed.clone()));
            } else {
                return Ok(OversightPrincipal::global(&identity.name));
            }
        }
        if parts.extensions.get::<AuthenticatedOperator>().is_some() {
            return Ok(OversightPrincipal::global("authenticated_operator"));
        }
        Err(AppError::Unauthorized(
            "Authentication required: missing verified operator principal".to_string(),
        ))
    }
}

/// Validates a path or payload identifier (`entry_id`, `entity_id`, `cluster_id`, `tool_name`).
pub fn validate_identifier(raw: &str, field_name: &str) -> Result<String, AppError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(AppError::BadRequest(format!(
            "{} cannot be empty",
            field_name
        )));
    }
    if trimmed.len() > MAX_IDENTIFIER_LEN {
        return Err(AppError::BadRequest(format!(
            "{} exceeds maximum length of {} characters",
            field_name, MAX_IDENTIFIER_LEN
        )));
    }
    if trimmed.chars().any(|c| c.is_control()) {
        return Err(AppError::BadRequest(format!(
            "{} contains invalid control characters",
            field_name
        )));
    }
    Ok(trimmed.to_string())
}

/// Validates that a budget quota is a non-negative finite floating-point number within ceiling bounds.
pub fn validate_quota_budget(budget_usd: f64) -> Result<(), AppError> {
    if !budget_usd.is_finite() || budget_usd < 0.0 || budget_usd > MAX_DEFAULT_BUDGET_USD_CEILING {
        return Err(AppError::BadRequest(
            "Budget quota must be a non-negative finite number within ceiling limits".to_string(),
        ));
    }
    Ok(())
}

/// Validates global governance settings prior to mutating `GovernanceHub` atomics.
pub fn validate_oversight_settings(
    payload: &crate::routes::oversight::settings::OversightSettingsPayload,
) -> Result<(), AppError> {
    if let Some(budget) = payload.default_budget_usd {
        validate_quota_budget(budget)?;
    }
    if let Some(agents) = payload.max_agents {
        if agents == 0 || agents > MAX_AGENTS_CEILING {
            return Err(AppError::BadRequest(format!(
                "max_agents must be between 1 and {}",
                MAX_AGENTS_CEILING
            )));
        }
    }
    if let Some(clusters) = payload.max_clusters {
        if clusters == 0 || clusters > MAX_CLUSTERS_CEILING {
            return Err(AppError::BadRequest(format!(
                "max_clusters must be between 1 and {}",
                MAX_CLUSTERS_CEILING
            )));
        }
    }
    if let Some(depth) = payload.max_swarm_depth {
        if depth == 0 || depth > MAX_SWARM_DEPTH_CEILING {
            return Err(AppError::BadRequest(format!(
                "max_swarm_depth must be between 1 and {}",
                MAX_SWARM_DEPTH_CEILING
            )));
        }
    }
    if let Some(task_len) = payload.max_task_length {
        if task_len == 0 || task_len > MAX_TASK_LENGTH_CEILING {
            return Err(AppError::BadRequest(format!(
                "max_task_length must be between 1 and {}",
                MAX_TASK_LENGTH_CEILING
            )));
        }
    }
    Ok(())
}

/// Validates and normalizes an operator oversight decision token (`"approved"` or `"rejected"`).
pub fn parse_oversight_decision(raw: &str) -> Result<(&'static str, bool), AppError> {
    let trimmed = raw.trim();
    match trimmed.to_ascii_lowercase().as_str() {
        "approved" => Ok(("approved", true)),
        "rejected" => Ok(("rejected", false)),
        _ => {
            let safe_echo: String = trimmed
                .chars()
                .filter(|c| !c.is_control())
                .take(32)
                .collect();
            Err(AppError::BadRequest(format!(
                "Invalid oversight decision '{}': expected 'approved' or 'rejected'",
                safe_echo
            )))
        }
    }
}

pub fn is_sensitive_json_key(key: &str) -> bool {
    let lower = key.trim().to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "api_key"
            | "apikey"
            | "secret"
            | "secret_key"
            | "password"
            | "token"
            | "credential"
            | "credentials"
            | "neural_token"
            | "access_token"
            | "refresh_token"
            | "authorization"
            | "x-api-key"
            | "private_key"
            | "passcode"
            | "passwd"
    ) || lower.ends_with("_api_key")
        || lower.ends_with("_access_token")
        || lower.ends_with("_refresh_token")
        || lower.ends_with("_secret")
        || lower.ends_with("_secret_key")
        || lower.ends_with("_password")
        || lower.ends_with("_passwd")
        || lower.ends_with("_private_key")
        || lower.ends_with("_token")
}

/// Recursively scrubs sensitive keys and secret patterns across a JSON value tree.
pub fn redact_json_value(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::String(s) => {
            *s = crate::utils::security::redact_secrets(s);
        }
        serde_json::Value::Array(arr) => {
            for item in arr.iter_mut() {
                redact_json_value(item);
            }
        }
        serde_json::Value::Object(map) => {
            for (k, v) in map.iter_mut() {
                if is_sensitive_json_key(k) {
                    *v = serde_json::Value::String("[REDACTED]".to_string());
                    continue;
                }
                redact_json_value(v);
            }
        }
        _ => {}
    }
}

// Metadata: [principal]



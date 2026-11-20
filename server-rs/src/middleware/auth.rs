//! @docs ARCHITECTURE:Security
//!
//! ### AI Assist Note
//! **Auth Middleware**: Orchestrates the validation of **Bearer Tokens**
//! for the Tadpole OS engine. Implements **Constant-Time Comparison**
//! (`constant_time_eq`) to mask timing-based side-channel attacks on
//! the `NEURAL_TOKEN`. Supports **Dual-Mechanism Validation**: standard
//! `Authorization` headers for REST, and `Sec-WebSocket-Protocol`
//! (bearer.<token>) for browser-based WebSocket upgrades (AUTH-01).
//!
//! ### 🔍 Debugging & Observability
//! - **Telemetry Link**: Search `[auth]` in tracing logs.
//! - **Failure Path**: Missing `Authorization` header, invalid token
//!   checksum, or malformed WebSocket subprotocol strings. Triggers
//!   `401 Unauthorized`.
//! - **Trace Scope**: `server-rs::middleware::auth`

use crate::error::ProblemDetails;
use crate::AppState;
use axum::{
    body::Body,
    extract::{OriginalUri, State},
    http::{Method, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::sync::Arc;

use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

/// A server-verified operator credential. Client headers and body fields cannot
/// create this request extension; only successful token validation installs it.
#[derive(Clone, Copy, Debug)]
pub struct AuthenticatedOperator;

/// Server-authoritative privilege tier derived exclusively from the verified token.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum OperatorScope {
    /// Restricted to safe read-only HTTP methods (`GET`, `HEAD`, `OPTIONS`) without WS upgrades.
    ReadOnly = 1,
    /// Standard swarm operation (missions, agents, oversight) excluding destructive host/script admin routes.
    Operator = 2,
    /// Full administrative authority including host deploy, engine shutdown/kill, and script/hook mutation.
    Admin = 3,
}

/// Detailed server-verified operator identity with name, privilege scope, and optional mission scopes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OperatorIdentity {
    pub name: String,
    pub scope: OperatorScope,
    pub allowed_missions: Option<std::collections::HashSet<String>>,
}

/// Constant-time string comparison to prevent timing-based side-channel attacks.
///
/// ### 🔒 Security: Constant-Time Comparison (AUTH-01)
/// Compares fixed-length SHA-256 digests using `subtle::ConstantTimeEq`
/// to guarantee execution time is completely independent of the input length,
/// eliminating timing-based side-channel leakage.
pub(crate) fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    let hash_a = Sha256::digest(a);
    let hash_b = Sha256::digest(b);
    hash_a.as_slice().ct_eq(hash_b.as_slice()).into()
}

pub(crate) fn match_token_with_scope(
    token: &str,
    state: &AppState,
) -> Option<(&'static str, OperatorScope)> {
    let has_dedicated_admin = state.security.admin_token.is_some();
    let default_scope = if has_dedicated_admin {
        OperatorScope::Operator
    } else {
        OperatorScope::Admin
    };

    if state
        .security
        .admin_token
        .as_ref()
        .is_some_and(|t| constant_time_eq(token.as_bytes(), t.as_bytes()))
    {
        return Some(("NEURAL_TOKEN_ADMIN", OperatorScope::Admin));
    }

    if constant_time_eq(token.as_bytes(), state.security.deploy_token.as_bytes()) {
        return Some(("NEURAL_TOKEN", default_scope));
    }

    if state
        .security
        .deploy_token_new
        .as_ref()
        .is_some_and(|t| constant_time_eq(token.as_bytes(), t.as_bytes()))
    {
        return Some(("NEURAL_TOKEN_NEW", default_scope));
    }

    if state
        .security
        .deploy_token_old
        .as_ref()
        .is_some_and(|t| constant_time_eq(token.as_bytes(), t.as_bytes()))
    {
        // Enforce server-authoritative grace window. SEC-AUTH-01: Fail closed if rotation metadata is missing.
        if let (Some(rotated_at), Some(grace_secs)) = (
            state.security.token_rotated_at,
            state.security.token_grace_secs,
        ) {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            if now.saturating_sub(rotated_at) > grace_secs {
                tracing::warn!(
                    "[auth] Expired NEURAL_TOKEN_OLD rejected: rotated {}s ago, grace window is {}s",
                    now.saturating_sub(rotated_at),
                    grace_secs
                );
                return None;
            }
            return Some(("NEURAL_TOKEN_OLD", default_scope));
        } else {
            tracing::error!(
                "🚫 [auth] NEURAL_TOKEN_OLD configured but NEURAL_TOKEN_ROTATED_AT or NEURAL_TOKEN_GRACE_SECS is missing. Failing closed to prevent perpetual fallback credentials."
            );
            return None;
        }
    }

    if state
        .security
        .readonly_token
        .as_ref()
        .is_some_and(|t| constant_time_eq(token.as_bytes(), t.as_bytes()))
    {
        return Some(("NEURAL_TOKEN_READONLY", OperatorScope::ReadOnly));
    }

    None
}

#[allow(dead_code)]
pub(crate) fn match_token(token: &str, state: &AppState) -> Option<&'static str> {
    match_token_with_scope(token, state).map(|(name, _)| name)
}

/// Extracts a Bearer token from an HTTP `Authorization` header per RFC 7235
/// (case-insensitive `Bearer` auth scheme, e.g. `Bearer `, `bearer `, `BEARER `).
pub(crate) fn extract_bearer_token(header: &str) -> Option<&str> {
    let trimmed = header.trim();
    if trimmed.len() >= 7 && trimmed[..7].eq_ignore_ascii_case("bearer ") {
        let token = trimmed[7..].trim();
        if !token.is_empty() {
            return Some(token);
        }
    }
    None
}

/// Returns the minimum `OperatorScope` required for a given HTTP method, path, and upgrade flag.
pub(crate) fn required_scope_for_request(
    method: &Method,
    raw_path: &str,
    is_ws_upgrade: bool,
) -> OperatorScope {
    let path = raw_path.strip_prefix("/v1").unwrap_or(raw_path);
    let is_mutating = !matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS);

    let is_admin_host_route = matches!(
        path,
        "/engine/deploy" | "/engine/pre-pr" | "/engine/kill" | "/engine/shutdown" | "/engine/templates/install"
    ) || (is_mutating && path.starts_with("/engine/templates/"));

    let is_admin_skill_mutation = is_mutating
        && (path.starts_with("/skills/scripts/")
            || path.starts_with("/skills/workflows/")
            || path.starts_with("/skills/hooks/")
            || path == "/skills/register"
            || path == "/skills/promote"
            || (path.starts_with("/skills/proposals/") && path.ends_with("/resolve"))
            || (path.starts_with("/skills/mcp-tools/") && path.ends_with("/execute")));

    let is_admin_governance_mutation = is_mutating
        && (path == "/oversight/settings"
            || path.starts_with("/oversight/security/")
            || path.starts_with("/model-manager/")
            || path == "/api/pull"
            || path.starts_with("/governance/blueprints")
            || path.starts_with("/continuity/")
            || path == "/mcp/message"
            || (path.starts_with("/agents/")
                && path.contains("/skills/")
                && path.ends_with("/approve")));

    let is_stateful_stream = is_ws_upgrade || path == "/mcp/sse";

    if is_admin_host_route || is_admin_skill_mutation || is_admin_governance_mutation {
        OperatorScope::Admin
    } else if is_mutating || is_stateful_stream {
        OperatorScope::Operator
    } else {
        OperatorScope::ReadOnly
    }
}

fn scope_forbidden_response(
    token_name: &str,
    actual: OperatorScope,
    required: OperatorScope,
) -> Response {
    tracing::warn!(
        token = token_name,
        ?actual,
        ?required,
        "🚫 [Auth] Insufficient operator scope for request"
    );
    ProblemDetails::new(
        StatusCode::FORBIDDEN,
        "Insufficient Scope",
        format!(
            "Token '{}' has {:?} scope, but this endpoint requires {:?} scope.",
            token_name, actual, required
        ),
    )
    .into_response()
}

/// Middleware to validate the Bearer token and enforce server-authoritative `OperatorScope`.
/// Supports two mechanisms:
/// 1. Standard `Authorization: Bearer <token>` header (REST endpoints)
/// 2. `Sec-WebSocket-Protocol: bearer.<token>` header (browser WebSocket upgrades)
pub async fn validate_token(
    State(state): State<Arc<AppState>>,
    mut req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let raw_path = req
        .extensions()
        .get::<OriginalUri>()
        .map(|u| u.0.path().to_string())
        .unwrap_or_else(|| req.uri().path().to_string());

    // PUBLIC BYPASS: Allow health endpoint to bypass global auth.
    if raw_path == "/v1/engine/health" || raw_path == "/engine/health" {
        return Ok(next.run(req).await);
    }

    let is_ws_upgrade = req
        .headers()
        .get(axum::http::header::UPGRADE)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.eq_ignore_ascii_case("websocket"))
        .unwrap_or(false);

    let required_scope = required_scope_for_request(req.method(), &raw_path, is_ws_upgrade);
    let client_ip = crate::middleware::extract_client_ip(&req);
    let mut token_opt = None;

    // Check for standard Authorization header first (RFC 7235: case-insensitive scheme)
    let auth_header = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|val| val.to_str().ok());

    if let Some(auth_str) = auth_header {
        if let Some(token) = extract_bearer_token(auth_str) {
            token_opt = Some(token.to_string());
        }
    }

    if let Some(ref token) = token_opt {
        if let Some((token_name, scope)) = match_token_with_scope(token, &state) {
            tracing::debug!("🔑 [Auth] Authorized request using {} ({:?})", token_name, scope);
            crate::middleware::auth_rate_limit::record_auth_success(&client_ip);
            if scope < required_scope {
                return Ok(scope_forbidden_response(token_name, scope, required_scope));
            }
            req.extensions_mut().insert(OperatorIdentity {
                name: token_name.to_string(),
                scope,
                allowed_missions: None,
            });
            req.extensions_mut().insert(AuthenticatedOperator);
            return Ok(next.run(req).await);
        } else {
            tracing::warn!("🚫 Invalid token provided");
            return Err(StatusCode::UNAUTHORIZED);
        }
    }

    // Fallback: check Sec-WebSocket-Protocol for browser WS connections
    // Browsers cannot set Authorization headers on WebSocket upgrade requests,
    // so the frontend sends the token as a subprotocol: "bearer.<token>"
    if is_ws_upgrade {
        let proto_header = req
            .headers()
            .get("sec-websocket-protocol")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        // SEC-01: Split the comma-separated list of protocols
        // Browsers often combine multiple subprotocols in one header
        for protocol in proto_header.split(',') {
            let protocol = protocol.trim();
            if let Some(token) = protocol.strip_prefix("bearer.") {
                if let Some((token_name, scope)) = match_token_with_scope(token, &state) {
                    tracing::info!("🔑 [Auth] Authorized WebSocket request using {} ({:?})", token_name, scope);
                    crate::middleware::auth_rate_limit::record_auth_success(&client_ip);
                    if scope < required_scope {
                        return Ok(scope_forbidden_response(token_name, scope, required_scope));
                    }
                    req.extensions_mut().insert(OperatorIdentity {
                        name: token_name.to_string(),
                        scope,
                        allowed_missions: None,
                    });
                    req.extensions_mut().insert(AuthenticatedOperator);
                    return Ok(next.run(req).await);
                }
            }
        }
        let redacted_protocols: Vec<String> = proto_header
            .split(',')
            .map(|p| {
                let p = p.trim();
                if p.starts_with("bearer.") {
                    "bearer.[REDACTED]".to_string()
                } else {
                    p.to_string()
                }
            })
            .collect();
        tracing::warn!(protocols = ?redacted_protocols, "🚫 Unauthorized WebSocket upgrade: invalid or missing protocol token");
    } else {
        tracing::warn!("🚫 Missing or malformed Authorization header");
    }

    Err(StatusCode::UNAUTHORIZED)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode, header},
        middleware::from_fn_with_state,
        routing::get,
        Router,
    };
    use tower::ServiceExt;

    async fn dummy_handler(axum::Extension(_operator): axum::Extension<AuthenticatedOperator>) -> StatusCode {
        StatusCode::OK
    }

    #[tokio::test]
    async fn test_auth_bearer_success() {
        let state = Arc::new(AppState::new_minimal_mock().await);
        let app = Router::new()
            .route("/", get(dummy_handler))
            .layer(from_fn_with_state(state, validate_token));

        let req = Request::builder()
            .uri("/")
            .header(header::AUTHORIZATION, "Bearer test-token")
            .body(Body::empty())
            .unwrap();

        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_auth_bearer_invalid() {
        let state = Arc::new(AppState::new_minimal_mock().await);
        let app = Router::new()
            .route("/", get(dummy_handler))
            .layer(from_fn_with_state(state, validate_token));

        let req = Request::builder()
            .uri("/")
            .header(header::AUTHORIZATION, "Bearer wrong-token")
            .body(Body::empty())
            .unwrap();

        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_auth_websocket_protocol_success() {
        let state = Arc::new(AppState::new_minimal_mock().await);
        let app = Router::new()
            .route("/", get(dummy_handler))
            .layer(from_fn_with_state(state, validate_token));

        let req = Request::builder()
            .uri("/")
            .header(header::UPGRADE, "websocket")
            .header("sec-websocket-protocol", "bearer.test-token")
            .body(Body::empty())
            .unwrap();

        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_auth_websocket_protocol_malformed() {
        let state = Arc::new(AppState::new_minimal_mock().await);
        let app = Router::new()
            .route("/", get(dummy_handler))
            .layer(from_fn_with_state(state, validate_token));

        // Missing "bearer." prefix
        let req = Request::builder()
            .uri("/")
            .header(header::UPGRADE, "websocket")
            .header("sec-websocket-protocol", "test-token")
            .body(Body::empty())
            .unwrap();

        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_auth_missing_header() {
        let state = Arc::new(AppState::new_minimal_mock().await);
        let app = Router::new()
            .route("/", get(dummy_handler))
            .layer(from_fn_with_state(state, validate_token));

        let req = Request::builder()
            .uri("/")
            .header("x-tadpole-role", "admin")
            .body(Body::empty())
            .unwrap();

        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_tiered_operator_scope_enforcement() {
        let mut base_state = AppState::new_minimal_mock().await;
        let sec = crate::state::hubs::sec::SecurityHub {
            audit_trail: base_state.security.audit_trail.clone(),
            budget_guard: base_state.security.budget_guard.clone(),
            shell_scanner: base_state.security.shell_scanner.clone(),
            secret_redactor: base_state.security.secret_redactor.clone(),
            system_monitor: base_state.security.system_monitor.clone(),
            permission_policy: base_state.security.permission_policy.clone(),
            verification_gate: base_state.security.verification_gate.clone(),
            deploy_token: "operator-token-secret".to_string(),
            admin_token: Some("admin-token-secret".to_string()),
            readonly_token: Some("readonly-token-secret".to_string()),
            deploy_token_old: None,
            deploy_token_new: None,
            token_rotated_at: None,
            token_grace_secs: None,
            conflict: base_state.security.conflict.clone(),
        };
        base_state.security = Arc::new(sec);
        let state = Arc::new(base_state);

        let app = Router::new()
            .route("/v1/intelligence/graph", get(dummy_handler))
            .route("/v1/agents", axum::routing::post(dummy_handler))
            .route("/v1/mcp/sse", get(dummy_handler))
            .route("/v1/engine/deploy", axum::routing::post(dummy_handler))
            .route("/v1/engine/templates/install", axum::routing::post(dummy_handler))
            .route("/v1/engine/templates/demo", axum::routing::delete(dummy_handler))
            .route("/v1/skills/hooks/test", axum::routing::put(dummy_handler))
            .route("/v1/skills/promote", axum::routing::post(dummy_handler))
            .route("/v1/skills/proposals/p1/resolve", axum::routing::post(dummy_handler))
            .route("/v1/skills/mcp-tools/shell/execute", axum::routing::post(dummy_handler))
            .route("/v1/oversight/security/policies", axum::routing::put(dummy_handler))
            .route("/v1/oversight/security/missions/m1/quota", axum::routing::put(dummy_handler))
            .route("/v1/oversight/settings", axum::routing::put(dummy_handler))
            .route("/v1/model-manager/providers/openai/test", axum::routing::post(dummy_handler))
            .route("/v1/api/pull", axum::routing::post(dummy_handler))
            .route("/v1/mcp/message", axum::routing::post(dummy_handler))
            .layer(from_fn_with_state(state.clone(), validate_token));

        // 1. ReadOnly token: allowed on GET (including RFC 7235 case-insensitive BEARER/bearer), forbidden on POST & SSE
        for scheme in ["Bearer", "bearer", "BEARER"] {
            let ro_get = Request::builder()
                .uri("/v1/intelligence/graph")
                .header(header::AUTHORIZATION, format!("{} readonly-token-secret", scheme))
                .body(Body::empty())
                .unwrap();
            assert_eq!(app.clone().oneshot(ro_get).await.unwrap().status(), StatusCode::OK);
        }

        let ro_post = Request::builder()
            .method(Method::POST)
            .uri("/v1/agents")
            .header(header::AUTHORIZATION, "Bearer readonly-token-secret")
            .body(Body::empty())
            .unwrap();
        assert_eq!(app.clone().oneshot(ro_post).await.unwrap().status(), StatusCode::FORBIDDEN);

        let ro_sse = Request::builder()
            .method(Method::GET)
            .uri("/v1/mcp/sse")
            .header(header::AUTHORIZATION, "Bearer readonly-token-secret")
            .body(Body::empty())
            .unwrap();
        assert_eq!(app.clone().oneshot(ro_sse).await.unwrap().status(), StatusCode::FORBIDDEN);

        // 2. Operator token: allowed on standard POST & SSE, forbidden on ALL Admin routes (C1 regression)
        let op_post = Request::builder()
            .method(Method::POST)
            .uri("/v1/agents")
            .header(header::AUTHORIZATION, "Bearer operator-token-secret")
            .body(Body::empty())
            .unwrap();
        assert_eq!(app.clone().oneshot(op_post).await.unwrap().status(), StatusCode::OK);

        let op_sse = Request::builder()
            .method(Method::GET)
            .uri("/v1/mcp/sse")
            .header(header::AUTHORIZATION, "Bearer operator-token-secret")
            .body(Body::empty())
            .unwrap();
        assert_eq!(app.clone().oneshot(op_sse).await.unwrap().status(), StatusCode::OK);

        let admin_only_routes: &[(Method, &str)] = &[
            (Method::POST, "/v1/engine/deploy"),
            (Method::POST, "/v1/engine/templates/install"),
            (Method::DELETE, "/v1/engine/templates/demo"),
            (Method::PUT, "/v1/skills/hooks/test"),
            (Method::POST, "/v1/skills/promote"),
            (Method::POST, "/v1/skills/proposals/p1/resolve"),
            (Method::POST, "/v1/skills/mcp-tools/shell/execute"),
            (Method::PUT, "/v1/oversight/security/policies"),
            (Method::PUT, "/v1/oversight/security/missions/m1/quota"),
            (Method::PUT, "/v1/oversight/settings"),
            (Method::POST, "/v1/model-manager/providers/openai/test"),
            (Method::POST, "/v1/api/pull"),
            (Method::POST, "/v1/mcp/message"),
        ];
        for (method, uri) in admin_only_routes {
            let op_req = Request::builder()
                .method(method.clone())
                .uri(*uri)
                .header(header::AUTHORIZATION, "Bearer operator-token-secret")
                .body(Body::empty())
                .unwrap();
            assert_eq!(
                app.clone().oneshot(op_req).await.unwrap().status(),
                StatusCode::FORBIDDEN,
                "Operator token MUST be rejected on {} {}",
                method,
                uri
            );

            let adm_req = Request::builder()
                .method(method.clone())
                .uri(*uri)
                .header(header::AUTHORIZATION, "Bearer admin-token-secret")
                .body(Body::empty())
                .unwrap();
            assert_eq!(
                app.clone().oneshot(adm_req).await.unwrap().status(),
                StatusCode::OK,
                "Admin token MUST be permitted on {} {}",
                method,
                uri
            );
        }
    }
}





// Metadata: [auth]

//! @docs ARCHITECTURE:Security
//!
//! ### AI Assist Note
//! **Token Rotation Test Suite**: Verifies that requests authenticated with
//! NEURAL_TOKEN, NEURAL_TOKEN_NEW, or NEURAL_TOKEN_OLD are accepted during the
//! rotation grace period.
//!
//! ### 🔍 Debugging & Observability
//! - **Telemetry Link**: Search `[token_rotation_tests]` in tracing logs.
//! - **Trace Scope**: `server-rs::routes::token_rotation_tests`

#[cfg(test)]
mod tests {
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        routing::get,
        Router,
    };
    use std::sync::Arc;
    use tower::ServiceExt;

    use crate::{middleware::auth::validate_token, state::AppState};

    async fn test_app_with_tokens(
        current: &str,
        new: Option<&str>,
        old: Option<&str>,
    ) -> Router {
        let mut app_state = AppState::new_minimal_mock().await;
        
        // Inject token rotation configuration
        let new_security_hub = crate::state::hubs::sec::SecurityHub {
            audit_trail: app_state.security.audit_trail.clone(),
            budget_guard: app_state.security.budget_guard.clone(),
            shell_scanner: app_state.security.shell_scanner.clone(),
            secret_redactor: app_state.security.secret_redactor.clone(),
            system_monitor: app_state.security.system_monitor.clone(),
            permission_policy: app_state.security.permission_policy.clone(),
            deploy_token: current.to_string(),
            deploy_token_old: old.map(|s| s.to_string()),
            deploy_token_new: new.map(|s| s.to_string()),
        };
        app_state.security = Arc::new(new_security_hub);

        let shared_state = Arc::new(app_state);

        Router::new()
            .route("/protected", get(|| async { "success" }))
            .layer(axum::middleware::from_fn_with_state(
                shared_state,
                validate_token,
            ))
    }

    #[tokio::test]
    async fn test_auth_accepts_current_token() {
        let app = test_app_with_tokens("current-123", Some("new-456"), Some("old-789")).await;

        let request = Request::builder()
            .uri("/protected")
            .header("Authorization", "Bearer current-123")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_auth_accepts_new_token() {
        let app = test_app_with_tokens("current-123", Some("new-456"), Some("old-789")).await;

        let request = Request::builder()
            .uri("/protected")
            .header("Authorization", "Bearer new-456")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_auth_accepts_old_token() {
        let app = test_app_with_tokens("current-123", Some("new-456"), Some("old-789")).await;

        let request = Request::builder()
            .uri("/protected")
            .header("Authorization", "Bearer old-789")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_auth_rejects_invalid_token() {
        let app = test_app_with_tokens("current-123", Some("new-456"), Some("old-789")).await;

        let request = Request::builder()
            .uri("/protected")
            .header("Authorization", "Bearer invalid-999")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_auth_websocket_subprotocol_rotation() {
        let app = test_app_with_tokens("current-123", Some("new-456"), Some("old-789")).await;

        // Try upgrade with new token
        let request = Request::builder()
            .uri("/protected")
            .header("Upgrade", "websocket")
            .header("Sec-WebSocket-Protocol", "bearer.new-456")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
}





// Metadata: [token_rotation_tests]

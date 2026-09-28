//! @docs ARCHITECTURE:Networking
//!
//! ### AI Assist Note
//! **Intelligence Layer Route Tests**: Validates the Force Graph and symbol intelligence
//! API surface (`/v1/intelligence/graph`, `/v1/intelligence/graph/rebuild`, `/v1/intelligence/blast-radius`).
//! Verifies Bearer token auth enforcement, path traversal defense, and parameter boundaries.
//!
//! ### 🔍 Debugging & Observability
//! - **Telemetry Link**: Search `[intelligence_tests]` in tracing logs.
//! - **Trace Scope**: `server-rs::routes::intelligence_tests`

#[cfg(test)]
mod tests {
    use axum::{
        body::Body,
        http::{header, Request, StatusCode},
    };
    use tower::ServiceExt;
    use std::sync::Arc;

    use crate::state::AppState;
    use crate::router::create_router;

    const TEST_TOKEN: &str = "test-token-force-graph-secret-12345";

    async fn setup_test_app() -> (axum::Router, Arc<AppState>) {
        let mut app_state = AppState::new_minimal_mock().await;

        let new_security = crate::state::hubs::sec::SecurityHub {
            audit_trail: app_state.security.audit_trail.clone(),
            budget_guard: app_state.security.budget_guard.clone(),
            shell_scanner: app_state.security.shell_scanner.clone(),
            secret_redactor: app_state.security.secret_redactor.clone(),
            system_monitor: app_state.security.system_monitor.clone(),
            permission_policy: app_state.security.permission_policy.clone(),
            verification_gate: app_state.security.verification_gate.clone(),
            deploy_token: TEST_TOKEN.to_string(),
            deploy_token_old: None,
            deploy_token_new: None,
            token_rotated_at: None,
            token_grace_secs: None,
            conflict: app_state.security.conflict.clone(),
        };
        app_state.security = Arc::new(new_security);
        let state = Arc::new(app_state);
        state.notify_boot_complete();
        let app = create_router(state.clone());
        (app, state)
    }

    // 1. Happy Path: Authorized code graph retrieval
    #[tokio::test]
    async fn test_get_code_graph_authorized() {
        let (app, _state) = setup_test_app().await;

        let request = Request::builder()
            .uri("/v1/intelligence/graph")
            .header(header::AUTHORIZATION, format!("Bearer {}", TEST_TOKEN))
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = axum::body::to_bytes(response.into_body(), 100_000).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).expect("Valid JSON");
        assert!(json["nodes"].is_array());
        assert!(json["links"].is_array());
    }

    // 2. Happy Path: Authorized dry-run graph rebuild
    #[tokio::test]
    async fn test_rebuild_code_graph_dry_run_authorized() {
        let (app, _state) = setup_test_app().await;

        let request = Request::builder()
            .method("POST")
            .uri("/v1/intelligence/graph/rebuild?dry_run=true")
            .header(header::AUTHORIZATION, format!("Bearer {}", TEST_TOKEN))
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = axum::body::to_bytes(response.into_body(), 100_000).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).expect("Valid JSON");
        assert_eq!(json["status"], "success");
        assert_eq!(json["dry_run"], true);
        assert!(json["summary"].is_object());
    }

    // 3. Failure Path: Reject unauthenticated requests
    #[tokio::test]
    async fn test_intelligence_endpoints_reject_unauthorized() {
        let (app, _state) = setup_test_app().await;

        // Missing Authorization header
        let request = Request::builder()
            .uri("/v1/intelligence/graph")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    // 4. Failure Path: Reject missing required query parameters on blast radius
    #[tokio::test]
    async fn test_get_blast_radius_missing_params() {
        let (app, _state) = setup_test_app().await;

        let request = Request::builder()
            .uri("/v1/intelligence/blast-radius")
            .header(header::AUTHORIZATION, format!("Bearer {}", TEST_TOKEN))
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        // Axum Query rejection returns 400 Bad Request or 422 Unprocessable Entity
        assert!(response.status() == StatusCode::BAD_REQUEST || response.status() == StatusCode::UNPROCESSABLE_ENTITY);
    }

    // 5. Edge Case: max_nodes=0 clamps output
    #[tokio::test]
    async fn test_get_code_graph_max_nodes_boundary() {
        let (app, _state) = setup_test_app().await;

        let request = Request::builder()
            .uri("/v1/intelligence/graph?max_nodes=0")
            .header(header::AUTHORIZATION, format!("Bearer {}", TEST_TOKEN))
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = axum::body::to_bytes(response.into_body(), 100_000).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).expect("Valid JSON");
        let nodes = json["nodes"].as_array().expect("nodes array");
        assert_eq!(nodes.len(), 0);
    }

    // 6. Edge Case: Path traversal attempt is blocked
    #[tokio::test]
    async fn test_get_code_graph_path_traversal_blocked() {
        let (app, _state) = setup_test_app().await;

        let request = Request::builder()
            .uri("/v1/intelligence/graph?path_prefix=../../sensitive")
            .header(header::AUTHORIZATION, format!("Bearer {}", TEST_TOKEN))
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }
}

// Metadata: [intelligence_tests]

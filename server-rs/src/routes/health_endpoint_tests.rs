//! @docs ARCHITECTURE:Networking
//!
//! ### AI Assist Note
//! **Engine Health Endpoint Test Suite**: Verifies extended system observability metrics,
//! checking WAL size, sqlite connection pool status, and LLM budget usage structure.
//! Additionally verifies loopback vs remote peer redaction and cache disclosure immunity.
//!
//! ### 🔍 Debugging & Observability
//! - **Telemetry Link**: Search `[health_endpoint_tests]` in tracing logs.
//! - **Trace Scope**: `server-rs::routes::health_endpoint_tests`

#[cfg(test)]
mod tests {
    use axum::{
        body::Body,
        extract::ConnectInfo,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;
    use std::net::SocketAddr;
    use std::sync::Arc;

    use crate::state::AppState;
    use crate::router::create_router;

    async fn create_test_app() -> axum::Router {
        let mut app_state = AppState::new_minimal_mock().await;
        let new_security = crate::state::hubs::sec::SecurityHub {
            audit_trail: app_state.security.audit_trail.clone(),
            budget_guard: app_state.security.budget_guard.clone(),
            shell_scanner: app_state.security.shell_scanner.clone(),
            secret_redactor: app_state.security.secret_redactor.clone(),
            system_monitor: app_state.security.system_monitor.clone(),
            permission_policy: app_state.security.permission_policy.clone(),
            verification_gate: app_state.security.verification_gate.clone(),
            deploy_token: "test-token-123".to_string(),
            deploy_token_old: None,
            deploy_token_new: None,
            token_rotated_at: None,
            token_grace_secs: None,
            conflict: app_state.security.conflict.clone(),
        };
        app_state.security = Arc::new(new_security);
        let state = Arc::new(app_state);
        state.notify_boot_complete();
        create_router(state)
    }

    #[tokio::test]
    async fn test_health_endpoint_extended_metrics_ipv4_loopback() {
        let app = create_test_app().await;

        let request = Request::builder()
            .uri("/v1/engine/health")
            .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 8001))))
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = axum::body::to_bytes(response.into_body(), 100_000).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).expect("Failed to parse health JSON");

        assert_eq!(json["status"], "tadpole_online_rust");
        assert!(json["version"].is_string());
        assert!(json["heartbeat"].is_string());

        // Check full extended metrics are present for loopback
        let db = &json["database"];
        assert_eq!(db["status"], "healthy");
        assert!(db["pool_size"].as_u64().is_some());
        assert!(db["pool_idle"].as_u64().is_some());
        assert!(db["wal_size_mb"].as_f64().is_some());

        let budget = &json["budget"];
        assert!(budget["status"].is_string());
        assert!(budget["total_spent_usd"].as_f64().is_some());

        let swarm = &json["swarm"];
        assert!(swarm["status"].is_string());
        assert!(swarm["total_agents"].as_u64().is_some());

        assert!(json["uptime_seconds"].as_u64().is_some());
    }

    #[tokio::test]
    async fn test_health_endpoint_extended_metrics_ipv6_loopback() {
        let app = create_test_app().await;

        let request = Request::builder()
            .uri("/v1/engine/health")
            .extension(ConnectInfo(SocketAddr::from(([0, 0, 0, 0, 0, 0, 0, 1], 8001))))
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = axum::body::to_bytes(response.into_body(), 100_000).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).expect("Failed to parse health JSON");

        assert_eq!(json["status"], "tadpole_online_rust");
        assert!(json["database"].is_object());
        assert!(json["budget"].is_object());
    }

    #[tokio::test]
    async fn test_health_endpoint_remote_ipv4_returns_minimal_heartbeat() {
        let app = create_test_app().await;

        let request = Request::builder()
            .uri("/v1/engine/health")
            .extension(ConnectInfo(SocketAddr::from(([192, 168, 1, 50], 8001))))
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = axum::body::to_bytes(response.into_body(), 100_000).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).expect("Failed to parse health JSON");

        assert_eq!(json["status"], "ok");
        assert!(json["heartbeat"].is_string());

        // Assert strictly redacted fields
        assert!(json.get("database").is_none(), "database metrics must be redacted for remote callers");
        assert!(json.get("budget").is_none(), "budget metrics must be redacted for remote callers");
        assert!(json.get("swarm").is_none(), "swarm metrics must be redacted for remote callers");
        assert!(json.get("uptime_seconds").is_none(), "uptime must be redacted for remote callers");
        assert!(json.get("version").is_none(), "version must be redacted for remote callers");
    }

    #[tokio::test]
    async fn test_health_endpoint_remote_ipv6_returns_minimal_heartbeat() {
        let app = create_test_app().await;

        // Remote IPv6: 2001:db8::1
        let remote_ipv6: [u8; 16] = [0x20, 0x01, 0x0d, 0xb8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1];
        let request = Request::builder()
            .uri("/v1/engine/health")
            .extension(ConnectInfo(SocketAddr::from((remote_ipv6, 8001))))
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = axum::body::to_bytes(response.into_body(), 100_000).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).expect("Failed to parse health JSON");

        assert_eq!(json["status"], "ok");
        assert!(json["heartbeat"].is_string());
        assert!(json.get("database").is_none());
    }

    #[tokio::test]
    async fn test_health_cache_does_not_disclose_telemetry_to_remote_caller() {
        let app = create_test_app().await;

        // 1. Send loopback request to populate HEALTH_CACHE with detailed telemetry
        let local_req = Request::builder()
            .uri("/v1/engine/health")
            .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 8001))))
            .body(Body::empty())
            .unwrap();

        let local_resp = app.clone().oneshot(local_req).await.unwrap();
        assert_eq!(local_resp.status(), StatusCode::OK);
        let local_bytes = axum::body::to_bytes(local_resp.into_body(), 100_000).await.unwrap();
        let local_json: serde_json::Value = serde_json::from_slice(&local_bytes).unwrap();
        assert_eq!(local_json["status"], "tadpole_online_rust");
        assert!(local_json.get("database").is_some());

        // 2. Immediately send remote request to the same app (cache is populated and fresh)
        let remote_req = Request::builder()
            .uri("/v1/engine/health")
            .extension(ConnectInfo(SocketAddr::from(([10, 0, 0, 42], 8001))))
            .body(Body::empty())
            .unwrap();

        let remote_resp = app.oneshot(remote_req).await.unwrap();
        assert_eq!(remote_resp.status(), StatusCode::OK);
        let remote_bytes = axum::body::to_bytes(remote_resp.into_body(), 100_000).await.unwrap();
        let remote_json: serde_json::Value = serde_json::from_slice(&remote_bytes).unwrap();

        // 3. Confirm remote caller receives minimal response and NOT cached telemetry
        assert_eq!(remote_json["status"], "ok");
        assert!(remote_json.get("database").is_none(), "CACHE LEAK: database metrics disclosed to remote caller!");
        assert!(remote_json.get("budget").is_none(), "CACHE LEAK: budget metrics disclosed to remote caller!");
        assert!(remote_json.get("swarm").is_none(), "CACHE LEAK: swarm metrics disclosed to remote caller!");
    }
}

// Metadata: [health_endpoint_tests]

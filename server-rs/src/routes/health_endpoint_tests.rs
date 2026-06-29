//! @docs ARCHITECTURE:Networking
//!
//! ### AI Assist Note
//! **Engine Health Endpoint Test Suite**: Verifies extended system observability metrics,
//! checking WAL size, sqlite connection pool status, and LLM budget usage structure.
//!
//! ### 🔍 Debugging & Observability
//! - **Telemetry Link**: Search `[health_endpoint_tests]` in tracing logs.
//! - **Trace Scope**: `server-rs::routes::health_endpoint_tests`

#[cfg(test)]
mod tests {
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;
    use std::sync::Arc;

    use crate::state::AppState;
    use crate::router::create_router;

    #[tokio::test]
    async fn test_health_endpoint_extended_metrics() {
        // 1. Setup AppState with a mock database
        let mut app_state = AppState::new_minimal_mock().await;
        
        // Inject a known deployment token
        let new_security = crate::state::hubs::sec::SecurityHub {
            audit_trail: app_state.security.audit_trail.clone(),
            budget_guard: app_state.security.budget_guard.clone(),
            shell_scanner: app_state.security.shell_scanner.clone(),
            secret_redactor: app_state.security.secret_redactor.clone(),
            system_monitor: app_state.security.system_monitor.clone(),
            permission_policy: app_state.security.permission_policy.clone(),
            deploy_token: "test-token-123".to_string(),
            deploy_token_old: None,
            deploy_token_new: None,
        };
        app_state.security = Arc::new(new_security);
        
        let state = Arc::new(app_state);

        // Signal boot-complete so the boot middleware doesn't block the request.
        state.notify_boot_complete();

        // 2. Create the Axum router
        let app = create_router(state);

        // 3. Make GET request to /v1/engine/health (Public endpoint, bypasses auth)
        let request = Request::builder()
            .uri("/v1/engine/health")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // 4. Parse JSON body
        let body_bytes = axum::body::to_bytes(response.into_body(), 100_000).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).expect("Failed to parse health response JSON");

        // 5. Verify the required structure
        // The engine reports its own status tag rather than a generic "healthy" string.
        assert_eq!(json["status"], "tadpole_online_rust");
        assert!(json["version"].is_string());
        assert!(json["heartbeat"].is_string());
        
        // Check database metrics
        let db = &json["database"];
        assert_eq!(db["status"], "healthy");
        assert!(db["pool_size"].as_u64().is_some());
        assert!(db["pool_idle"].as_u64().is_some());
        assert!(db["wal_size_mb"].as_f64().is_some());

        // Check budget metrics
        let budget = &json["budget"];
        assert!(budget["status"].is_string());
        assert!(budget["total_spent_usd"].as_f64().is_some());

        // Check swarm metrics
        let swarm = &json["swarm"];
        assert!(swarm["status"].is_string());
        assert!(swarm["total_agents"].as_u64().is_some());

        // Check uptime
        assert!(json["uptime_seconds"].as_u64().is_some());
    }
}





// Metadata: [health_endpoint_tests]

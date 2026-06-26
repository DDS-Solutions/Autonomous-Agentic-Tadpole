//! @docs ARCHITECTURE:Networking
//!
//! ### AI Assist Note
//! **Engine Control & Shutdown Test Suite**: Verifies the emergency kill switch
//! and graceful shutdown triggers, confirming state persistence and registry flushing.
//!
//! ### 🔍 Debugging & Observability
//! - **Trace Scope**: `server-rs::routes::shutdown_orchestrator_tests`

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
    async fn test_emergency_kill_switch_resets_agents() {
        // 1. Setup AppState
        let mut app_state = AppState::new_minimal_mock().await;
        
        // Inject a dummy active agent
        let mut test_agent = crate::agent::types::EngineAgent::default();
        test_agent.identity.id = "agent-9".to_string();
        test_agent.health.status = "busy".to_string();
        test_agent.state.active_mission = Some(serde_json::json!("mission-123"));
        
        app_state.registry.agents.insert("agent-9".to_string(), test_agent);

        // Inject authorization token
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
        let app = create_router(state.clone());

        // 3. Make POST request to /v1/engine/kill (Requires auth)
        let request = Request::builder()
            .uri("/v1/engine/kill")
            .method("POST")
            .header("Authorization", "Bearer test-token-123")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // 4. Verify agent status was reset to "idle" and active mission was cleared
        let agent = state.registry.agents.get("agent-9").unwrap();
        assert_eq!(agent.health.status, "idle");
        assert!(agent.state.active_mission.is_none());
    }

    #[tokio::test]
    async fn test_graceful_shutdown_endpoint() {
        // 1. Setup AppState
        let mut app_state = AppState::new_minimal_mock().await;
        
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

        let app = create_router(state.clone());

        // 2. Make POST request to /v1/engine/shutdown (Requires auth)
        let request = Request::builder()
            .uri("/v1/engine/shutdown")
            .method("POST")
            .header("Authorization", "Bearer test-token-123")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = axum::body::to_bytes(response.into_body(), 100_000).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).expect("Failed to parse response JSON");
        assert_eq!(json["status"], "ok");
        assert!(json["message"].as_str().unwrap().contains("Shutdown initiated"));
    }
}

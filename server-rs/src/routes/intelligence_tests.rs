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

    #[tokio::test]
    async fn test_initial_graph_failure_stays_failed_and_rebuild_recovers() {
        let mut app_state = AppState::new_minimal_mock().await;
        let resources = Arc::get_mut(&mut app_state.resources).unwrap();
        resources.base_dir = resources.base_dir.join("missing-workspace");
        let state = Arc::new(app_state);
        for _ in 0..2 {
            assert!(state.resources.get_symbol_graph().await.is_err());
            assert!(matches!(state.resources.get_initialization_snapshot().get("SymbolGraph"), Some(crate::types::SubsystemStatus::Failed(_))));
        }

        let service = crate::intelligence::service::IntelligenceService::new(state.clone());
        assert!(service.list_graph(None, None).await.is_err());
        assert!(service.get_impacted_tests("*", "fixture.rs").await.is_err());
        std::fs::create_dir_all(&state.resources.base_dir).unwrap();
        let result = service.rebuild_graph(false).await.unwrap();
        assert_eq!(result.status, "success");
        assert!(matches!(state.resources.get_initialization_snapshot().get("SymbolGraph"), Some(crate::types::SubsystemStatus::Ready)));
        assert!(service.list_graph(None, None).await.is_ok());
    }

    #[tokio::test]
    async fn test_cancelled_rebuild_retains_gate_until_worker_finishes() {
        let (_, state) = setup_test_app().await;
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = tokio::sync::oneshot::channel();
        let init_state = state.clone();
        let initializer = tokio::spawn(async move {
            init_state.resources.symbol_graph.get_or_init(|| async {
                let _ = started_tx.send(());
                let _ = release_rx.await;
                Arc::new(arc_swap::ArcSwap::new(Arc::new(
                    crate::intelligence::graph::CodeSymbolGraph::new(init_state.resources.base_dir.clone()),
                )))
            }).await;
        });
        started_rx.await.unwrap();
        let request_state = state.clone();
        let request = tokio::spawn(async move {
            crate::intelligence::service::IntelligenceService::new(request_state).rebuild_graph(true).await
        });
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while !state.resources.is_rebuilding_symbol_graph.load(std::sync::atomic::Ordering::SeqCst) {
                tokio::task::yield_now().await;
            }
        }).await.unwrap();
        request.abort();
        let _ = request.await;
        assert!(state.resources.is_rebuilding_symbol_graph.load(std::sync::atomic::Ordering::SeqCst));
        let service = crate::intelligence::service::IntelligenceService::new(state.clone());
        assert!(matches!(service.rebuild_graph(true).await, Err(crate::error::AppError::Conflict(_))));
        release_tx.send(()).unwrap();
        initializer.await.unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while state.resources.is_rebuilding_symbol_graph.load(std::sync::atomic::Ordering::SeqCst) {
                tokio::task::yield_now().await;
            }
        }).await.unwrap();
    }

    #[tokio::test]
    async fn test_rebuild_counts_files_rather_than_symbols() {
        let (_, state) = setup_test_app().await;
        std::fs::write(state.resources.base_dir.join("fixture.rs"), "fn first() {}\nfn second() {}\n").unwrap();
        let service = crate::intelligence::service::IntelligenceService::new(state.clone());
        let result = service.rebuild_graph(true).await.unwrap();
        let summary = result.summary.unwrap();
        assert_eq!(summary.file_count, 1);
        assert_eq!(summary.node_count, 2);
        assert_eq!(result.status, "success");
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

    #[tokio::test]
    async fn partial_impacted_tests_return_explicit_error() {
        let (_, state) = setup_test_app().await;
        let mut graph = crate::intelligence::graph::CodeSymbolGraph::new(state.resources.base_dir.clone());
        let root = graph.graph.add_node(crate::intelligence::graph::SymbolNode {
            name: "root".into(), path: "chain.rs".into(), ..Default::default()
        });
        graph.index.insert(crate::intelligence::graph::key::index_key("chain.rs", "root"), root);
        let mut target = root;
        for index in 0..60 {
            let caller = graph.graph.add_node(crate::intelligence::graph::SymbolNode {
                name: format!("caller_{index}"), path: "chain.rs".into(), ..Default::default()
            });
            graph.graph.add_edge(caller, target, crate::intelligence::graph::SymbolEdge { kind: "call".into() });
            target = caller;
        }
        let swap = Arc::new(arc_swap::ArcSwap::new(Arc::new(graph)));
        assert!(state.resources.symbol_graph.set(swap).is_ok());
        let result = crate::intelligence::service::IntelligenceService::new(state).get_impacted_tests("root", "chain.rs").await;
        assert!(matches!(result, Err(crate::error::AppError::DomainError { code, .. }) if code == "BR_TRUNCATED"));
    }

    #[tokio::test]
    async fn live_http_rebuild_reports_sync_failure_and_releases_gate() {
        let (app, state) = setup_test_app().await;
        // A file where a database directory belongs forces a deterministic local sync failure.
        std::fs::write(state.resources.base_dir.join(".code-review-graph"), "blocked directory").unwrap();
        let request = Request::builder().method("POST").uri("/v1/intelligence/graph/rebuild")
            .header(header::AUTHORIZATION, format!("Bearer {}", TEST_TOKEN)).body(Body::empty()).unwrap();
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), 100_000).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["status"], "warning");
        assert!(!state.resources.is_rebuilding_symbol_graph.load(std::sync::atomic::Ordering::SeqCst));
        assert!(state.resources.get_symbol_graph().await.is_ok());
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

    // 7. Happy Path: Authorized symbol context resolution with token budget
    #[tokio::test]
    async fn test_resolve_code_context_authorized() {
        let (app, _state) = setup_test_app().await;

        let request = Request::builder()
            .uri("/v1/intelligence/resolve?name=test_sym&path=src/test.ts&budget=4000")
            .header(header::AUTHORIZATION, format!("Bearer {}", TEST_TOKEN))
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = axum::body::to_bytes(response.into_body(), 100_000).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).expect("Valid JSON");
        assert_eq!(json["budget"], 4000);
        assert!(json["symbols"].is_array());
    }

    // 8. Edge Case: Budget clamped to MAX_CONTEXT_BUDGET
    #[tokio::test]
    async fn test_resolve_code_context_budget_clamp() {
        let (app, _state) = setup_test_app().await;

        let request = Request::builder()
            .uri("/v1/intelligence/resolve?name=test_sym&path=src/test.ts&budget=99999")
            .header(header::AUTHORIZATION, format!("Bearer {}", TEST_TOKEN))
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = axum::body::to_bytes(response.into_body(), 100_000).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).expect("Valid JSON");
        assert_eq!(json["budget"], 32_768);
    }

    // 9. Happy Path: Authorized impacted test calculation
    #[tokio::test]
    async fn test_get_impacted_tests_authorized() {
        let (app, _state) = setup_test_app().await;

        let request = Request::builder()
            .uri("/v1/intelligence/impacted-tests?name=test_sym&path=src/test.ts")
            .header(header::AUTHORIZATION, format!("Bearer {}", TEST_TOKEN))
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = axum::body::to_bytes(response.into_body(), 100_000).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).expect("Valid JSON");
        assert!(json.is_array());
    }

    // 10. Failure Path: Resolve path traversal blocked
    #[tokio::test]
    async fn test_resolve_path_traversal_blocked() {
        let (app, _state) = setup_test_app().await;

        let request = Request::builder()
            .uri("/v1/intelligence/resolve?name=test_sym&path=../../etc/passwd")
            .header(header::AUTHORIZATION, format!("Bearer {}", TEST_TOKEN))
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    // 11. Edge Case Regression: max_nodes=0 with self-loop must return empty nodes AND empty links
    #[tokio::test]
    async fn test_get_code_graph_max_nodes_zero_excludes_self_loop_links() {
        let (app, state) = setup_test_app().await;
        let mut sym_graph = crate::intelligence::graph::CodeSymbolGraph::new(state.resources.base_dir.clone());
        let obf = crate::intelligence::graph::obfuscate_path("src/self_loop.rs", "salt").unwrap();
        sym_graph.obfuscated_to_real_path.insert(obf.clone(), "src/self_loop.rs".to_string());
        let idx = sym_graph.graph.add_node(crate::intelligence::graph::SymbolNode {
            name: "recursive_fn".to_string(),
            path: obf,
            kind: "function".to_string(),
            ..Default::default()
        });
        sym_graph.graph.add_edge(idx, idx, crate::intelligence::graph::SymbolEdge { kind: "call".to_string() });
        sym_graph.index.insert(crate::intelligence::graph::index_key("src/self_loop.rs", "recursive_fn"), idx);

        let swap = state.resources.get_symbol_graph().await.unwrap();
        swap.store(Arc::new(sym_graph));

        let request = Request::builder()
            .uri("/v1/intelligence/graph?max_nodes=0")
            .header(header::AUTHORIZATION, format!("Bearer {}", TEST_TOKEN))
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body_bytes = axum::body::to_bytes(response.into_body(), 100_000).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["nodes"].as_array().unwrap().len(), 0);
        assert_eq!(json["links"].as_array().unwrap().len(), 0);
    }

    // 12. Regression: path_prefix filters obfuscated anomalies by real workspace path
    #[tokio::test]
    async fn test_get_code_graph_path_prefix_filters_obfuscated_anomalies() {
        let (app, state) = setup_test_app().await;
        let mut sym_graph = crate::intelligence::graph::CodeSymbolGraph::new(state.resources.base_dir.clone());
        let svc_obf = crate::intelligence::graph::obfuscate_path("src/services/orphan_svc.rs", "salt").unwrap();
        let rte_obf = crate::intelligence::graph::obfuscate_path("src/routes/orphan_rte.rs", "salt").unwrap();
        sym_graph.obfuscated_to_real_path.insert(svc_obf.clone(), "src/services/orphan_svc.rs".to_string());
        sym_graph.obfuscated_to_real_path.insert(rte_obf.clone(), "src/routes/orphan_rte.rs".to_string());

        let svc_idx = sym_graph.graph.add_node(crate::intelligence::graph::SymbolNode {
            name: "orphan_svc_fn".to_string(),
            path: svc_obf.clone(),
            kind: "function".to_string(),
            ..Default::default()
        });
        let rte_idx = sym_graph.graph.add_node(crate::intelligence::graph::SymbolNode {
            name: "orphan_rte_fn".to_string(),
            path: rte_obf.clone(),
            kind: "function".to_string(),
            ..Default::default()
        });
        sym_graph.index.insert(crate::intelligence::graph::index_key("src/services/orphan_svc.rs", "orphan_svc_fn"), svc_idx);
        sym_graph.index.insert(crate::intelligence::graph::index_key("src/routes/orphan_rte.rs", "orphan_rte_fn"), rte_idx);

        let swap = state.resources.get_symbol_graph().await.unwrap();
        swap.store(Arc::new(sym_graph));

        let request = Request::builder()
            .uri("/v1/intelligence/graph?path_prefix=src/services")
            .header(header::AUTHORIZATION, format!("Bearer {}", TEST_TOKEN))
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body_bytes = axum::body::to_bytes(response.into_body(), 100_000).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        let anomalies = json["anomalies"].as_array().unwrap();
        assert!(anomalies.iter().any(|a| a.as_str().unwrap_or("").contains("orphan_svc_fn")));
        assert!(!anomalies.iter().any(|a| a.as_str().unwrap_or("").contains("orphan_rte_fn")));
    }

    // 13. Regression: impacted tests resolves obfuscated nested test directory paths
    #[tokio::test]
    async fn test_get_impacted_tests_resolves_obfuscated_nested_test_dir() {
        let (_app, state) = setup_test_app().await;
        let mut sym_graph = crate::intelligence::graph::CodeSymbolGraph::new(state.resources.base_dir.clone());
        let core_obf = crate::intelligence::graph::obfuscate_path("src/core.rs", "salt").unwrap();
        let test_obf = crate::intelligence::graph::obfuscate_path("tests/integration/core_suite.rs", "salt").unwrap();
        sym_graph.obfuscated_to_real_path.insert(core_obf.clone(), "src/core.rs".to_string());
        sym_graph.obfuscated_to_real_path.insert(test_obf.clone(), "tests/integration/core_suite.rs".to_string());

        let core_idx = sym_graph.graph.add_node(crate::intelligence::graph::SymbolNode {
            name: "core_fn".to_string(),
            path: core_obf,
            kind: "function".to_string(),
            ..Default::default()
        });
        let test_idx = sym_graph.graph.add_node(crate::intelligence::graph::SymbolNode {
            name: "verify_behavior".to_string(),
            path: test_obf,
            kind: "function".to_string(),
            ..Default::default()
        });
        // Caller (test_idx) depends on callee (core_idx)
        sym_graph.graph.add_edge(test_idx, core_idx, crate::intelligence::graph::SymbolEdge { kind: "call".to_string() });
        sym_graph.index.insert(crate::intelligence::graph::index_key("src/core.rs", "core_fn"), core_idx);
        sym_graph.index.insert(crate::intelligence::graph::index_key("tests/integration/core_suite.rs", "verify_behavior"), test_idx);

        let swap = state.resources.get_symbol_graph().await.unwrap();
        swap.store(Arc::new(sym_graph));

        let service = crate::intelligence::service::IntelligenceService::new(state.clone());
        let impacted = service.get_impacted_tests("core_fn", "src/core.rs").await.unwrap();
        assert_eq!(impacted, vec!["tests/integration/core_suite.rs".to_string()]);
    }
}

// Metadata: [intelligence_tests]

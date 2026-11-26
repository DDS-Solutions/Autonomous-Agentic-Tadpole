//! @docs ARCHITECTURE:Testing
//!
//! ### AI Assist Note
//! **Template Route Tests**: Validates input sanitization, SSRF protection boundaries,
//! MCP server configurations, connector lock digests, URL redaction, and serialization.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Regression in SSRF filter, command injection validation, or digest mismatches.
//! - **Telemetry Link**: Search `[templates_tests]` in tracing logs.
//! - **Trace Scope**: `server-rs::routes::templates::tests`

#[cfg(test)]
mod suite {
    use super::super::models::{InstallReceipt, InstallTemplateRequest};
    use super::super::security::{
        parse_expected_skill_hash, redact_url, validate_install_request,
        validate_mcp_server_config, verify_skill_sha256,
    };
    use sha2::{Digest, Sha256};
    use std::collections::HashMap;

    #[test]
    fn test_install_template_validation_and_ssrf_boundaries() {
        // Option injection check
        let req_hyphen = InstallTemplateRequest {
            repository_url: "--upload-pack=evil".to_string(),
            path: "agents".to_string(),
        };
        assert!(validate_install_request(&req_hyphen).is_err());

        // Protocol check
        let req_ftp = InstallTemplateRequest {
            repository_url: "ftp://github.com/user/repo".to_string(),
            path: "agents".to_string(),
        };
        assert!(validate_install_request(&req_ftp).is_err());

        // SSRF localhost
        let req_localhost = InstallTemplateRequest {
            repository_url: "http://localhost/repo.git".to_string(),
            path: "agents".to_string(),
        };
        assert!(validate_install_request(&req_localhost).is_err());

        // SSRF loopback IP
        let req_loopback = InstallTemplateRequest {
            repository_url: "http://127.0.0.1:8080/repo.git".to_string(),
            path: "agents".to_string(),
        };
        assert!(validate_install_request(&req_loopback).is_err());

        // SSRF cloud metadata IP
        let req_metadata = InstallTemplateRequest {
            repository_url: "http://169.254.169.254/latest/meta-data".to_string(),
            path: "agents".to_string(),
        };
        assert!(validate_install_request(&req_metadata).is_err());

        // Path traversal
        let req_traversal = InstallTemplateRequest {
            repository_url: "https://github.com/user/repo.git".to_string(),
            path: "../outside".to_string(),
        };
        assert!(validate_install_request(&req_traversal).is_err());

        // Valid path
        let req_valid = InstallTemplateRequest {
            repository_url: "https://github.com/user/repo.git".to_string(),
            path: "starters/finance".to_string(),
        };
        assert!(validate_install_request(&req_valid).is_ok());
    }

    #[test]
    fn test_mcp_server_config_validation_rules() {
        let valid_cfg = crate::agent::mcp::McpServerConfig {
            command: "npx".to_string(),
            args: vec!["-y".to_string(), "@modelcontextprotocol/server-filesystem".to_string()],
            env: None,
        };
        assert!(validate_mcp_server_config("filesystem", &valid_cfg).is_ok());

        // Rule: '__' rejected
        assert!(validate_mcp_server_config("custom__server", &valid_cfg).is_err());

        // Rule: Disallowed shell binaries rejected
        let shell_cfg = crate::agent::mcp::McpServerConfig {
            command: "/bin/sh".to_string(),
            args: vec!["-c".to_string(), "whoami".to_string()],
            env: None,
        };
        assert!(validate_mcp_server_config("shell-runner", &shell_cfg).is_err());

        // Rule: Prohibited shell metacharacters in args
        let metachar_cfg = crate::agent::mcp::McpServerConfig {
            command: "node".to_string(),
            args: vec!["script.js; rm -rf /".to_string()],
            env: None,
        };
        assert!(validate_mcp_server_config("bad-arg", &metachar_cfg).is_err());
    }

    #[test]
    fn test_connector_lock_sha256_verification_and_fail_closed() {
        let sample_bytes = b"print('hello world')";
        let mut hasher = Sha256::new();
        hasher.update(sample_bytes);
        let valid_hash = hex::encode(hasher.finalize());

        assert!(verify_skill_sha256(sample_bytes, &valid_hash));
        assert!(!verify_skill_sha256(
            sample_bytes,
            "0000000000000000000000000000000000000000000000000000000000000000"
        ));

        let lock_json = serde_json::json!({
            "skills": {
                "scrape.py": format!("sha256:{valid_hash}")
            },
            "direct.py": valid_hash.clone()
        });

        assert_eq!(
            parse_expected_skill_hash(&lock_json, "scrape.py"),
            Some(valid_hash.clone())
        );
        assert_eq!(
            parse_expected_skill_hash(&lock_json, "direct.py"),
            Some(valid_hash.clone())
        );
        assert_eq!(
            parse_expected_skill_hash(&lock_json, "non_existent.py"),
            None
        );
    }

    #[test]
    fn test_url_credential_redaction() {
        let url_with_token = "https://user:ghp_secretToken1234@github.com/org/repo.git";
        let redacted = redact_url(url_with_token);
        assert!(!redacted.contains("ghp_secretToken1234"));
        assert!(redacted.contains("REDACTED"));

        let normal_url = "https://github.com/org/repo.git";
        assert_eq!(redact_url(normal_url), normal_url);
    }

    #[test]
    fn test_receipt_serialization() {
        let receipt = InstallReceipt {
            template_id: "test-tmpl".to_string(),
            template_path: "finance/test".to_string(),
            installed_at: "2026-09-11T20:00:00Z".to_string(),
            agents: vec!["agent1.json".to_string()],
            agent_ids: vec!["agent_1".to_string()],
            workflows: vec!["wf.md".to_string()],
            skills: vec!["skill.py".to_string()],
            knowledge: vec!["sop.md".to_string()],
            mcp_servers: vec!["server1".to_string()],
            file_digests: HashMap::from([("skill.py".to_string(), "abc123hash".to_string())]),
        };

        let json_str = serde_json::to_string(&receipt).unwrap();
        let parsed: InstallReceipt = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed.template_id, "test-tmpl");
        assert_eq!(parsed.agent_ids.len(), 1);
        assert_eq!(parsed.file_digests.get("skill.py").unwrap(), "abc123hash");
    }

    #[tokio::test]
    async fn test_uninstall_retains_ledger_linked_agent_in_registry_and_db() {
        use super::super::models::UninstallTemplateResponse;
        use super::super::principal::TemplatePrincipal;
        use super::super::uninstall::uninstall_template;
        use crate::agent::persistence::create_agent_db;
        use crate::agent::types::EngineAgent;
        use crate::state::AppState;
        use axum::{
            extract::{Path, State},
            response::IntoResponse,
        };
        use std::sync::Arc;

        let tmp = tempfile::tempdir().unwrap();
        let mut state = AppState::new_minimal_mock().await;
        state.base_dir = tmp.path().to_path_buf();
        let state = Arc::new(state);

        // Create two agents: one unencumbered, one linked to an a2a_ledger entry
        let mut free_agent = EngineAgent::default();
        free_agent.identity.id = "tmpl-free-agent".to_string();
        free_agent.version = create_agent_db(&state.resources.pool, &free_agent)
            .await
            .unwrap();
        state
            .registry
            .agents
            .insert(free_agent.identity.id.clone(), free_agent.clone());

        let mut ledger_agent = EngineAgent::default();
        ledger_agent.identity.id = "tmpl-ledger-agent".to_string();
        ledger_agent.version = create_agent_db(&state.resources.pool, &ledger_agent)
            .await
            .unwrap();
        state
            .registry
            .agents
            .insert(ledger_agent.identity.id.clone(), ledger_agent.clone());

        sqlx::query(
            "INSERT INTO a2a_ledger (tx_id, debit_agent_id, credit_agent_id, amount_micros, status, lock_id) \
             VALUES ('tx-tmpl-1', 'tmpl-ledger-agent', 'tmpl-ledger-agent', 500, 'COMMITTED', 'lock-tmpl-1')",
        )
        .execute(&state.resources.pool)
        .await
        .unwrap();

        // Write an installed template directory + receipt referencing both agents
        let tmpl_dir = tmp
            .path()
            .join("data/swarm_config/installed/demo-template");
        tokio::fs::create_dir_all(&tmpl_dir).await.unwrap();
        let receipt = InstallReceipt {
            template_id: "demo-template".to_string(),
            template_path: "demo".to_string(),
            installed_at: "2026-11-26T00:00:00Z".to_string(),
            agents: vec![],
            agent_ids: vec![
                "tmpl-free-agent".to_string(),
                "tmpl-ledger-agent".to_string(),
            ],
            workflows: vec![],
            skills: vec![],
            knowledge: vec![],
            mcp_servers: vec![],
            file_digests: HashMap::new(),
        };
        tokio::fs::write(
            tmpl_dir.join("install_receipt.json"),
            serde_json::to_string_pretty(&receipt).unwrap(),
        )
        .await
        .unwrap();

        let resp = uninstall_template(
            TemplatePrincipal::new("admin-test"),
            State(state.clone()),
            Path("demo-template".to_string()),
        )
        .await
        .unwrap()
        .into_response();

        assert_eq!(resp.status(), axum::http::StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let parsed: UninstallTemplateResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(parsed.retained_agents, vec!["tmpl-ledger-agent".to_string()]);

        // Unencumbered agent deleted from BOTH SQLite and RAM
        assert!(!state.registry.agents.contains_key("tmpl-free-agent"));
        let free_db_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM agents WHERE id = 'tmpl-free-agent'")
                .fetch_one(&state.resources.pool)
                .await
                .unwrap();
        assert_eq!(free_db_count, 0);

        // Ledger-linked agent retained in BOTH SQLite and RAM (no split-brain)
        assert!(state.registry.agents.contains_key("tmpl-ledger-agent"));
        let ledger_db_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM agents WHERE id = 'tmpl-ledger-agent'")
                .fetch_one(&state.resources.pool)
                .await
                .unwrap();
        assert_eq!(ledger_db_count, 1);
    }
}

// Metadata: [templates_tests]


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
mod tests {
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
}

// Metadata: [templates_tests]


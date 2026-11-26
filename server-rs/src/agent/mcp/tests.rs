//! @docs ARCHITECTURE:TestSuites
//!
//! ### AI Assist Note
//! **MCP Host Unit Tests**: Validates environment placeholder interpolation, UTF-8 reply
//! truncation, prefixed external server resolution, structured child-process spawning,
//! JSON-RPC result formatting, and recursive secret scrubbing.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Missing env error regression, UTF-8 panic, or leaked secret in JSON payload.
//! - **Telemetry Link**: Search `[mcp_tests]` in cargo test logs.

use super::*;
use std::collections::HashMap;

#[test]
fn test_resolve_mcp_env_success_and_fallback() {
    let mut env = HashMap::new();
    env.insert("AUTH_HEADER".to_string(), "Bearer ${TADPOLE_TEST_MCP_KEY}".to_string());
    env.insert("FALLBACK_VAR".to_string(), "${UNSET_VAR_12345:-http://localhost:8000}".to_string());
    env.insert("STATIC_VAR".to_string(), "plain_value".to_string());

    let resolved = resolve_mcp_env_with(&env, |k| {
        if k == "TADPOLE_TEST_MCP_KEY" {
            Some("secret-token-value".to_string())
        } else {
            None
        }
    })
    .expect("env resolution should succeed");
    assert_eq!(resolved.get("AUTH_HEADER").unwrap(), "Bearer secret-token-value");
    assert_eq!(resolved.get("FALLBACK_VAR").unwrap(), "http://localhost:8000");
    assert_eq!(resolved.get("STATIC_VAR").unwrap(), "plain_value");
}

#[test]
fn test_resolve_mcp_env_missing_required_variable_errors() {
    let mut env = HashMap::new();
    env.insert("API_TOKEN".to_string(), "${COMPLETELY_UNSET_REQUIRED_VAR}".to_string());

    let result = resolve_mcp_env_with(&env, |_| None);
    assert!(result.is_err(), "Expected missing variable without fallback to return an error");
    if let Err(AppError::BadRequest(msg)) = result {
        assert!(msg.contains("COMPLETELY_UNSET_REQUIRED_VAR"));
    } else {
        panic!("Expected BadRequest error for missing required credential");
    }
}

#[test]
fn test_truncate_mcp_reply_and_external_matching_happy_failure_and_edge() {
    // (a) Happy Path: Payload under limit is returned unchanged; prefixed external tools resolve cleanly
    let short = "hello sovereign world";
    assert_eq!(truncate_mcp_reply(short, 100, "Tool response"), short);

    let mut servers = HashMap::new();
    servers.insert(
        "brave-search".to_string(),
        McpServerConfig {
            command: "npx".to_string(),
            args: vec![],
            env: None,
        },
    );
    let cfg = McpConfig { mcp_servers: servers };
    assert_eq!(
        McpHost::match_prefixed_external_tool("brave-search:web_search", &cfg),
        Some(("brave-search".to_string(), "web_search".to_string()))
    );
    assert_eq!(
        McpHost::match_prefixed_external_tool("brave-search::local_search", &cfg),
        Some(("brave-search".to_string(), "local_search".to_string()))
    );
    assert_eq!(
        McpHost::match_prefixed_external_tool("mcp_brave-search_web_search", &cfg),
        Some(("brave-search".to_string(), "web_search".to_string()))
    );

    // (b) Failure Path: Unknown server prefixes or empty tool suffixes return None
    assert_eq!(
        McpHost::match_prefixed_external_tool("unknown-server:web_search", &cfg),
        None
    );
    assert_eq!(
        McpHost::match_prefixed_external_tool("brave-search:", &cfg),
        None
    );
    assert_eq!(
        McpHost::match_prefixed_external_tool("mcp_brave-search_", &cfg),
        None
    );

    // (c) Edge Case Path: Multi-byte UTF-8 boundary (4-byte emoji '🦀' at bytes 3..7) never panics
    let multibyte = "abc🦀def";
    assert_eq!(floor_utf8_char_boundary(multibyte, 4), 3);
    assert_eq!(floor_utf8_char_boundary(multibyte, 5), 3);
    assert_eq!(floor_utf8_char_boundary(multibyte, 6), 3);
    assert_eq!(floor_utf8_char_boundary(multibyte, 7), 7);
    let truncated = truncate_mcp_reply(multibyte, 5, "Tool response");
    assert!(truncated.starts_with("abc\n\n[TRUNCATED: Tool response exceeded 2MB limit]"));
}

#[test]
fn test_format_external_mcp_result_extracts_content_and_falls_back_to_json() {
    let content_payload = serde_json::json!({
        "content": [
            { "type": "text", "text": "part-1 " },
            { "type": "text", "text": "part-2" }
        ]
    });
    match McpHost::format_external_mcp_result(&content_payload).unwrap() {
        McpResult::Raw(text) => assert_eq!(text, "part-1 part-2"),
        _ => panic!("Expected McpResult::Raw"),
    }

    let raw_obj = serde_json::json!({ "status": "ok", "count": 42 });
    match McpHost::format_external_mcp_result(&raw_obj).unwrap() {
        McpResult::Raw(text) => {
            assert!(text.contains("\"status\": \"ok\""));
            assert!(text.contains("42"));
        }
        _ => panic!("Expected McpResult::Raw"),
    }
}

#[tokio::test]
async fn test_scrub_json_value_redacts_nested_secrets() {
    let state = crate::state::AppState::new_minimal_mock().await;
    let (tx, _rx) = tokio::sync::broadcast::channel(8);
    let host = McpHost::new(
        tx,
        None,
        state.security.permission_policy.clone(),
        Arc::new(SecretRedactor::empty()),
    );

    let gh_sample = format!("{}_{}", "ghp", "1234567890abcdefghijklmnopqrstuvwxyz");
    let openai_sample = format!("{}-{}", "sk", "abcdefghijklmnopqrstuvwxyz1234567890");
    let mut payload = serde_json::json!({
        "safe": "hello",
        "nested": {
            "token": gh_sample
        },
        "items": [openai_sample, 7]
    });

    host.scrub_json_value(&mut payload);
    assert_eq!(payload["safe"], "hello");
    assert!(!payload["nested"]["token"].as_str().unwrap().contains("1234567890"));
    assert!(!payload["items"][0].as_str().unwrap().contains("abcdefghijklmnopqrstuvwxyz"));
    assert_eq!(payload["items"][1], 7);
}

#[tokio::test]
async fn test_structured_mcp_spawning_preserves_arguments_and_env() {
    let args = vec![
        "-c".to_string(),
        r#"import sys, os, json; print(json.dumps({"args": sys.argv[1:], "env_val": os.environ.get("MCP_TEST_KEY")}))"#.to_string(),
        "first arg with spaces".to_string(),
        "".to_string(),
        r#"literal"quote"arg"#.to_string(),
    ];

    let mut env = HashMap::new();
    env.insert("MCP_TEST_KEY".to_string(), "configured_secret_value".to_string());

    let mut client = client::McpClient::spawn("python", &args, Some(&env))
        .await
        .expect("Failed to spawn python child process");

    use tokio::io::AsyncBufReadExt;
    let mut line = String::new();
    client.stdout.read_line(&mut line).await.expect("Failed to read child stdout");

    let output: serde_json::Value = serde_json::from_str(&line).expect("Child process did not emit valid JSON");
    let parsed_args = output["args"].as_array().expect("args should be an array");
    assert_eq!(parsed_args.len(), 3);
    assert_eq!(parsed_args[0], "first arg with spaces");
    assert_eq!(parsed_args[1], "");
    assert_eq!(parsed_args[2], r#"literal"quote"arg"#);
    assert_eq!(output["env_val"], "configured_secret_value");
}

// Metadata: [tests]

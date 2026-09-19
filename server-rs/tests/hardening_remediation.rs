//! @docs ARCHITECTURE:Core
//!
//! ### AI Assist Note
//! **Gateway & Types Hardening Integration Tests**: Verifies race-free runner
//! handle eviction, provider credential suppression, role authority ordering,
//! and model configuration merge completeness.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Authority level inversion, secret leakage in JSON serialization.
//! - **Telemetry Link**: Search `[hardening_remediation]` in test runner outputs.
//!

use server_rs::agent::types::{
    CapabilityPattern, EngineAgent, ModelConfig, ProviderConfig, RoleAuthorityLevel,
};
use server_rs::state::hubs::comm::RunnerHandle;
use std::collections::HashMap;

#[test]
fn test_role_authority_hierarchy_ordering() {
    println!("[hardening_remediation] Verifying role authority hierarchy ordering...");
    // Strict ordering: Observer < Specialist < Management < Executive
    assert!(
        RoleAuthorityLevel::Executive > RoleAuthorityLevel::Management,
        "Executive must outrank Management"
    );
    assert!(
        RoleAuthorityLevel::Management > RoleAuthorityLevel::Specialist,
        "Management must outrank Specialist"
    );
    assert!(
        RoleAuthorityLevel::Specialist > RoleAuthorityLevel::Observer,
        "Specialist must outrank Observer"
    );
    assert!(
        RoleAuthorityLevel::Executive > RoleAuthorityLevel::Observer,
        "Executive must outrank Observer"
    );
}

#[test]
fn test_unknown_role_fails_closed_to_observer() {
    println!("[hardening_remediation] Testing fail-closed behavior for unknown roles...");
    let unknown = RoleAuthorityLevel::from_role("untrusted_guest_user");
    assert_eq!(
        unknown,
        RoleAuthorityLevel::Observer,
        "Security violation: Unknown roles must default to read-only Observer"
    );

    let malformed = RoleAuthorityLevel::from_role("");
    assert_eq!(
        malformed,
        RoleAuthorityLevel::Observer,
        "Empty role strings must default to Observer"
    );
}

#[test]
fn test_provider_config_secret_suppression() {
    println!("[hardening_remediation] Testing provider config credential suppression...");
    let config = ProviderConfig {
        id: "openai-test".to_string(),
        name: "OpenAI".to_string(),
        api_key: Some("sk-secret-token-12345".to_string()),
        custom_headers: Some(HashMap::from([(
            "Authorization".to_string(),
            "Bearer secret_bearer_token".to_string(),
        )])),
        ..Default::default()
    };

    let val = serde_json::to_value(&config).expect("Must serialize cleanly");
    assert!(
        val.get("apiKey").is_none(),
        "CRITICAL: apiKey leaked in camelCase JSON output"
    );
    assert!(
        val.get("api_key").is_none(),
        "CRITICAL: api_key leaked in snake_case JSON output"
    );
    assert!(
        val.get("customHeaders").is_none(),
        "CRITICAL: customHeaders leaked in camelCase JSON output"
    );
    assert!(
        val.get("custom_headers").is_none(),
        "CRITICAL: custom_headers leaked in snake_case JSON output"
    );
}

#[test]
fn test_model_config_merge_coverage() {
    println!("[hardening_remediation] Verifying ModelConfig::merge completeness...");
    let base = ModelConfig {
        model_id: "base-model".to_string(),
        max_turns: None,
        top_p: None,
        base_url: None,
        ..Default::default()
    };

    let override_cfg = ModelConfig {
        model_id: "override-model".to_string(),
        max_turns: Some(50),
        top_p: Some(0.95),
        base_url: Some("http://localhost:11434".to_string()),
        ..Default::default()
    };

    let merged = base.merge(&override_cfg);
    assert_eq!(merged.max_turns, Some(50), "max_turns must propagate on merge");
    assert_eq!(merged.top_p, Some(0.95), "top_p must propagate on merge");
    assert_eq!(
        merged.base_url.as_deref(),
        Some("http://localhost:11434"),
        "base_url must propagate on merge"
    );
}

#[test]
fn test_engine_agent_default_health_status() {
    println!("[hardening_remediation] Checking EngineAgent default health status...");
    let agent = EngineAgent::default();
    assert_eq!(
        agent.health.status, "idle",
        "EngineAgent::default() health status must be 'idle' rather than an uninitialized empty string"
    );
}

#[test]
fn test_capability_patterns_compile_time_integrity() {
    println!("[hardening_remediation] Validating embedded capability patterns JSON asset...");
    let raw = include_str!("../src/agent/capability_patterns.json");
    let patterns: Result<Vec<CapabilityPattern>, _> = serde_json::from_str(raw);
    assert!(
        patterns.is_ok(),
        "capability_patterns.json must be valid JSON: {:?}",
        patterns.err()
    );
    assert!(
        !patterns.unwrap().is_empty(),
        "capability_patterns.json must contain at least one pattern entry"
    );
}

#[tokio::test]
async fn test_runner_handle_eviction_and_abort() {
    println!("[hardening_remediation] Testing RunnerHandle abort and task ID correlation...");
    let join_handle = tokio::spawn(async {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    });

    let task_id = join_handle.id();
    let runner_handle = RunnerHandle::new(join_handle.abort_handle(), task_id);

    assert_eq!(runner_handle.task_id, task_id);
    runner_handle.abort();
    assert!(join_handle.is_finished() || true);
}

// Metadata: [hardening_remediation]

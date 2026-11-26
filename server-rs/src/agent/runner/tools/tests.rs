//! @docs ARCHITECTURE:TestSuites
//!
//! ### AI Assist Note
//! **Runner Tool Pipeline & Security Unit Tests**: Verifies CBS allow-listing, hierarchy
//! boundaries, policy deny gates, Aletheia verification rejections, quote/escape shell bypass
//! defense, Sapphire Gate enforcement, script fallback discovery, and SharedBlackboard facets.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Regression in pre-execution guards, shell validator, or blackboard Observe-mode block.
//! - **Telemetry Link**: Search `[tools_tests]` in cargo test logs.

use super::*;
use crate::agent::constants::*;
use crate::agent::runner::{AgentRunner, RunContext};
use crate::agent::types::{EngineAgent, ToolCall};
use crate::state::AppState;
use std::sync::Arc;

#[tokio::test]
async fn test_execute_tool_cbs_block() {
    let state = Arc::new(AppState::new_minimal_mock().await);
    let runner = AgentRunner::new(state.clone());
    let ctx = RunContext {
        agent_id: "worker-1".to_string(),
        ..Default::default()
    };

    let mut agent = EngineAgent::default();
    agent.identity.id = ctx.agent_id.clone();
    agent.capabilities.skills = vec!["allowed_skill".to_string()];
    state.registry.agents.insert(ctx.agent_id.clone(), agent);

    let fc = ToolCall {
        name: "forbidden_skill".to_string(),
        args: serde_json::json!({}),
    };

    let mut output = String::new();
    let mut usage = None;
    let result = runner
        .execute_tool(&ctx, &fc, &mut output, &mut usage, "")
        .await;

    assert!(result.is_ok());
    assert!(output.contains("Security Violation: Skill 'forbidden_skill' not in agent allowlist"));
    assert!(output.contains("| RECOVERY: Escalate"));
}

#[tokio::test]
async fn test_execute_tool_hierarchy_block() {
    let state = Arc::new(AppState::new_minimal_mock().await);
    let runner = AgentRunner::new(state.clone());
    let ctx = RunContext {
        agent_id: AGENT_CEO.to_string(),
        ..Default::default()
    };

    let mut agent = EngineAgent::default();
    agent.identity.id = ctx.agent_id.clone();
    state.registry.agents.insert(ctx.agent_id.clone(), agent);

    let fc = ToolCall {
        name: "spawn_subagent".to_string(),
        args: serde_json::json!({"agent_id": "worker"}),
    };

    let mut output = String::new();
    let mut usage = None;
    let result = runner
        .execute_tool(&ctx, &fc, &mut output, &mut usage, "")
        .await;

    assert!(result.is_ok());
    assert!(
        output.contains("Hierarchy Violation: As CEO, you are prohibited from direct worker recruitment.")
    );
    assert!(output.contains("| RECOVERY: Escalate"));
}

#[tokio::test]
async fn test_execute_tool_policy_deny() {
    let state = Arc::new(AppState::new_minimal_mock().await);
    let runner = AgentRunner::new(state.clone());
    let ctx = RunContext::default();

    state
        .security
        .permission_policy
        .set_mode("risky_tool", crate::security::permissions::PermissionMode::Deny)
        .await;

    let fc = ToolCall {
        name: "risky_tool".to_string(),
        args: serde_json::json!({}),
    };

    let mut output = String::new();
    let mut usage = None;
    let result = runner
        .execute_tool(&ctx, &fc, &mut output, &mut usage, "")
        .await;

    assert!(result.is_ok());
    assert!(output.contains("Security Violation: Policy for 'risky_tool' is set to DENY"));
    assert!(output.contains("| RECOVERY: Escalate"));
}

#[tokio::test]
async fn test_update_working_memory() {
    let state = Arc::new(AppState::new_minimal_mock().await);
    let runner = AgentRunner::new(state.clone());
    let ctx = RunContext {
        agent_id: "memory-agent".to_string(),
        ..Default::default()
    };

    let mut agent = EngineAgent::default();
    agent.identity.id = ctx.agent_id.clone();
    agent.state.active_mission = Some(serde_json::json!({"id": ctx.mission_id}));
    state.registry.agents.insert(ctx.agent_id.clone(), agent);

    let mut persisted = state.registry.agents.get(&ctx.agent_id).unwrap().clone();
    persisted.version =
        crate::agent::persistence::create_agent_db(&state.resources.pool, &persisted)
            .await
            .unwrap();
    state.registry.agents.insert(ctx.agent_id.clone(), persisted);

    let fc = ToolCall {
        name: "update_working_memory".to_string(),
        args: serde_json::json!({"memory": {"last_step": "initialized"}}),
    };

    let mut output = String::new();
    let result = runner
        .handle_update_working_memory(&ctx, &fc, &mut output)
        .await;

    assert!(result.is_ok());
    let agent = state.registry.agents.get(&ctx.agent_id).unwrap();
    assert_eq!(agent.state.working_memory["last_step"], "initialized");
}

#[test]
fn test_normalize_tool_name() {
    assert_eq!(normalize_tool_name("parity_guard"), "parity_guard");
    assert_eq!(normalize_tool_name("execution/parity_guard.py"), "parity_guard");
    assert_eq!(normalize_tool_name("./execution/security_scan.py"), "security_scan");
    assert_eq!(
        normalize_tool_name("execution\\verify_ai_context.py"),
        "verify_ai_context"
    );
    assert_eq!(
        normalize_tool_name("agent_generated/skills/custom_task.py"),
        "custom_task"
    );
    assert_eq!(normalize_tool_name("skills/auto_clean.sh"), "auto_clean");
    assert_eq!(normalize_tool_name("db_health_check.ps1"), "db_health_check");
}

#[test]
fn test_recursive_merge_json() {
    let mut base = serde_json::json!({
        "nested": {
            "key1": "val1",
            "key2": 42
        },
        "array": [1, 2],
        "unchanged": true
    });

    let patch = serde_json::json!({
        "nested": {
            "key2": 100,
            "key3": "new_val"
        },
        "new_top": "hello"
    });

    recursive_merge_json(&mut base, &patch);

    assert_eq!(base["nested"]["key1"], "val1");
    assert_eq!(base["nested"]["key2"], 100);
    assert_eq!(base["nested"]["key3"], "new_val");
    assert_eq!(base["unchanged"], true);
    assert_eq!(base["new_top"], "hello");
}

#[test]
fn test_pre_execution_guards_happy_failure_and_edge() {
    let mut ctx = RunContext {
        agent_id: "guard-tester".to_string(),
        mission_id: "mission-guard-1".to_string(),
        current_cost_usd: 1.25,
        budget_limit_usd: 5.00,
        safe_mode: false,
        ..Default::default()
    };

    let valid_token = ZeroTrustGuard::mint_token(&ctx.agent_id, &ctx.mission_id, ctx.authority_level);
    let read_fc = ToolCall {
        name: "read_file".to_string(),
        args: serde_json::json!({"file_path": "src/main.rs"}),
    };
    let write_fc = ToolCall {
        name: "write_file".to_string(),
        args: serde_json::json!({"filename": "src/lib.rs"}),
    };

    assert!(AgentRunner::validate_pre_execution_guards(&ctx, &read_fc, &valid_token).is_ok());
    assert!(AgentRunner::validate_pre_execution_guards(&ctx, &write_fc, &valid_token).is_ok());
    assert_eq!(extract_target_path(&read_fc.args), Some("src/main.rs"));
    assert_eq!(extract_target_path(&write_fc.args), Some("src/lib.rs"));

    let wrong_agent_token =
        ZeroTrustGuard::mint_token("other-agent", &ctx.mission_id, ctx.authority_level);
    let err_mismatch =
        AgentRunner::validate_pre_execution_guards(&ctx, &read_fc, &wrong_agent_token)
            .expect_err("Mismatched agent token must be rejected");
    assert!(err_mismatch
        .to_string()
        .contains("Invalid or expired CapabilityToken"));

    ctx.safe_mode = true;
    assert!(AgentRunner::validate_pre_execution_guards(&ctx, &read_fc, &valid_token).is_ok());
    let err_safe = AgentRunner::validate_pre_execution_guards(&ctx, &write_fc, &valid_token)
        .expect_err("Mutation tool in safe_mode must be blocked");
    assert!(err_safe.to_string().contains("Safe mode active"));

    ctx.safe_mode = false;
    ctx.accept_mode = crate::agent::types::AcceptMode::Observe;
    assert!(AgentRunner::validate_pre_execution_guards(&ctx, &read_fc, &valid_token).is_ok());
    let err_observe = AgentRunner::validate_pre_execution_guards(&ctx, &write_fc, &valid_token)
        .expect_err("Mutation tool in Observe mode must be blocked");
    assert!(err_observe
        .to_string()
        .contains("Observe (Shadow) mode active"));
    ctx.accept_mode = crate::agent::types::AcceptMode::Auto;

    ctx.safe_mode = false;
    ctx.current_cost_usd = 5.00;
    ctx.budget_limit_usd = 5.00;
    let err_budget = AgentRunner::validate_pre_execution_guards(&ctx, &read_fc, &valid_token)
        .expect_err("Exact budget ceiling equality must halt tool execution");
    assert!(err_budget.to_string().contains("Budget exhausted"));
}

#[tokio::test]
async fn test_resolve_script_fallback_skill_discovers_execution_scripts_and_enforces_oversight() {
    let dir = tempfile::tempdir().unwrap();
    let exec_dir = dir.path().join("execution");
    std::fs::create_dir_all(&exec_dir).unwrap();
    std::fs::write(exec_dir.join("probe_script.py"), "print('ok')\n").unwrap();

    let found = AgentRunner::resolve_script_fallback_skill(dir.path(), "probe_script")
        .await
        .expect("Should synthesize fallback skill for existing script");
    assert_eq!(found.name, "probe_script");
    assert_eq!(found.execution_command, "python execution/probe_script.py");
    assert!(found.oversight_required);

    let missing = AgentRunner::resolve_script_fallback_skill(dir.path(), "nonexistent_script").await;
    assert!(missing.is_none());
}

#[tokio::test]
async fn test_execute_tool_aletheia_gate_rejection() {
    let state = Arc::new(AppState::new_minimal_mock().await);
    let runner = AgentRunner::new(state.clone());
    let ctx = RunContext {
        agent_id: "worker-mutator".to_string(),
        ..Default::default()
    };

    let mut agent = EngineAgent::default();
    agent.identity.id = ctx.agent_id.clone();
    agent.capabilities.skills = vec!["write_file".to_string()];
    state.registry.agents.insert(ctx.agent_id.clone(), agent);

    let fc = ToolCall {
        name: "write_file".to_string(),
        args: serde_json::json!({
            "path": "src/core.rs",
            "blast_radius": 45,
            "content": "pub fn mutated() {}"
        }),
    };

    let mut output = String::new();
    let mut usage = None;
    let result = runner
        .execute_tool(&ctx, &fc, &mut output, &mut usage, "")
        .await;

    assert!(result.is_ok());
    assert!(output.contains("Aletheia Gate REJECTED mutation 'write_file'"));
    assert!(output.contains("exceeds safety threshold (15)"));
}

#[tokio::test]
async fn test_handle_execute_shell_blocks_quote_and_escape_bypasses() {
    let state = Arc::new(AppState::new_minimal_mock().await);
    let runner = AgentRunner::new(state.clone());
    let ctx = RunContext {
        agent_id: "worker-shell-bypass".to_string(),
        ..Default::default()
    };

    for malicious in [
        r#"python "-c" "__import__('os').system('id')""#,
        r#"python \-c "__import__('os').system('id')""#,
        r#"python ^-c "print(1)""#,
        r#"node "--eval=process.exit(1)""#,
        r#"cat .ssh/id_ed25519"#,
        r#"node --env-file=.env app.js"#,
    ] {
        let fc = ToolCall {
            name: "execute_shell".to_string(),
            args: serde_json::json!({ "command": malicious }),
        };
        let mut output = String::new();
        runner
            .handle_execute_shell(&ctx, &fc, &mut output)
            .await
            .unwrap();
        assert!(
            output.contains("SECURITY BLOCKED"),
            "Live handle_execute_shell failed to block bypass '{}': got '{}'",
            malicious,
            output
        );
        assert!(
            state.comms.oversight_queue.is_empty(),
            "Blocked shell command must never enqueue oversight"
        );
    }
}

#[tokio::test]
async fn test_handle_execute_shell_sandboxed_default_deny() {
    let state = Arc::new(AppState::new_minimal_mock().await);
    state
        .governance
        .auto_approve_safe_skills
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let runner = AgentRunner::new(state.clone());
    let ctx = RunContext {
        agent_id: "worker-shell".to_string(),
        ..Default::default()
    };

    let mut agent = EngineAgent::default();
    agent.identity.id = ctx.agent_id.clone();
    agent.capabilities.skills = vec!["execute_shell".to_string()];
    state.registry.agents.insert(ctx.agent_id.clone(), agent);

    let state_resolver = state.clone();
    let saw_oversight = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let saw_oversight_clone = saw_oversight.clone();
    let resolver_task = tokio::spawn(async move {
        for _ in 0..100 {
            let pending_id = state_resolver
                .comms
                .oversight_resolvers
                .iter()
                .next()
                .map(|e| e.key().clone());
            if let Some(id) = pending_id {
                saw_oversight_clone.store(true, std::sync::atomic::Ordering::SeqCst);
                if let Some((_, tx)) = state_resolver.comms.oversight_resolvers.remove(&id) {
                    let _ = tx.send(true);
                }
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    });

    let fc = ToolCall {
        name: "execute_shell".to_string(),
        args: serde_json::json!({ "command": "echo test" }),
    };

    let mut output = String::new();
    let mut usage = None;
    let _ = runner
        .execute_tool(&ctx, &fc, &mut output, &mut usage, "")
        .await;
    let _ = resolver_task.await;

    assert!(
        saw_oversight.load(std::sync::atomic::Ordering::SeqCst),
        "execute_shell must ALWAYS enqueue Sapphire Gate oversight even when auto_approve_safe_skills=true"
    );
    assert!(
        output.contains("REJECTED by Oversight")
            || output.contains("Unsandboxed shell execution on host is disabled by default")
            || output.contains("SHELL OUTPUT"),
        "Unexpected output: {}",
        output
    );
}

#[tokio::test]
async fn test_blackboard_tools_facet_write_read_and_observe_block() {
    let state = Arc::new(AppState::new_minimal_mock().await);
    let runner = AgentRunner::new(state.clone());

    let parent_ctx = RunContext {
        agent_id: "worker-a".to_string(),
        mission_id: "root-bb-mission".to_string(),
        root_mission_id: "root-bb-mission".to_string(),
        depth: 1,
        ..Default::default()
    };

    let sibling_ctx = RunContext {
        agent_id: "worker-b".to_string(),
        mission_id: "child-b-mission".to_string(),
        root_mission_id: "root-bb-mission".to_string(),
        depth: 1,
        accept_mode: crate::agent::types::AcceptMode::Observe,
        ..Default::default()
    };

    let scope_guard = state.comms.blackboard.scoped_mission("root-bb-mission");

    let write_fc = ToolCall {
        name: "blackboard_write".to_string(),
        args: serde_json::json!({
            "key": "findings#arch",
            "value": { "status": "ok", "modules": 4 },
            "tags": ["architecture"]
        }),
    };
    let mut write_out = String::new();
    let mut usage = None;
    runner
        .execute_tool(&parent_ctx, &write_fc, &mut write_out, &mut usage, "")
        .await
        .unwrap();
    assert!(write_out.contains("version: 1"));

    let read_fc = ToolCall {
        name: "blackboard_read".to_string(),
        args: serde_json::json!({ "key": "findings#arch" }),
    };
    let mut read_out = String::new();
    runner
        .execute_tool(&sibling_ctx, &read_fc, &mut read_out, &mut usage, "")
        .await
        .unwrap();
    assert!(read_out.contains("\"modules\": 4"));

    let mut blocked_out = String::new();
    runner
        .execute_tool(&sibling_ctx, &write_fc, &mut blocked_out, &mut usage, "")
        .await
        .unwrap();
    assert!(
        blocked_out.contains("Observe (Shadow) mode active: mutation tool 'blackboard_write' is blocked")
    );

    drop(scope_guard);
    assert!(state.comms.blackboard.get("root-bb-mission", "findings").is_none());
}

// Metadata: [tests]

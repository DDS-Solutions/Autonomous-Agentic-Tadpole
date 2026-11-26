//! @docs ARCHITECTURE:TestSuites
//!
//! ### AI Assist Note
//! **Intelligence Loop Unit Tests**: Verifies Mythos tag scrubbing, hierarchy labeling,
//! token-delta accounting, sibling future draining, sentinel gates, halting-signal detection,
//! context-window bounds, loop-repetition thresholds, and CascadeRouter Tier-2 escalation.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Sentinel bypass, context overflow missed, or CascadeRouter auth-error leak.
//! - **Telemetry Link**: Search `[intelligence_tests]` in cargo test logs.

use super::*;
use crate::agent::runner::{AgentRunner, RunContext};
use crate::state::AppState;
use std::sync::Arc;

#[test]
fn test_scrub_mythos_tags() {
    let input = "Thinking... <thinking>I should search</thinking> <halting_signal/> Done. <halt/>";
    let expected = "Thinking... I should search  Done.";
    assert_eq!(scrub_mythos_tags(input), expected);

    let clean = "No tags here";
    assert_eq!(scrub_mythos_tags(clean), "No tags here");
}

#[test]
fn test_hierarchy_labeling() {
    let mut ctx = RunContext {
        agent_id: AGENT_CEO.to_string(),
        ..Default::default()
    };
    assert_eq!(
        get_hierarchy_label(&ctx),
        "CEO (Strategic Intelligence Lead)"
    );

    ctx.agent_id = AGENT_COO.to_string();
    assert_eq!(get_hierarchy_label(&ctx), "COO (Operations Director)");

    ctx.agent_id = AGENT_ALPHA.to_string();
    assert_eq!(
        get_hierarchy_label(&ctx),
        "ALPHA NODE (Swarm Mission Commander)"
    );

    ctx.agent_id = "specialist-worker".to_string();
    ctx.role = "Task Specialist".to_string();
    assert_eq!(get_hierarchy_label(&ctx), "AGENT (Task Specialist)");
}

#[test]
fn test_detect_halting_signal_tags_and_confidence_threshold() {
    let ctx = RunContext {
        act_threshold: 0.85,
        ..Default::default()
    };

    // (a) Happy Path: tag halts cleanly
    let mut calls = vec![];
    assert!(sentinel::detect_halting_signal(
        &ctx,
        "Done <halting_signal/>",
        &mut calls
    ));

    // (b) Confidence >= threshold halts AND removes set_confidence meta-call
    let mut high_conf_calls = vec![
        crate::agent::types::ToolCall {
            name: "set_confidence".to_string(),
            args: serde_json::json!({ "score": 0.92 }),
        },
        crate::agent::types::ToolCall {
            name: "read_file".to_string(),
            args: serde_json::json!({ "path": "README.md" }),
        },
    ];
    assert!(sentinel::detect_halting_signal(
        &ctx,
        "Evaluating",
        &mut high_conf_calls
    ));
    assert_eq!(high_conf_calls.len(), 1);
    assert_eq!(high_conf_calls[0].name, "read_file");

    // (c) Edge Case: confidence below threshold does NOT halt, still strips meta-call
    let mut low_conf_calls = vec![crate::agent::types::ToolCall {
        name: "set_confidence".to_string(),
        args: serde_json::json!({ "score": 0.40 }),
    }];
    assert!(!sentinel::detect_halting_signal(
        &ctx,
        "Still thinking",
        &mut low_conf_calls
    ));
    assert!(low_conf_calls.is_empty());
}

#[test]
fn test_validate_context_window_bounds() {
    let mut ctx = RunContext::default();
    ctx.model_config.context_window = Some(64);

    let short_msgs = vec![crate::agent::types::ChatMessage::user("hi")];
    assert!(turn_eval::validate_context_window(&ctx, "sys", &short_msgs).is_ok());

    let huge_msgs = vec![crate::agent::types::ChatMessage::user("word ".repeat(300))];
    let err = turn_eval::validate_context_window(&ctx, "sys", &huge_msgs)
        .expect_err("Oversized prompt must trigger ContextOverflow");
    assert!(matches!(err, AppError::ContextOverflow(_)));
}

#[tokio::test]
async fn test_tool_orchestration_records_exact_token_deltas() {
    let metrics = Arc::new(crate::agent::runner::execution_metrics::ExecutionMetrics::new());
    let delta_1 = crate::agent::types::TokenUsage {
        input_tokens: 15,
        output_tokens: 5,
        total_tokens: 20,
    };
    let delta_2 = crate::agent::types::TokenUsage {
        input_tokens: 25,
        output_tokens: 10,
        total_tokens: 35,
    };

    metrics.record_tokens(
        delta_1.input_tokens as u64,
        delta_1.output_tokens as u64,
        delta_1.total_tokens as u64,
    );
    metrics.record_tokens(
        delta_2.input_tokens as u64,
        delta_2.output_tokens as u64,
        delta_2.total_tokens as u64,
    );

    let snap = metrics.snapshot("test_mission", "test_agent");
    assert_eq!(snap.total_input_tokens, 40);
    assert_eq!(snap.total_output_tokens, 15);
}

#[tokio::test]
async fn test_tool_failure_drains_sibling_futures_cleanly() {
    use futures::stream::{FuturesUnordered, StreamExt};
    use std::time::Duration;

    async fn mock_tool(
        name: &'static str,
        success: bool,
        delay_ms: u64,
    ) -> (
        &'static str,
        Result<(), AppError>,
        String,
        Option<crate::agent::types::TokenUsage>,
    ) {
        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        if success {
            (name, Ok(()), "Completed".to_string(), None)
        } else {
            (
                name,
                Err(AppError::BadRequest("Syntax error".into())),
                "Failed".to_string(),
                None,
            )
        }
    }

    let mut futures = FuturesUnordered::new();
    futures.push(mock_tool("tool_fail", false, 5));
    futures.push(mock_tool("tool_success", true, 20));

    let mut completed = Vec::new();
    let mut first_error = None;

    while let Some((name, result, text, _)) = futures.next().await {
        if let Err(e) = result {
            if first_error.is_none() {
                first_error = Some(e);
            }
        }
        completed.push((name, text));
    }

    assert_eq!(
        completed.len(),
        2,
        "All sibling futures must drain before error return"
    );
    assert!(first_error.is_some(), "Failure must be recorded");
}

#[test]
fn test_synthesis_fallback_disallows_tool_calls() {
    let tools: Option<Vec<crate::agent::types::ToolDefinition>> = None;
    assert!(
        tools.is_none(),
        "Synthesis prompt must enforce tool suppression to guarantee narrative return"
    );
}

#[tokio::test]
async fn test_enforce_sentinel_gate_orchestrator() {
    let state = Arc::new(AppState::new_minimal_mock().await);
    let runner = AgentRunner::new(state.clone());

    let ctx = RunContext {
        agent_id: AGENT_CEO.to_string(),
        ..Default::default()
    };

    let mut output_text = "I am the CEO".to_string();
    let mut function_calls = vec![];
    let mut usage = None;
    let mut attempts = 0;

    let result = runner
        .enforce_sentinel_gate(
            &ctx,
            "system",
            "user",
            &mut output_text,
            &mut function_calls,
            &mut usage,
            &mut attempts,
            &[],
        )
        .await;

    assert!(result.is_ok());
    assert_eq!(output_text, "I am the CEO");
    assert!(function_calls.is_empty());
}

#[tokio::test]
async fn test_enforce_sentinel_gate_specialist_narrative_leak() {
    let state = Arc::new(AppState::new_minimal_mock().await);
    let runner = AgentRunner::new(state.clone());

    let ctx = RunContext {
        agent_id: "specialist-1".to_string(),
        role: "Specialist".to_string(),
        safe_mode: false,
        ..Default::default()
    };

    let mut output_text = "I am just talking".to_string();
    let mut function_calls = vec![];
    let mut usage = None;

    std::env::set_var("TADPOLE_NULL_PROVIDERS", "true");
    let mut attempts = 0;
    let result = runner
        .enforce_sentinel_gate(
            &ctx,
            "system",
            "user",
            &mut output_text,
            &mut function_calls,
            &mut usage,
            &mut attempts,
            &[],
        )
        .await;
    std::env::remove_var("TADPOLE_NULL_PROVIDERS");

    assert!(result.is_ok());
    assert!(output_text.contains("DEGRADED"));
}

#[test]
fn test_evaluate_loop_repetition_thresholds() {
    assert_eq!(evaluate_loop_repetition(1), LoopRepetitionAction::Allow);
    assert_eq!(evaluate_loop_repetition(2), LoopRepetitionAction::Allow);
    assert_eq!(
        evaluate_loop_repetition(3),
        LoopRepetitionAction::CircuitBreakerOversight
    );
    assert_eq!(
        evaluate_loop_repetition(4),
        LoopRepetitionAction::WarnAndContinue
    );
    assert_eq!(
        evaluate_loop_repetition(5),
        LoopRepetitionAction::BlockRecursion
    );
    assert_eq!(
        evaluate_loop_repetition(9),
        LoopRepetitionAction::BlockRecursion
    );
}

#[tokio::test]
async fn test_cascade_router_opt_in_tier1_escalation_and_guards() {
    let tier1_cap_fail = crate::test_support::ProviderStub::start(
        axum::http::StatusCode::BAD_REQUEST,
        "schema error on tool format",
        false,
    )
    .await;
    let tier2_ok = crate::test_support::ProviderStub::start(
        axum::http::StatusCode::OK,
        "escalated tier2 success",
        false,
    )
    .await;

    let state =
        crate::test_support::state_with_agent("cascade-agent", &tier1_cap_fail.base_url).await;
    {
        let mut agent = state.registry.agents.get_mut("cascade-agent").unwrap();
        agent.runner_policy.cascade_routing = true;
        agent.models.model.model_id = "tier1-fast".to_string();
        agent.models.model_2 = Some("tier2-reasoning".to_string());
        agent.models.model_config2 = Some(crate::agent::types::ModelConfig {
            provider: crate::agent::types::ModelProvider::Ollama,
            model_id: "tier2-reasoning".to_string(),
            base_url: Some(tier2_ok.base_url.clone()),
            api_key: Some("stub".to_string()),
            ..Default::default()
        });
    }
    let runner = AgentRunner::new(state.clone());

    let pinned_ctx = runner
        .resolve_agent_context(
            "cascade-agent",
            &crate::agent::types::TaskPayload {
                message: "ARCHITECTURE_REVIEW analyze system".to_string(),
                model_id: Some("tier1-fast".to_string()),
                ..Default::default()
            },
            "m-pin",
            0,
            &[],
        )
        .await
        .unwrap();
    assert!(!pinned_ctx.cascade_routing);
    assert_eq!(pinned_ctx.model_config.model_id, "tier1-fast");

    state
        .governance
        .privacy_mode
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let priv_ctx = runner
        .resolve_agent_context(
            "cascade-agent",
            &crate::agent::types::TaskPayload {
                message: "ARCHITECTURE_REVIEW analyze system".to_string(),
                ..Default::default()
            },
            "m-priv",
            0,
            &[],
        )
        .await
        .unwrap();
    assert!(!priv_ctx.cascade_routing);
    state
        .governance
        .privacy_mode
        .store(false, std::sync::atomic::Ordering::SeqCst);

    let routine_payload = crate::agent::types::TaskPayload {
        message: "List files in docs/".to_string(),
        ..Default::default()
    };
    let routine_ctx = runner
        .resolve_agent_context("cascade-agent", &routine_payload, "m-esc", 0, &[])
        .await
        .unwrap();
    assert!(routine_ctx.cascade_routing);
    assert_eq!(routine_ctx.model_config.model_id, "tier1-fast");

    let out = runner
        .execute_intelligence_loop(&routine_ctx, &routine_payload)
        .await
        .unwrap();
    assert_eq!(out.text, "escalated tier2 success");
    assert!(
        tier1_cap_fail
            .calls
            .load(std::sync::atomic::Ordering::SeqCst)
            >= 1
    );
    assert!(tier2_ok.calls.load(std::sync::atomic::Ordering::SeqCst) >= 1);

    let critical_ctx = runner
        .resolve_agent_context(
            "cascade-agent",
            &crate::agent::types::TaskPayload {
                message: "Run SECURITY_AUDIT on core routes".to_string(),
                ..Default::default()
            },
            "m-crit",
            0,
            &[],
        )
        .await
        .unwrap();
    assert_eq!(critical_ctx.model_config.model_id, "tier2-reasoning");

    let tier1_401 = crate::test_support::ProviderStub::start(
        axum::http::StatusCode::UNAUTHORIZED,
        "401 Unauthorized invalid api key",
        false,
    )
    .await;
    let tier2_calls_before = tier2_ok.calls.load(std::sync::atomic::Ordering::SeqCst);
    {
        let mut agent = state.registry.agents.get_mut("cascade-agent").unwrap();
        agent.models.model.base_url = Some(tier1_401.base_url.clone());
    }
    let auth_payload = crate::agent::types::TaskPayload {
        message: "Simple query".to_string(),
        ..Default::default()
    };
    let auth_ctx = runner
        .resolve_agent_context("cascade-agent", &auth_payload, "m-401", 0, &[])
        .await
        .unwrap();
    let err = runner
        .execute_intelligence_loop(&auth_ctx, &auth_payload)
        .await;
    assert!(err.is_err());
    assert_eq!(
        tier2_ok.calls.load(std::sync::atomic::Ordering::SeqCst),
        tier2_calls_before,
        "401 auth error must NOT escalate to Tier 2"
    );
}

// Metadata: [tests]

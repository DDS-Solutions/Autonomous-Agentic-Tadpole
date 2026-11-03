//! @docs ARCHITECTURE:Networking
//!
//! ### AI Assist Note
//! **Oversight Subsystem Test Suite**: Adversarial verification suite testing happy paths,
//! input validation failure paths, and concurrency race conditions across oversight operations.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Assertion mismatch, mock state drift, or concurrency race.
//! - **Telemetry Link**: Search for `[oversight_tests]` in test logs.
//! - **Trace Scope**: `server-rs::routes::oversight::tests`

use super::*;
use crate::error::AppError;
use crate::routes::pagination::PaginationParams;
use crate::security::metering::{BudgetGuard, ResetPeriod};
use crate::state::AppState;
use crate::agent::types::{OversightDecision, OversightEntry, ToolCallAudit};
use axum::{
    body::to_bytes,
    extract::{Path, Query, State},
    response::IntoResponse,
    Json,
};
use serde_json::json;
use sqlx::Row;
use std::sync::Arc;

async fn response_json(resp: impl IntoResponse) -> serde_json::Value {
    let response = resp.into_response();
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn test_update_agent_quota_rejects_negative_nan_and_unauthenticated() {
    let state = Arc::new(AppState::new_minimal_mock().await);
    let principal = OversightPrincipal::global("operator-1");

    // 0. Empty principal rejected with Unauthorized
    let res_unauth = update_agent_quota(
        OversightPrincipal::global("   "),
        Path("agent-1".to_string()),
        State(state.clone()),
        Json(UpdateQuotaPayload {
            budget_usd: 10.0,
            reset_period: None,
        }),
    )
    .await;
    assert!(matches!(res_unauth, Err(AppError::Unauthorized(_))));

    // 1. Negative budget
    let res_neg = update_agent_quota(
        principal.clone(),
        Path("agent-1".to_string()),
        State(state.clone()),
        Json(UpdateQuotaPayload {
            budget_usd: -50.0,
            reset_period: None,
        }),
    )
    .await;
    assert!(matches!(res_neg, Err(AppError::BadRequest(_))));

    // 2. NaN budget
    let res_nan = update_agent_quota(
        principal.clone(),
        Path("agent-1".to_string()),
        State(state.clone()),
        Json(UpdateQuotaPayload {
            budget_usd: f64::NAN,
            reset_period: None,
        }),
    )
    .await;
    assert!(matches!(res_nan, Err(AppError::BadRequest(_))));

    // 3. Infinite budget
    let res_inf = update_agent_quota(
        principal,
        Path("agent-1".to_string()),
        State(state.clone()),
        Json(UpdateQuotaPayload {
            budget_usd: f64::INFINITY,
            reset_period: None,
        }),
    )
    .await;
    assert!(matches!(res_inf, Err(AppError::BadRequest(_))));
}

#[test]
fn test_oversight_decision_and_settings_validation_three_paths() {
    // (a) Happy Path: valid decisions & governance bounds
    assert_eq!(
        parse_oversight_decision("approved").unwrap(),
        ("approved", true)
    );
    assert_eq!(
        parse_oversight_decision("rejected").unwrap(),
        ("rejected", false)
    );
    let valid_settings = OversightSettingsPayload {
        auto_approve_safe_skills: Some(true),
        privacy_mode: Some(false),
        max_agents: Some(50),
        max_clusters: Some(10),
        max_swarm_depth: Some(5),
        max_task_length: Some(4096),
        default_budget_usd: Some(5.0),
    };
    assert!(validate_oversight_settings(&valid_settings).is_ok());

    // (b) Failure Path: invalid decision token, NaN budget, or exceeding ceiling limits
    assert!(matches!(
        parse_oversight_decision("maybe"),
        Err(AppError::BadRequest(_))
    ));
    let invalid_budget = OversightSettingsPayload {
        auto_approve_safe_skills: Some(false),
        privacy_mode: None,
        max_agents: None,
        max_clusters: None,
        max_swarm_depth: None,
        max_task_length: None,
        default_budget_usd: Some(f64::NAN),
    };
    assert!(matches!(
        validate_oversight_settings(&invalid_budget),
        Err(AppError::BadRequest(_))
    ));

    let above_ceiling = OversightSettingsPayload {
        auto_approve_safe_skills: None,
        privacy_mode: None,
        max_agents: Some(MAX_AGENTS_CEILING + 1),
        max_clusters: None,
        max_swarm_depth: None,
        max_task_length: None,
        default_budget_usd: None,
    };
    assert!(matches!(
        validate_oversight_settings(&above_ceiling),
        Err(AppError::BadRequest(_))
    ));

    // (c) Edge Case Path: case/whitespace-padded decision token and zero-valued governance limits
    assert_eq!(
        parse_oversight_decision("  APPROVED \n").unwrap(),
        ("approved", true)
    );
    let zero_depth = OversightSettingsPayload {
        auto_approve_safe_skills: None,
        privacy_mode: None,
        max_agents: None,
        max_clusters: None,
        max_swarm_depth: Some(0),
        max_task_length: None,
        default_budget_usd: Some(0.0),
    };
    assert!(matches!(
        validate_oversight_settings(&zero_depth),
        Err(AppError::BadRequest(_))
    ));
}

#[tokio::test]
async fn test_decide_oversight_end_to_end_attribution_and_idempotency() {
    let state = Arc::new(AppState::new_minimal_mock().await);
    let entry_id = "entry-e2e-1".to_string();
    let (tx, rx) = tokio::sync::oneshot::channel();

    state.comms.oversight_resolvers.insert(entry_id.clone(), tx);
    state.comms.oversight_queue.insert(
        entry_id.clone(),
        OversightEntry {
            id: entry_id.clone(),
            mission_id: None,
            tool_call: Some(ToolCallAudit {
                id: "tc-1".to_string(),
                agent_id: "agent-alpha".to_string(),
                mission_id: None,
                skill: "write_file".to_string(),
                params: json!({"path": "README.md"}),
                department: "engineering".to_string(),
                description: "Write README.md".to_string(),
                timestamp: chrono::Utc::now().to_rfc3339(),
            }),
            skill_proposal: None,
            status: "pending".to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
        },
    );

    let sample_secret = format!("{}-{}", "sk", "1234567890abcdef1234567890abcdef");
    let resp = decide_oversight(
        OversightPrincipal::global("operator-alice"),
        Path(entry_id.clone()),
        State(state.clone()),
        Json(OversightDecision {
            decision: "approved".to_string(),
            answer: Some(format!("Proceed with key {}", sample_secret)),
        }),
    )
    .await
    .expect("decide_oversight should succeed");

    let body = response_json(resp).await;
    assert_eq!(body["status"], "ok");
    assert_eq!(body["decision"], "approved");
    assert_eq!(body["decided_by"], "operator-alice");
    assert_eq!(body["delivered"], true);
    assert!(body["answer"].as_str().unwrap().contains("[REDACTED]"));
    assert!(!body["answer"].as_str().unwrap().contains(&sample_secret));

    // Resolver received `true`
    assert!(rx.await.unwrap());

    // DB row updated with operator attribution
    let row = sqlx::query("SELECT status, decision, decided_by FROM oversight_log WHERE id = ?")
        .bind(&entry_id)
        .fetch_one(&state.resources.pool)
        .await
        .unwrap();
    assert_eq!(row.try_get::<String, _>("status").unwrap(), "resolved");
    assert_eq!(row.try_get::<String, _>("decision").unwrap(), "approved");
    assert_eq!(
        row.try_get::<String, _>("decided_by").unwrap(),
        "operator-alice"
    );

    // Audit trail contains `oversight_decision` attributed to `operator-alice`
    let audit_row = sqlx::query(
        "SELECT user_id, action, params FROM audit_trail WHERE action = 'oversight_decision' LIMIT 1",
    )
    .fetch_one(&state.resources.pool)
    .await
    .unwrap();
    assert_eq!(
        audit_row.try_get::<Option<String>, _>("user_id").unwrap().as_deref(),
        Some("operator-alice")
    );

    // Second decision on the same ID returns 409 Conflict
    let second = decide_oversight(
        OversightPrincipal::global("operator-alice"),
        Path(entry_id),
        State(state),
        Json(OversightDecision {
            decision: "rejected".to_string(),
            answer: None,
        }),
    )
    .await;
    assert!(matches!(second, Err(AppError::Conflict(_))));
}

#[tokio::test]
async fn test_decide_oversight_db_failure_preserves_queue_and_does_not_unblock() {
    let state = Arc::new(AppState::new_minimal_mock().await);
    let entry_id = "entry-db-fail".to_string();
    let (tx, mut rx) = tokio::sync::oneshot::channel();

    sqlx::query(
        "INSERT INTO oversight_log (id, agent_id, entry_type, skill, params, status) \
         VALUES (?, 'agent-1', 'tool_call', 'bash', '{}', 'pending')",
    )
    .bind(&entry_id)
    .execute(&state.resources.pool)
    .await
    .unwrap();

    state.comms.oversight_resolvers.insert(entry_id.clone(), tx);
    state.comms.oversight_queue.insert(
        entry_id.clone(),
        OversightEntry {
            id: entry_id.clone(),
            mission_id: None,
            tool_call: None,
            skill_proposal: None,
            status: "pending".to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
        },
    );

    // Inject DB failure on oversight_log update
    sqlx::query(
        "CREATE TRIGGER abort_oversight_update BEFORE UPDATE ON oversight_log \
         BEGIN SELECT RAISE(ABORT, 'injected oversight_log failure'); END",
    )
    .execute(&state.resources.pool)
    .await
    .unwrap();

    let res = decide_oversight(
        OversightPrincipal::global("operator-1"),
        Path(entry_id.clone()),
        State(state.clone()),
        Json(OversightDecision {
            decision: "approved".to_string(),
            answer: None,
        }),
    )
    .await;

    assert!(res.is_err(), "DB update failure must not return 200 OK");
    assert!(
        state.comms.oversight_queue.contains_key(&entry_id),
        "Entry must remain in queue when DB persistence fails"
    );
    assert!(
        rx.try_recv().is_err(),
        "Waiting agent must not be unblocked when DB persistence fails"
    );
}

#[tokio::test]
async fn test_decide_oversight_mission_scoping_authorization() {
    let state = Arc::new(AppState::new_minimal_mock().await);
    let entry_id = "entry-scoped-1".to_string();

    state.comms.oversight_queue.insert(
        entry_id.clone(),
        OversightEntry {
            id: entry_id.clone(),
            mission_id: Some("mission-alpha".to_string()),
            tool_call: None,
            skill_proposal: None,
            status: "pending".to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
        },
    );

    // Principal scoped to mission-beta cannot resolve mission-alpha's entry
    let forbidden = decide_oversight(
        OversightPrincipal::scoped("op-beta", ["mission-beta"]),
        Path(entry_id.clone()),
        State(state.clone()),
        Json(OversightDecision {
            decision: "approved".to_string(),
            answer: None,
        }),
    )
    .await;
    assert!(matches!(forbidden, Err(AppError::Forbidden(_))));
    assert!(state.comms.oversight_queue.contains_key(&entry_id));

    // Principal scoped to mission-alpha succeeds
    let allowed = decide_oversight(
        OversightPrincipal::scoped("op-alpha", ["mission-alpha"]),
        Path(entry_id),
        State(state),
        Json(OversightDecision {
            decision: "approved".to_string(),
            answer: None,
        }),
    )
    .await;
    assert!(allowed.is_ok());
}

#[tokio::test]
async fn test_update_policy_global_upsert_deduplicates_null_agent_id() {
    let state = Arc::new(AppState::new_minimal_mock().await);
    let principal = OversightPrincipal::global("sec-admin");

    update_policy(
        principal.clone(),
        State(state.clone()),
        Json(UpdatePolicyPayload {
            tool_name: "custom_deploy_tool".to_string(),
            agent_id: None,
            mode: PolicyMode::Allow,
        }),
    )
    .await
    .unwrap();

    update_policy(
        principal,
        State(state.clone()),
        Json(UpdatePolicyPayload {
            tool_name: "custom_deploy_tool".to_string(),
            agent_id: None,
            mode: PolicyMode::Deny,
        }),
    )
    .await
    .unwrap();

    let (count, mode): (i64, String) = sqlx::query_as(
        "SELECT COUNT(*), COALESCE(MAX(mode), '') FROM permission_policies WHERE tool_name = 'custom_deploy_tool'",
    )
    .fetch_one(&state.resources.pool)
    .await
    .unwrap();
    assert_eq!(count, 1, "Repeated global policy updates must not insert duplicate NULL rows");
    assert_eq!(mode, "deny");

    let policies_json = response_json(get_policies(State(state)).await.unwrap()).await;
    let matching: Vec<_> = policies_json
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["tool_name"] == "custom_deploy_tool")
        .collect();
    assert_eq!(matching.len(), 1);
    assert!(matching[0].get("agent_id").is_some());
}

#[tokio::test]
async fn test_get_integrity_status_and_audit_trail_verification() {
    let state = Arc::new(AppState::new_minimal_mock().await);

    // 1. Empty chain -> EMPTY (not SECURE, score None)
    let empty_json =
        response_json(get_integrity_status(State(state.clone())).await.unwrap()).await;
    assert_eq!(empty_json["status"], "EMPTY");
    assert!(empty_json["integrity_score"].is_null());

    // 2. Record two valid entries -> UNSIGNED_CHAIN (or VERIFIED), is_verified == true
    let e1 = state
        .security
        .audit_trail
        .record("agent-1", None, Some("op-1"), "step_one", "{}")
        .await
        .unwrap();
    let _e2 = state
        .security
        .audit_trail
        .record("agent-1", None, Some("op-1"), "step_two", "{}")
        .await
        .unwrap();

    let valid_integrity =
        response_json(get_integrity_status(State(state.clone())).await.unwrap()).await;
    assert_eq!(valid_integrity["status"], "UNSIGNED_CHAIN");
    assert_eq!(valid_integrity["verified_count"], 2);
    assert_eq!(valid_integrity["total_count"], 2);
    assert!(valid_integrity["chain_head"].is_string());

    let trail_ok = response_json(
        get_audit_trail(
            State(state.clone()),
            Query(PaginationParams {
                page: 1,
                per_page: 10,
            }),
        )
        .await
        .unwrap(),
    )
    .await;
    let rows_ok = trail_ok["data"].as_array().unwrap();
    assert_eq!(rows_ok.len(), 2);
    assert!(rows_ok.iter().all(|r| r["is_verified"] == true));
    assert!(rows_ok.iter().all(|r| r["prev_hash"].is_string() && r["current_hash"].is_string()));

    // 3. Tamper with entry 1 -> get_audit_trail reports is_verified == false & get_integrity_status reports TAMPERED
    sqlx::query("UPDATE audit_trail SET action = 'forged_action' WHERE id = ?")
        .bind(&e1.id)
        .execute(&state.resources.pool)
        .await
        .unwrap();

    let trail_tampered = response_json(
        get_audit_trail(
            State(state.clone()),
            Query(PaginationParams {
                page: 1,
                per_page: 10,
            }),
        )
        .await
        .unwrap(),
    )
    .await;
    let tampered_entry = trail_tampered["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == e1.id)
        .unwrap();
    assert_eq!(tampered_entry["is_verified"], false);
    assert_eq!(tampered_entry["status"], "tampered");

    let tampered_integrity =
        response_json(get_integrity_status(State(state.clone())).await.unwrap()).await;
    assert_eq!(tampered_integrity["status"], "TAMPERED");

    // 4. Verification error (table dropped) -> UNAVAILABLE (not TAMPERED)
    sqlx::query("DROP TABLE audit_trail")
        .execute(&state.resources.pool)
        .await
        .unwrap();
    let unavail_integrity =
        response_json(get_integrity_status(State(state)).await.unwrap()).await;
    assert_eq!(unavail_integrity["status"], "UNAVAILABLE");
    assert!(unavail_integrity["integrity_score"].is_null());
    assert!(unavail_integrity["error"].is_string());
}

#[tokio::test]
async fn test_mission_quota_round_trip_and_null_reset_period_handling() {
    let state = Arc::new(AppState::new_minimal_mock().await);
    let principal = OversightPrincipal::global("op-quota");

    update_mission_quota(
        principal,
        Path("cluster-alpha".to_string()),
        State(state.clone()),
        Json(UpdateQuotaPayload {
            budget_usd: 42.5,
            reset_period: Some(ResetPeriod::Monthly),
        }),
    )
    .await
    .unwrap();

    let quotas_json = response_json(get_mission_quotas(State(state)).await.unwrap()).await;
    let quotas = quotas_json.as_array().unwrap();
    let found = quotas
        .iter()
        .find(|q| q["entity_id"] == "cluster-alpha")
        .expect("Updated mission quota must be returned by GET");
    assert_eq!(found["budget_usd"], 42.5);
    assert_eq!(found["reset_period"], "monthly");

    // Assert next_reset_at horizon is at least 25 days out for Monthly (§5.5)
    let next_reset_str = found["next_reset_at"]
        .as_str()
        .expect("next_reset_at must be populated for Monthly quota");
    let next_reset = chrono::DateTime::parse_from_rfc3339(next_reset_str)
        .expect("Valid RFC3339 timestamp")
        .with_timezone(&chrono::Utc);
    assert!(
        next_reset > chrono::Utc::now() + chrono::Duration::days(25),
        "Monthly quota reset horizon must be > 25 days out, was: {:?}",
        next_reset
    );

    // Verify NULL reset_period in SQLite does not panic
    assert_eq!(BudgetGuard::parse_reset_period(None), ResetPeriod::Never);
}

#[tokio::test]
async fn test_ledger_redaction_malformed_json_and_negative_duration_anomaly() {
    let state = Arc::new(AppState::new_minimal_mock().await);
    let sample_secret = format!("{}-{}", "sk", "1234567890abcdef1234567890abcdef");
    let decided_at = chrono::Utc::now();
    let predating_audit_ts = decided_at - chrono::Duration::seconds(5);

    // Row 1: secret in params + audit entry predating decided_at (timestamp anomaly)
    sqlx::query(
        "INSERT INTO oversight_log (id, mission_id, agent_id, entry_type, skill, params, status, decision, decided_at, decided_by, created_at, payload) \
         VALUES ('led-1', NULL, 'agent-1', 'tool_call', 'deploy', ?1, 'resolved', 'approved', ?2, 'op-1', ?3, ?1)",
    )
    .bind(format!(r#"{{"api_key":"{}"}}"#, sample_secret))
    .bind(decided_at)
    .bind(decided_at - chrono::Duration::seconds(10))
    .execute(&state.resources.pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO audit_trail (id, agent_id, mission_id, user_id, action, params, prev_hash, current_hash, timestamp) \
         VALUES ('aud-1', 'agent-1', NULL, 'op-1', '[SUCCESS] deploy', 'deployed ok', ?1, ?2, ?3)",
    )
    .bind("0".repeat(64))
    .bind("1".repeat(64))
    .bind(predating_audit_ts)
    .execute(&state.resources.pool)
    .await
    .unwrap();

    // Row 2: malformed JSON in params and NULL skill
    sqlx::query(
        "INSERT INTO oversight_log (id, mission_id, agent_id, entry_type, skill, params, status, decision, decided_at, decided_by, created_at, payload) \
         VALUES ('led-2', NULL, 'agent-1', 'tool_call', NULL, '{malformed_json', 'resolved', 'rejected', ?1, 'op-1', ?1, '')",
    )
    .bind(decided_at)
    .execute(&state.resources.pool)
    .await
    .unwrap();

    let ledger_json = response_json(
        get_ledger(
            State(state),
            Query(PaginationParams {
                page: 1,
                per_page: 25,
            }),
        )
        .await
        .unwrap(),
    )
    .await;
    let items = ledger_json["data"].as_array().unwrap();

    let item1 = items.iter().find(|i| i["id"] == "led-1").unwrap();
    assert_eq!(item1["tool_call"]["params"]["api_key"], "[REDACTED]");
    assert_eq!(item1["payload"]["api_key"], "[REDACTED]");
    assert_eq!(item1["result"]["timestamp_anomaly"], true);
    assert!(item1["result"]["duration_ms"].as_i64().unwrap() < 0);

    let item2 = items.iter().find(|i| i["id"] == "led-2").unwrap();
    assert!(
        item2["tool_call"]["params"]["_parse_error"].is_string(),
        "Malformed JSON must emit _parse_error rather than silent null"
    );
}

#[tokio::test]
async fn test_partial_update_settings_preserves_auto_approve_safe_skills() {
    let state = Arc::new(AppState::new_minimal_mock().await);

    // First configure auto_approve_safe_skills: true
    update_settings(
        OversightPrincipal::global("admin-1"),
        State(state.clone()),
        Json(OversightSettingsPayload {
            auto_approve_safe_skills: Some(true),
            privacy_mode: None,
            max_agents: None,
            max_clusters: None,
            max_swarm_depth: None,
            max_task_length: None,
            default_budget_usd: None,
        }),
    )
    .await
    .unwrap();

    // Now perform a partial update targeting only max_agents
    let resp = update_settings(
        OversightPrincipal::global("admin-1"),
        State(state.clone()),
        Json(OversightSettingsPayload {
            auto_approve_safe_skills: None,
            privacy_mode: None,
            max_agents: Some(25),
            max_clusters: None,
            max_swarm_depth: None,
            max_task_length: None,
            default_budget_usd: None,
        }),
    )
    .await
    .unwrap();

    let body = response_json(resp).await;
    assert_eq!(body["auto_approve_safe_skills"], true);
    assert_eq!(body["max_agents"], 25);
}

#[tokio::test]
async fn test_decide_oversight_happy_path_with_unredacted_guidance() {
    let state = Arc::new(AppState::new_minimal_mock().await);
    let entry_id = format!("hitl-happy-{}", uuid::Uuid::new_v4());
    let mission_id = "mission-alpha".to_string();

    sqlx::query("INSERT OR IGNORE INTO agents (id, name, role, department, description, status, metadata) VALUES ('agent-1', 'Test Agent', 'qa', 'eng', 'mock', 'idle', '{}')")
        .execute(&state.resources.pool)
        .await
        .unwrap();
    sqlx::query("INSERT OR IGNORE INTO mission_history (id, agent_id, title, status) VALUES (?, 'agent-1', 'Mission Alpha', 'active')")
        .bind(&mission_id)
        .execute(&state.resources.pool)
        .await
        .unwrap();

    sqlx::query(
        "INSERT INTO oversight_log (id, mission_id, agent_id, entry_type, skill, params, status) \
         VALUES (?, ?, 'agent-1', 'tool_call', 'deploy', '{}', 'pending')",
    )
    .bind(&entry_id)
    .bind(&mission_id)
    .execute(&state.resources.pool)
    .await
    .unwrap();

    let (tx, mut rx) = tokio::sync::oneshot::channel();
    state.comms.oversight_resolvers.insert(entry_id.clone(), tx);
    state.comms.oversight_queue.insert(
        entry_id.clone(),
        OversightEntry {
            id: entry_id.clone(),
            mission_id: Some(mission_id.clone()),
            tool_call: None,
            skill_proposal: None,
            status: "pending".to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
        },
    );

    let principal = OversightPrincipal::scoped("operator-alice", vec![mission_id.clone()]);
    let raw_guidance = "Use token Bearer test_token_secret_12345_abcdef for credentials";

    let resp = decide_oversight(
        principal,
        Path(entry_id.clone()),
        State(state.clone()),
        Json(OversightDecision {
            decision: "approved".to_string(),
            answer: Some(raw_guidance.to_string()),
        }),
    )
    .await
    .unwrap();

    // 1. Unblocked agent signal
    let approved = rx.try_recv().expect("Agent runner must receive signal");
    assert!(approved);

    // 2. Guidance delivered unredacted to agent runner (§2.3)
    let agent_guidance = state.comms.oversight_answers.get(&entry_id).map(|g| g.clone());
    assert_eq!(agent_guidance.as_deref(), Some(raw_guidance));

    // 3. Response JSON has secret redacted
    let body = response_json(resp).await;
    let ans_val = body["answer"].as_str().unwrap();
    assert!(ans_val.contains("[REDACTED]"), "Response answer must be redacted: {}", ans_val);

    // 4. Queue item removed
    assert!(!state.comms.oversight_queue.contains_key(&entry_id));

    // 5. DB row updated
    let row: (String, String) = sqlx::query_as(
        "SELECT status, decided_by FROM oversight_log WHERE id = ?",
    )
    .bind(&entry_id)
    .fetch_one(&state.resources.pool)
    .await
    .unwrap();
    assert_eq!(row.0, "resolved");
    assert_eq!(row.1, "operator-alice");
}

#[tokio::test]
async fn test_decide_oversight_failure_paths() {
    let state = Arc::new(AppState::new_minimal_mock().await);
    let entry_id = format!("hitl-fail-{}", uuid::Uuid::new_v4());
    let mission_id = "mission-classified".to_string();

    sqlx::query("INSERT OR IGNORE INTO agents (id, name, role, department, description, status, metadata) VALUES ('agent-1', 'Test Agent', 'qa', 'eng', 'mock', 'idle', '{}')")
        .execute(&state.resources.pool)
        .await
        .unwrap();
    sqlx::query("INSERT OR IGNORE INTO mission_history (id, agent_id, title, status) VALUES (?, 'agent-1', 'Mission Classified', 'active')")
        .bind(&mission_id)
        .execute(&state.resources.pool)
        .await
        .unwrap();

    sqlx::query(
        "INSERT INTO oversight_log (id, mission_id, agent_id, entry_type, skill, params, status) \
         VALUES (?, ?, 'agent-1', 'tool_call', 'vault_read', '{}', 'pending')",
    )
    .bind(&entry_id)
    .bind(&mission_id)
    .execute(&state.resources.pool)
    .await
    .unwrap();

    // 1. Mission scope mismatch -> 403 Forbidden
    let unauthorized = OversightPrincipal::scoped("operator-bob", vec!["mission-other"]);
    let err = match decide_oversight(
        unauthorized,
        Path(entry_id.clone()),
        State(state.clone()),
        Json(OversightDecision {
            decision: "approved".to_string(),
            answer: None,
        }),
    )
    .await
    {
        Err(e) => e,
        Ok(_) => panic!("Unauthorized mission access must be rejected"),
    };
    assert!(matches!(err, AppError::Forbidden(_)));

    // 2. Malformed decision token -> 400 BadRequest
    let authorized = OversightPrincipal::scoped("operator-bob", vec![mission_id.clone()]);
    let err_bad = match decide_oversight(
        authorized.clone(),
        Path(entry_id.clone()),
        State(state.clone()),
        Json(OversightDecision {
            decision: "maybe".to_string(),
            answer: None,
        }),
    )
    .await
    {
        Err(e) => e,
        Ok(_) => panic!("Invalid decision token must be rejected"),
    };
    assert!(matches!(err_bad, AppError::BadRequest(_)));

    // 3. Resolve first, then replay -> 409 Conflict
    decide_oversight(
        authorized.clone(),
        Path(entry_id.clone()),
        State(state.clone()),
        Json(OversightDecision {
            decision: "rejected".to_string(),
            answer: None,
        }),
    )
    .await
    .unwrap();

    let replay_err = match decide_oversight(
        authorized,
        Path(entry_id.clone()),
        State(state.clone()),
        Json(OversightDecision {
            decision: "approved".to_string(),
            answer: None,
        }),
    )
    .await
    {
        Err(e) => e,
        Ok(_) => panic!("Replay on resolved entry must return Conflict"),
    };
    assert!(matches!(replay_err, AppError::Conflict(_)));
}

#[tokio::test]
async fn test_decide_oversight_concurrency_race_single_winner() {
    let state = Arc::new(AppState::new_minimal_mock().await);
    let entry_id = format!("hitl-race-{}", uuid::Uuid::new_v4());
    let mission_id = "mission-concurrency".to_string();

    // Register in-memory queue only (simulating race before DB row committed)
    state.comms.oversight_queue.insert(
        entry_id.clone(),
        OversightEntry {
            id: entry_id.clone(),
            mission_id: Some(mission_id.clone()),
            tool_call: None,
            skill_proposal: None,
            status: "pending".to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
        },
    );

    let p1 = OversightPrincipal::global("admin-1");
    let p2 = OversightPrincipal::global("admin-2");
    let s1 = state.clone();
    let s2 = state.clone();
    let id1 = entry_id.clone();
    let id2 = entry_id.clone();

    let h1 = tokio::spawn(async move {
        decide_oversight(
            p1,
            Path(id1),
            State(s1),
            Json(OversightDecision {
                decision: "approved".to_string(),
                answer: None,
            }),
        )
        .await
    });

    let h2 = tokio::spawn(async move {
        decide_oversight(
            p2,
            Path(id2),
            State(s2),
            Json(OversightDecision {
                decision: "rejected".to_string(),
                answer: None,
            }),
        )
        .await
    });

    let (r1, r2) = tokio::join!(h1, h2);
    let res1 = r1.unwrap();
    let res2 = r2.unwrap();

    let ok_count = (res1.is_ok() as usize) + (res2.is_ok() as usize);
    let conflict_count = (matches!(res1, Err(AppError::Conflict(_))) as usize)
        + (matches!(res2, Err(AppError::Conflict(_))) as usize);

    assert_eq!(ok_count, 1, "Exactly one concurrent decide must succeed");
    assert_eq!(conflict_count, 1, "The loser must return 409 Conflict (never 500)");
}

#[tokio::test]
async fn test_get_settings_and_quota_audit_trail_recorded() {
    let state = Arc::new(AppState::new_minimal_mock().await);
    let principal = OversightPrincipal::global("super-admin");

    // 1. Test get_settings
    let get_resp = get_settings(State(state.clone())).await.unwrap();
    let get_json = response_json(get_resp).await;
    assert_eq!(get_json["status"], "ok");
    assert!(get_json.get("max_agents").is_some());

    // 2. Test update_agent_quota records audit trail (§3.7)
    update_agent_quota(
        principal.clone(),
        Path("agent-audit-test".to_string()),
        State(state.clone()),
        Json(UpdateQuotaPayload {
            budget_usd: 15.0,
            reset_period: Some(ResetPeriod::Monthly),
        }),
    )
    .await
    .unwrap();

    let audit_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_trail WHERE action = 'agent_quota_updated'",
    )
    .fetch_one(&state.resources.pool)
    .await
    .unwrap();
    assert!(audit_count >= 1, "Agent quota update must be audited in Merkle ledger");

    // 3. Test update_mission_quota records audit trail (§3.7)
    update_mission_quota(
        principal,
        Path("mission-audit-test".to_string()),
        State(state.clone()),
        Json(UpdateQuotaPayload {
            budget_usd: 50.0,
            reset_period: Some(ResetPeriod::Daily),
        }),
    )
    .await
    .unwrap();

    let mission_audit_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_trail WHERE action = 'mission_quota_updated'",
    )
    .fetch_one(&state.resources.pool)
    .await
    .unwrap();
    assert!(mission_audit_count >= 1, "Mission quota update must be audited in Merkle ledger");
}

#[test]
fn test_redact_json_value_scrubs_all_types_for_sensitive_keys() {
    let mut val = json!({
        "passcode": 12345,
        "secret_key": "raw-key-value",
        "is_token": true,
        "public_key": "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5",
        "idempotency_key": "idem-uuid-9999",
        "nested_auth": {
            "bearer_token": "sk-12345",
            "normal_field": "visible"
        }
    });

    redact_json_value(&mut val);

    assert_eq!(val["passcode"], "[REDACTED]");
    assert_eq!(val["secret_key"], "[REDACTED]");
    assert_eq!(val["is_token"], "[REDACTED]");
    assert_eq!(val["public_key"], "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5");
    assert_eq!(val["idempotency_key"], "idem-uuid-9999");
    assert_eq!(val["nested_auth"]["bearer_token"], "[REDACTED]");
    assert_eq!(val["nested_auth"]["normal_field"], "visible");
}

#[tokio::test]
async fn test_get_token_burn_fresh_install_fallback() {
    let state = Arc::new(AppState::new_minimal_mock().await);
    sqlx::query("INSERT OR IGNORE INTO agents (id, name, role, department, description, status, metadata, input_tokens, output_tokens, cost_usd) \
                 VALUES ('agent-tb', 'Telemetry Agent', 'worker', 'ops', 'burn tester', 'idle', '{}', 100, 50, 0.05)")
        .execute(&state.resources.pool)
        .await
        .unwrap();

    let resp = get_token_burn(State(state)).await.unwrap();
    let body = response_json(resp).await;
    assert!(body["totalTokensIn"].as_i64().is_some());
    assert!(body["totalTokensOut"].as_i64().is_some());
    assert!(body["totalCostUsd"].as_f64().is_some());
    let agents = body["agents"].as_array().unwrap();
    assert!(agents.iter().any(|a| a["agentId"] == "agent-tb"));
}


//! @docs ARCHITECTURE:Testing
//!
//! ### AI Assist Note
//! **Missions Route Tests**: Validates multiset line diff calculations, Merkle log hash chain verification,
//! SQL LIKE wildcard escaping, and Pillar IV 3-Path tests (Happy, Tamper Detection, Edge Cases).
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Tamper flag false-positive, Merkle chain regression, or diff multiset miscalculation.
//! - **Telemetry Link**: Search `[missions_tests]` in tracing logs.
//! - **Trace Scope**: `server-rs::routes::missions::tests`

#[cfg(test)]
mod tests {
    use axum::{
        extract::{Path, Query, State},
        Json,
    };
    use std::sync::Arc;

    use crate::state::AppState;
    use super::super::catalog::{escape_like_pattern, get_missions_catalog};
    use super::super::cas_diff::compute_line_diff_summary;
    use super::super::dossier::get_mission_dossier;
    use super::super::models::{CatalogQuery, PinMissionRequest};
    use super::super::pin::toggle_mission_pin;

    #[test]
    fn test_compute_line_diff_summary() {
        let pre = "line 1\nline 2\nline 3";
        let cur = "line 1\nline 2 modified\nline 3\nline 4";
        let summary = compute_line_diff_summary(pre, cur);

        assert_eq!(summary.additions, 2);
        assert_eq!(summary.deletions, 1);
        assert_eq!(summary.changes, 1);
    }

    #[test]
    fn test_compute_line_diff_summary_duplicate_lines() {
        let pre = "line\nline";
        let cur = "line\nline\nline\nline";
        let summary = compute_line_diff_summary(pre, cur);

        assert_eq!(summary.additions, 2);
        assert_eq!(summary.deletions, 0);
        assert_eq!(summary.changes, 0);
    }

    #[test]
    fn test_compute_line_diff_summary_identical() {
        let pre = "identical content\nsecond line";
        let cur = "identical content\nsecond line";
        let summary = compute_line_diff_summary(pre, cur);

        assert_eq!(summary.additions, 0);
        assert_eq!(summary.deletions, 0);
        assert_eq!(summary.changes, 0);
    }

    #[test]
    fn test_merkle_log_hash_verification() {
        let mission_id = "test-mission-123";
        let agent_id = "agent-456";
        let source = "System";
        let text = "Mission initialized";
        let timestamp = "2026-10-26T15:00:00Z";

        let hash1 = crate::agent::mission::compute_log_hash(
            None, mission_id, agent_id, source, text, timestamp,
        );
        assert!(!hash1.is_empty());

        let hash2 = crate::agent::mission::compute_log_hash(
            Some(&hash1),
            mission_id,
            agent_id,
            "Agent",
            "Next step executed",
            "2026-10-26T15:00:01Z",
        );
        assert_ne!(hash1, hash2);

        let verify_hash = crate::agent::mission::compute_log_hash(
            Some(&hash1),
            mission_id,
            agent_id,
            "Agent",
            "Next step executed",
            "2026-10-26T15:00:01Z",
        );
        assert_eq!(hash2, verify_hash);
    }

    #[test]
    fn test_escape_like_pattern() {
        assert_eq!(escape_like_pattern("test%search"), "%test\\%search%");
        assert_eq!(escape_like_pattern("test_search"), "%test\\_search%");
        assert_eq!(escape_like_pattern("test\\search"), "%test\\\\search%");
        assert_eq!(
            escape_like_pattern("100%_done\\here"),
            "%100\\%\\_done\\\\here%"
        );
        assert_eq!(escape_like_pattern("normal"), "%normal%");
    }

    /// Pillar IV 3-Path Test Suite: Happy Path, Adversarial Tamper Failure Path, and Edge Case Path
    #[tokio::test]
    async fn test_dossier_happy_tamper_and_edge_paths() {
        let state = Arc::new(AppState::new_minimal_mock().await);
        let pool = &state.resources.pool;

        // Seed test agent with all required NOT NULL columns
        sqlx::query(
            "INSERT OR IGNORE INTO agents (id, name, role, department, description, status, metadata, tokens_used)
             VALUES ('alpha-0', 'Alpha', 'Commander', 'Sovereign', 'Primary Commander', 'idle', '{}', 999999)",
        )
        .execute(pool)
        .await
        .unwrap();

        // 1. Create mission via authoritative orchestrator
        let mission =
            crate::agent::mission::create_mission(pool, "alpha-0", "Overlord Audit Mission", 5.0)
                .await
                .unwrap();
        crate::agent::mission::update_mission(
            pool,
            &mission.id,
            crate::agent::types::MissionStatus::Completed,
            0.05,
        )
        .await
        .unwrap();

        // (c.1) Edge Case: Empty logs -> COMPLETED_UNVERIFIED & lifetime agent tokens are NOT leaked into mission dossier
        let (_, Json(empty_dossier)) =
            get_mission_dossier(State(state.clone()), Path(mission.id.clone()))
                .await
                .unwrap();
        assert_eq!(
            empty_dossier.debrief.verdict.as_deref(),
            Some("COMPLETED_UNVERIFIED")
        );
        assert_eq!(empty_dossier.economics.total_tokens, 0);
        assert_eq!(empty_dossier.economics.total_tokens_source, "unavailable");

        // (a) Happy Path: Append 2 valid Merkle steps -> VERIFIED_SUCCESS
        let step1 = crate::agent::mission::log_step(
            pool,
            &mission.id,
            "alpha-0",
            "System",
            "Step 1 initialized",
            "info",
            Some(serde_json::json!({ "tokens_used": 420, "drift_score": "0.0125" })),
        )
        .await
        .unwrap();
        let step2 = crate::agent::mission::log_step(
            pool,
            &mission.id,
            "alpha-0",
            "Agent",
            "Step 2 completed",
            "success",
            Some(serde_json::json!({ "tokens_used": 180 })),
        )
        .await
        .unwrap();
        assert_eq!(step2.prev_hash, step1.hash);

        let (_, Json(valid_dossier)) =
            get_mission_dossier(State(state.clone()), Path(mission.id.clone()))
                .await
                .unwrap();
        assert_eq!(valid_dossier.logs.len(), 2);
        assert!(valid_dossier.logs.iter().all(|l| l.is_verified));
        assert_eq!(
            valid_dossier.debrief.verdict.as_deref(),
            Some("VERIFIED_SUCCESS")
        );
        assert_eq!(valid_dossier.economics.total_tokens, 600);
        assert_eq!(valid_dossier.economics.total_tokens_source, "logs");
        assert_eq!(valid_dossier.debrief.drift_score, Some(0.0125));

        // (b.1) Failure Path: Invalid catalog status filter returns 400 Bad Request, while 'ALL' succeeds
        let invalid_res = get_missions_catalog(
            State(state.clone()),
            Query(CatalogQuery {
                page: Some(1),
                per_page: Some(10),
                status: Some("not_a_real_status".to_string()),
                search: None,
                is_pinned: None,
                pinned_only: None,
            }),
        )
        .await;
        assert!(invalid_res.is_err());

        let (_, Json(all_catalog)) = get_missions_catalog(
            State(state.clone()),
            Query(CatalogQuery {
                page: Some(1),
                per_page: Some(10),
                status: Some("ALL".to_string()),
                search: None,
                is_pinned: None,
                pinned_only: None,
            }),
        )
        .await
        .unwrap();
        assert_eq!(all_catalog.total, 1);

        // (c.2) Edge Case: Idempotent Pin Request (repeating is_pinned=true stays true)
        let _ = toggle_mission_pin(
            State(state.clone()),
            None,
            Path(mission.id.clone()),
            Some(Json(PinMissionRequest {
                is_pinned: Some(true),
            })),
        )
        .await
        .unwrap();
        let _ = toggle_mission_pin(
            State(state.clone()),
            None,
            Path(mission.id.clone()),
            Some(Json(PinMissionRequest {
                is_pinned: Some(true),
            })),
        )
        .await
        .unwrap();
        let (_, Json(pinned_dossier)) =
            get_mission_dossier(State(state.clone()), Path(mission.id.clone()))
                .await
                .unwrap();
        assert!(pinned_dossier.mission.is_pinned);

        // (b.2) Failure Path: Out-of-band SQLite log tampering -> COMPLETED_WITH_TAMPER_FLAG
        sqlx::query("UPDATE mission_logs SET text = 'FORGED LOG ENTRY' WHERE id = ?")
            .bind(&step1.id)
            .execute(pool)
            .await
            .unwrap();

        let (_, Json(tampered_dossier)) =
            get_mission_dossier(State(state.clone()), Path(mission.id.clone()))
                .await
                .unwrap();
        assert!(!tampered_dossier.logs[0].is_verified);
        assert_eq!(
            tampered_dossier.debrief.verdict.as_deref(),
            Some("COMPLETED_WITH_TAMPER_FLAG")
        );
    }

    #[tokio::test]
    async fn test_legacy_unhashed_prefix_does_not_false_flag_tamper() {
        let state = Arc::new(AppState::new_minimal_mock().await);
        let pool = &state.resources.pool;

        sqlx::query(
            "INSERT OR IGNORE INTO agents (id, name, role, department, description, status, metadata)
             VALUES ('alpha-legacy', 'Alpha', 'Commander', 'Sovereign', 'Primary Commander', 'idle', '{}')",
        )
        .execute(pool)
        .await
        .unwrap();

        let mission = crate::agent::mission::create_mission(
            pool,
            "alpha-legacy",
            "Legacy Prefix Mission",
            5.0,
        )
        .await
        .unwrap();

        // Insert a legacy row with NULL hash/prev_hash first
        sqlx::query(
            "INSERT INTO mission_logs (id, mission_id, agent_id, source, text, severity, timestamp, hash, prev_hash)
             VALUES ('legacy-0', ?1, 'alpha-legacy', 'System', 'Legacy pre-hash step', 'info', '2026-10-30T00:00:00Z', NULL, NULL)",
        )
        .bind(&mission.id)
        .execute(pool)
        .await
        .unwrap();

        // Follow with two hashed steps
        let _s1 = crate::agent::mission::log_step(
            pool,
            &mission.id,
            "alpha-legacy",
            "Agent",
            "Hashed step 1",
            "info",
            None,
        )
        .await
        .unwrap();
        let _s2 = crate::agent::mission::log_step(
            pool,
            &mission.id,
            "alpha-legacy",
            "Agent",
            "Hashed step 2",
            "info",
            None,
        )
        .await
        .unwrap();

        crate::agent::mission::update_mission(
            pool,
            &mission.id,
            crate::agent::types::MissionStatus::Completed,
            0.01,
        )
        .await
        .unwrap();

        let (_, Json(dossier)) =
            get_mission_dossier(State(state.clone()), Path(mission.id.clone()))
                .await
                .unwrap();

        // Verdict must be COMPLETED_UNVERIFIED (due to legacy row), NEVER COMPLETED_WITH_TAMPER_FLAG
        assert_eq!(
            dossier.debrief.verdict.as_deref(),
            Some("COMPLETED_UNVERIFIED")
        );
        assert!(!dossier.logs[0].is_verified);
        assert!(dossier.logs[1].is_verified);
        assert!(dossier.logs[2].is_verified);
    }
}

// Metadata: [missions_tests]


//! @docs ARCHITECTURE:Storage
//!
//! ### AI Assist Note
//! **Missions Dossier Engine**: Orchestrates the aggregation of Merkle logs, CAS diffs,
//! knowledge graph entities, A2A economics, and Agent 99 automated debrief synthesis.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Mission not found, database error reading context, or debrief synthesis failure.
//! - **Telemetry Link**: Search `[missions_dossier]` in tracing logs.
//! - **Trace Scope**: `server-rs::routes::missions::dossier`

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool};
use std::collections::HashSet;
use std::sync::Arc;

use crate::error::AppError;
use crate::state::AppState;
use super::catalog::decode_mission_summary;
use super::cas_diff::{fetch_and_diff_cas_revisions, CasDiffsResult};
use super::merkle::{fetch_and_verify_mission_logs, VerifiedLogsResult};
use super::models::{
    CasDiffRecord, DebriefPayload, EconomicsPayload, MerkleStepRecord, MissionDossier,
    MissionSummaryItem, SwarmFindingRecord, TransactionLedgerRecord, TrustGraphEdge,
    TrustGraphNode, TrustGraphPayload, MAX_LOG_ROWS, MAX_PARTICIPANT_ADDRESSES,
};

pub async fn fetch_mission_header(
    pool: &SqlitePool,
    mission_id: &str,
) -> Result<MissionSummaryItem, AppError> {
    let mission_row = sqlx::query(
        r#"SELECT 
            m.id, 
            m.agent_id, 
            m.title, 
            m.status, 
            m.budget_usd, 
            m.cost_usd, 
            COALESCE(m.is_pinned, 0) as is_pinned,
            strftime('%Y-%m-%dT%H:%M:%SZ', m.created_at) as created_at_iso,
            strftime('%Y-%m-%dT%H:%M:%SZ', m.updated_at) as updated_at_iso,
            (SELECT COUNT(*) FROM mission_logs l WHERE l.mission_id = m.id) as log_count,
            (SELECT COUNT(DISTINCT r.file_path) FROM file_revisions r WHERE r.mission_id = m.id) as diff_count
        FROM mission_history m
        WHERE m.id = ?"#,
    )
    .bind(mission_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Mission not found: {}", mission_id)))?;

    decode_mission_summary(&mission_row)
}

pub async fn fetch_memory_palace(
    state: &AppState,
    mission_id: &str,
    partial: &mut bool,
    warnings: &mut Vec<String>,
) -> Vec<SwarmFindingRecord> {
    let context_rows = match sqlx::query(
        r#"SELECT 
            id, mission_id, agent_id, topic, finding, 
            strftime('%Y-%m-%dT%H:%M:%SZ', timestamp) as timestamp_iso
        FROM swarm_context
        WHERE mission_id = ?
        ORDER BY timestamp ASC
        LIMIT 500"#,
    )
    .bind(mission_id)
    .fetch_all(&state.resources.pool)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            *partial = true;
            let msg = "Failed to fetch swarm_context records for mission".to_string();
            tracing::error!(error = %e, mission_id = %mission_id, "{}", msg);
            warnings.push(msg);
            Vec::new()
        }
    };

    let mut memory_palace = Vec::with_capacity(context_rows.len());
    for r in context_rows {
        let raw_finding: String = r.try_get("finding").unwrap_or_default();
        memory_palace.push(SwarmFindingRecord {
            id: r.try_get("id").unwrap_or_default(),
            mission_id: r.try_get("mission_id").unwrap_or_default(),
            agent_id: r.try_get("agent_id").unwrap_or_default(),
            topic: r.try_get("topic").unwrap_or_default(),
            finding: state.security.secret_redactor.redact(&raw_finding),
            timestamp: r.try_get("timestamp_iso").unwrap_or_default(),
        });
    }
    memory_palace
}

pub async fn fetch_trust_graph(
    pool: &SqlitePool,
    mission_id: &str,
    partial: &mut bool,
    warnings: &mut Vec<String>,
) -> TrustGraphPayload {
    let entity_rows = match sqlx::query(
        r#"SELECT id, name, type, description, mission_id,
            COALESCE(CAST(created_at AS INTEGER), 0) as created_epoch
        FROM graph_entities
        WHERE mission_id = ?
        ORDER BY created_at ASC
        LIMIT 500"#,
    )
    .bind(mission_id)
    .fetch_all(pool)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            *partial = true;
            let msg = "Failed to fetch graph_entities for mission".to_string();
            tracing::error!(error = %e, mission_id = %mission_id, "{}", msg);
            warnings.push(msg);
            Vec::new()
        }
    };

    let mut entities = Vec::with_capacity(entity_rows.len());
    for r in entity_rows {
        entities.push(TrustGraphNode {
            id: r.try_get("id").unwrap_or_default(),
            name: r.try_get("name").unwrap_or_default(),
            node_type: r.try_get("type").unwrap_or_default(),
            description: r.try_get("description").ok(),
            mission_id: r.try_get("mission_id").ok(),
            created_at: r.try_get("created_epoch").unwrap_or(0),
        });
    }

    let relation_rows = match sqlx::query(
        r#"SELECT id, source_entity_id, target_entity_id, relation_type, weight, mission_id,
            COALESCE(CAST(created_at AS INTEGER), 0) as created_epoch
        FROM graph_relations
        WHERE mission_id = ?
        ORDER BY created_at ASC
        LIMIT 500"#,
    )
    .bind(mission_id)
    .fetch_all(pool)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            *partial = true;
            let msg = "Failed to fetch graph_relations for mission".to_string();
            tracing::error!(error = %e, mission_id = %mission_id, "{}", msg);
            warnings.push(msg);
            Vec::new()
        }
    };

    let mut relations = Vec::with_capacity(relation_rows.len());
    for r in relation_rows {
        relations.push(TrustGraphEdge {
            id: r.try_get("id").unwrap_or_default(),
            source_entity_id: r.try_get("source_entity_id").unwrap_or_default(),
            target_entity_id: r.try_get("target_entity_id").unwrap_or_default(),
            relation_type: r.try_get("relation_type").unwrap_or_default(),
            weight: r.try_get("weight").unwrap_or(1.0),
            mission_id: r.try_get("mission_id").ok(),
            created_at: r.try_get("created_epoch").unwrap_or(0),
        });
    }

    TrustGraphPayload {
        entities,
        relations,
    }
}

pub async fn build_economics_payload(
    pool: &SqlitePool,
    mission_id: &str,
    mission: &MissionSummaryItem,
    logs: &[MerkleStepRecord],
    memory_palace: &[SwarmFindingRecord],
    partial: &mut bool,
    warnings: &mut Vec<String>,
) -> EconomicsPayload {
    let mut participant_addresses: HashSet<String> = HashSet::new();
    participant_addresses.insert(mission_id.to_string());
    if !mission.agent_id.is_empty() {
        participant_addresses.insert(mission.agent_id.clone());
    }
    for l in logs {
        if !l.agent_id.is_empty() {
            participant_addresses.insert(l.agent_id.clone());
        }
    }
    for m in memory_palace {
        if !m.agent_id.is_empty() {
            participant_addresses.insert(m.agent_id.clone());
        }
    }

    let mut addrs: Vec<String> = participant_addresses.into_iter().collect();
    addrs.sort();
    addrs.truncate(MAX_PARTICIPANT_ADDRESSES);
    if addrs.is_empty() {
        addrs.push(mission_id.to_string());
    }

    let mut tx_qb = QueryBuilder::<Sqlite>::new(
        "SELECT tx_id, debit_agent_id, credit_agent_id, amount_micros, status, lock_id, CAST(strftime('%s', created_at) AS INTEGER) as created_epoch FROM a2a_ledger WHERE (debit_agent_id IN (",
    );
    let mut sep1 = tx_qb.separated(", ");
    for addr in &addrs {
        sep1.push_bind(addr);
    }
    tx_qb.push(") OR credit_agent_id IN (");
    let mut sep2 = tx_qb.separated(", ");
    for addr in &addrs {
        sep2.push_bind(addr);
    }
    tx_qb.push(")) ORDER BY created_at ASC LIMIT 500");

    let tx_rows = match tx_qb.build().fetch_all(pool).await {
        Ok(rows) => rows,
        Err(e) => {
            *partial = true;
            let msg = "Failed to fetch a2a_ledger records for mission".to_string();
            tracing::warn!(error = %e, mission_id = %mission_id, "{}", msg);
            warnings.push(msg);
            Vec::new()
        }
    };

    let mut transactions = Vec::with_capacity(tx_rows.len());
    for r in tx_rows {
        transactions.push(TransactionLedgerRecord {
            id: r.try_get("tx_id").unwrap_or_default(),
            transaction_type: "A2A_SETTLEMENT".to_string(),
            buyer_address: r.try_get("debit_agent_id").ok(),
            seller_address: r.try_get("credit_agent_id").ok(),
            asset_id: r.try_get("lock_id").ok(),
            amount: r.try_get("amount_micros").unwrap_or(0),
            status: r.try_get("status").unwrap_or_default(),
            audit_signature: None,
            created_at: r.try_get("created_epoch").unwrap_or(0),
        });
    }

    // Compute mission-scoped token rollup strictly from mission logs (never fall back to lifetime agents.tokens_used)
    let mut turn_tokens: u64 = 0;
    let mut saw_token_metadata = false;
    for l in logs {
        if let Some(m) = &l.metadata {
            if let Some(tu) = m
                .get("token_usage")
                .and_then(|u| u.get("total_tokens"))
                .and_then(|t| t.as_u64())
            {
                turn_tokens = turn_tokens.saturating_add(tu);
                saw_token_metadata = true;
            } else if let Some(tu) = m.get("tokens_used").and_then(|t| t.as_u64()) {
                turn_tokens = turn_tokens.saturating_add(tu);
                saw_token_metadata = true;
            } else if let Some(tu) = m.get("total_tokens").and_then(|t| t.as_u64()) {
                turn_tokens = turn_tokens.saturating_add(tu);
                saw_token_metadata = true;
            }
        }
    }

    let total_tokens_source = if saw_token_metadata {
        "logs".to_string()
    } else {
        "unavailable".to_string()
    };

    EconomicsPayload {
        total_cost_usd: mission.cost_usd,
        budget_usd: mission.budget_usd,
        total_tokens: turn_tokens,
        total_tokens_source,
        transactions,
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn synthesize_debrief_payload(
    state: &AppState,
    mission_id: &str,
    mission_status: &str,
    logs: &[MerkleStepRecord],
    diffs: &[CasDiffRecord],
    has_tamper: bool,
    has_unverified: bool,
    chain_truncated: bool,
    total_logs: i64,
    partial: &mut bool,
    warnings: &mut Vec<String>,
) -> DebriefPayload {
    let agent99_log: Option<String> = match sqlx::query_scalar(
        r#"SELECT text FROM mission_logs 
        WHERE mission_id = ?1 AND (agent_id = '99' OR source = 'Agent 99' OR LOWER(agent_id) LIKE '%auditor%')
        ORDER BY rowid DESC LIMIT 1"#,
    )
    .bind(mission_id)
    .fetch_optional(&state.resources.pool)
    .await
    {
        Ok(opt) => opt,
        Err(e) => {
            *partial = true;
            let msg = "Failed to fetch Agent 99 auditor log".to_string();
            tracing::warn!(error = %e, mission_id = %mission_id, "{}", msg);
            warnings.push(msg);
            None
        }
    };
    let agent99_notes = agent99_log.map(|t| state.security.secret_redactor.redact(&t));

    // Derive drift score from actual log metadata (supporting both numeric and string-encoded JSON numbers)
    let drift_score = logs.iter().rev().find_map(|l| {
        l.metadata
            .as_ref()
            .and_then(|m| m.get("drift_score"))
            .and_then(|v| {
                v.as_f64()
                    .or_else(|| v.as_str().and_then(|s| s.parse::<f64>().ok()))
            })
    });

    let mut recommendations = Vec::new();
    let error_count = logs
        .iter()
        .filter(|l| l.severity.eq_ignore_ascii_case("error"))
        .count();
    let warn_count = logs
        .iter()
        .filter(|l| {
            l.severity.eq_ignore_ascii_case("warning") || l.severity.eq_ignore_ascii_case("warn")
        })
        .count();
    let is_empty = logs.is_empty();

    if has_tamper {
        recommendations.push(
            "CRITICAL: Merkle hash-chain or CAS snapshot anomaly detected. Validate disk logs against remote backup."
                .to_string(),
        );
    } else if chain_truncated {
        recommendations.push(format!(
            "NOTICE: Mission flight logs exceed {} entries ceiling ({} total); tail truncation prevented complete Merkle verification.",
            MAX_LOG_ROWS, total_logs
        ));
    } else if has_unverified {
        recommendations.push(
            "NOTICE: One or more log entries lack Merkle hashes or started from an unanchored legacy record."
                .to_string(),
        );
    } else if is_empty {
        recommendations
            .push("NOTICE: No execution step logs recorded for this mission.".to_string());
    }

    if error_count > 0 {
        recommendations.push(format!(
            "Mission logged {} error events. Inspect Merkle log entries with error severity.",
            error_count
        ));
    }
    if warn_count > 3 {
        recommendations.push(format!(
            "High warning density ({} warnings). Review rate limits or tool parameter tolerances.",
            warn_count
        ));
    }
    if diffs.is_empty() {
        recommendations.push(
            "No CAS file mutations recorded for this mission (read-only execution).".to_string(),
        );
    } else {
        recommendations.push(format!(
            "Verified {} file mutations preserved in pre-mutation revision storage.",
            diffs.len()
        ));
    }

    // Tri-state verification verdict with chain truncation guard
    let verdict = match mission_status.to_lowercase().as_str() {
        "completed" => {
            if has_tamper {
                Some("COMPLETED_WITH_TAMPER_FLAG".to_string())
            } else if is_empty || has_unverified || chain_truncated {
                Some("COMPLETED_UNVERIFIED".to_string())
            } else {
                Some("VERIFIED_SUCCESS".to_string())
            }
        }
        "failed" => {
            if has_tamper {
                Some("FAILED_WITH_TAMPER_FLAG".to_string())
            } else if is_empty || has_unverified || chain_truncated {
                Some("FAILED_UNVERIFIED".to_string())
            } else {
                Some("FAILED_VERIFIED".to_string())
            }
        }
        "active" | "running" => Some("IN_PROGRESS".to_string()),
        _ => Some("STATUS_NOMINAL".to_string()),
    };

    DebriefPayload {
        agent99_notes,
        drift_score,
        verdict,
        recommendations,
    }
}

/// GET /v1/missions/:id/dossier
///
/// Retrieves complete, multi-category flight recorder dossier for a single mission.
#[tracing::instrument(skip(state), name = "missions::get_mission_dossier")]
pub async fn get_mission_dossier(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<(StatusCode, Json<MissionDossier>), AppError> {
    let pool = &state.resources.pool;
    let mission_id = id.trim();
    if mission_id.is_empty() {
        return Err(AppError::BadRequest(
            "Mission ID cannot be empty".to_string(),
        ));
    }

    let mut partial = false;
    let mut warnings: Vec<String> = Vec::new();

    // 1. Fetch Mission Header
    let mission = fetch_mission_header(pool, mission_id).await?;

    // 2. Fetch Mission Logs & Compute Merkle Verification (`rowid ASC`)
    let VerifiedLogsResult {
        logs,
        total_logs,
        chain_truncated,
        mut has_tamper,
        has_unverified,
    } = fetch_and_verify_mission_logs(&state, mission_id, &mut partial, &mut warnings).await?;

    // 3. Fetch Lean CAS File Revisions & Generate Content Diffs
    let CasDiffsResult {
        diffs,
        diffs_total,
        diffs_truncated,
    } = fetch_and_diff_cas_revisions(
        &state,
        mission_id,
        mission.diff_count,
        &mut has_tamper,
        &mut partial,
        &mut warnings,
    )
    .await;

    // 4. Fetch Swarm Context (Memory Palace)
    let memory_palace = fetch_memory_palace(&state, mission_id, &mut partial, &mut warnings).await;

    // 5. Fetch TrustGraph (Nodes & Relations)
    let trust_graph = fetch_trust_graph(pool, mission_id, &mut partial, &mut warnings).await;

    // 6. Fetch Economics & A2A Ledger + Mission-Scoped Token Rollup
    let economics = build_economics_payload(
        pool,
        mission_id,
        &mission,
        &logs,
        &memory_palace,
        &mut partial,
        &mut warnings,
    )
    .await;

    // 7. Synthesize Agent 99 Debrief & Tri-State Verdict
    let debrief = synthesize_debrief_payload(
        &state,
        mission_id,
        &mission.status,
        &logs,
        &diffs,
        has_tamper,
        has_unverified,
        chain_truncated,
        total_logs,
        &mut partial,
        &mut warnings,
    )
    .await;

    let dossier = MissionDossier {
        economics,
        mission,
        logs,
        diffs,
        memory_palace,
        trust_graph,
        debrief,
        logs_total: total_logs.max(0) as u64,
        logs_truncated: chain_truncated,
        diffs_total,
        diffs_truncated,
        partial,
        warnings,
    };

    Ok((StatusCode::OK, Json(dossier)))
}

// Metadata: [missions_dossier]


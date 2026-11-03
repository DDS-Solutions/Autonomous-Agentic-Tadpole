//! @docs ARCHITECTURE:Storage
//!
//! ### AI Assist Note
//! **Missions Merkle Flight Log Verifier**: Strictly reads log columns and
//! validates backward cryptographic hash chains (`prev_hash` -> `hash`) to detect tampering.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Hash mismatch in chain, missing genesis block, or malformed metadata JSON.
//! - **Telemetry Link**: Search `[missions_merkle]` in tracing logs.
//! - **Trace Scope**: `server-rs::routes::missions::merkle`

use sqlx::{sqlite::SqliteRow, Row};

use crate::error::AppError;
use crate::state::AppState;
use super::models::{MerkleStepRecord, MAX_LOG_ROWS};

/// Cursor tracking the expected previous hash across a mission's log chain.
#[derive(Debug, Clone)]
pub enum ChainCursor {
    NotStarted,
    Active(String),
}

pub struct RawLogRow {
    pub id: String,
    pub agent_id: String,
    pub source: String,
    pub raw_text: String,
    pub severity: String,
    pub timestamp_str: String,
    pub meta_raw: Option<String>,
    pub hash: Option<String>,
    pub prev_hash: Option<String>,
}

/// Strictly decodes all columns participating in Merkle verification so NULLs or type
/// mismatches never silently fabricate default values into `compute_log_hash`.
pub fn read_log_fields(r: &SqliteRow) -> Result<RawLogRow, sqlx::Error> {
    Ok(RawLogRow {
        id: r.try_get("id")?,
        agent_id: r.try_get("agent_id")?,
        source: r.try_get("source")?,
        raw_text: r.try_get("text")?,
        severity: r.try_get("severity")?,
        timestamp_str: r.try_get("timestamp")?,
        meta_raw: r.try_get("metadata")?,
        hash: r.try_get("hash")?,
        prev_hash: r.try_get("prev_hash")?,
    })
}

pub struct VerifiedLogsResult {
    pub logs: Vec<MerkleStepRecord>,
    pub total_logs: i64,
    pub chain_truncated: bool,
    pub has_tamper: bool,
    pub has_unverified: bool,
}

pub async fn fetch_and_verify_mission_logs(
    state: &AppState,
    mission_id: &str,
    partial: &mut bool,
    warnings: &mut Vec<String>,
) -> Result<VerifiedLogsResult, AppError> {
    let pool = &state.resources.pool;

    // Propagate count query errors with `?` so a failed count can never mask truncation.
    let total_logs: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM mission_logs WHERE mission_id = ?")
            .bind(mission_id)
            .fetch_one(pool)
            .await?;
    let chain_truncated = total_logs > MAX_LOG_ROWS;

    let log_rows = sqlx::query(
        r#"SELECT 
            id, mission_id, agent_id, source, text, severity, 
            timestamp, metadata, hash, prev_hash
        FROM mission_logs
        WHERE mission_id = ?
        ORDER BY rowid ASC
        LIMIT ?"#,
    )
    .bind(mission_id)
    .bind(MAX_LOG_ROWS)
    .fetch_all(pool)
    .await?;

    let mut logs = Vec::with_capacity(log_rows.len());
    let mut cursor = ChainCursor::NotStarted;
    let mut has_tamper = false;
    let mut has_unverified = false;

    for r in log_rows {
        let RawLogRow {
            id: log_id,
            agent_id,
            source,
            raw_text,
            severity,
            timestamp_str,
            meta_raw,
            hash,
            prev_hash,
        } = match read_log_fields(&r) {
            Ok(fields) => fields,
            Err(e) => {
                has_unverified = true;
                *partial = true;
                let msg = format!(
                    "Malformed mission_logs row excluded from Merkle verification: {}",
                    e
                );
                tracing::error!(mission_id = %mission_id, error = %e, "{}", msg);
                warnings.push(msg);
                continue;
            }
        };

        let curr_clean = hash.as_deref().filter(|s| !s.is_empty());
        let prev_clean = prev_hash.as_deref().filter(|s| !s.is_empty());

        let is_verified = match curr_clean {
            Some(curr) => {
                let computed = crate::agent::mission::compute_log_hash(
                    prev_clean,
                    mission_id,
                    &agent_id,
                    &source,
                    &raw_text,
                    &timestamp_str,
                );
                let hash_matches = curr == computed;

                let link_ok = match &cursor {
                    ChainCursor::NotStarted => {
                        if prev_clean.is_some() {
                            // Chain started after legacy unhashed rows or pruned prefix:
                            // mark unverified genesis rather than false-positive tamper.
                            has_unverified = true;
                        }
                        true
                    }
                    ChainCursor::Active(expected_prev) => {
                        prev_clean == Some(expected_prev.as_str())
                    }
                };

                let valid = hash_matches && link_ok;
                if !valid {
                    has_tamper = true;
                    tracing::warn!(
                        mission_id = %mission_id,
                        log_id = %log_id,
                        link_ok = %link_ok,
                        hash_matches = %hash_matches,
                        "⚠️ [missions] Merkle hash verification mismatch detected in mission flight log step"
                    );
                }
                cursor = ChainCursor::Active(curr.to_string());
                valid
            }
            None => {
                // Legacy unhashed entry
                has_unverified = true;
                cursor = ChainCursor::NotStarted;
                false
            }
        };

        // S-01: Apply secret redactor to text and metadata before constructing response
        let text = state.security.secret_redactor.redact(&raw_text);
        let metadata: Option<serde_json::Value> = meta_raw
            .map(|s| state.security.secret_redactor.redact(&s))
            .filter(|s| !s.trim().is_empty())
            .and_then(|s| {
                serde_json::from_str(&s)
                    .map_err(|e| {
                        tracing::warn!(
                            mission_id = %mission_id,
                            log_id = %log_id,
                            error = %e,
                            "Mission log metadata is not valid JSON after redaction"
                        );
                        e
                    })
                    .ok()
            });

        logs.push(MerkleStepRecord {
            id: log_id,
            mission_id: mission_id.to_string(),
            agent_id,
            source,
            text,
            severity,
            timestamp: timestamp_str,
            metadata,
            hash,
            prev_hash,
            is_verified,
        });
    }

    Ok(VerifiedLogsResult {
        logs,
        total_logs,
        chain_truncated,
        has_tamper,
        has_unverified,
    })
}

// Metadata: [missions_merkle]

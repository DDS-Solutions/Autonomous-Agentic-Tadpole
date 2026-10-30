//! @docs ARCHITECTURE:Gateways
//!
//! ### AI Assist Note
//! **missions**: Sovereign Observability Core (SOC) HTTP routes for Overlord Entity (0).
//! Provides the Mission Run Catalog (`GET /v1/missions`), Cryptographic Flight Dossier
//! (`GET /v1/missions/{id}/dossier`), and Reaper Retention Pinning (`POST /v1/missions/{id}/pin`).
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: `AppError::BadRequest`, `AppError::NotFound`, `AppError::Sqlx`, or Merkle hash verification mismatch.
//! - **Telemetry Link**: Search `[missions]` in tracing logs.
//! - **Trace Scope**: `server-rs::routes::missions`

use crate::error::AppError;
use crate::middleware::auth::AuthenticatedOperator;
use crate::state::AppState;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{sqlite::SqliteRow, QueryBuilder, Row, Sqlite, SqlitePool};
use std::collections::{HashMap, HashSet};
use std::path::Path as StdPath;
use std::sync::Arc;

// ─────────────────────────────────────────────────────────
//  RESOURCE & PAYLOAD CEILING CONSTANTS
// ─────────────────────────────────────────────────────────

/// Maximum number of Merkle flight log steps returned in a single dossier payload.
pub const MAX_LOG_ROWS: i64 = 2000;
/// Maximum number of distinct file revision entries returned in a single dossier payload.
pub const MAX_REVISION_ROWS: i64 = 500;
/// Maximum byte size (256 KB) for an individual pre-mutation CAS blob or live disk preview.
pub const MAX_FILE_PREVIEW_BYTES: usize = 256 * 1024;
/// Maximum aggregate byte budget (4 MB) for all file diff previews combined in a single dossier.
pub const MAX_DOSSIER_DIFF_BYTES: usize = 4 * 1024 * 1024;
/// Maximum number of distinct participant addresses bound in the A2A ledger query.
pub const MAX_PARTICIPANT_ADDRESSES: usize = 50;

/// Known canonical mission statuses allowed in catalog filtering.
const VALID_MISSION_STATUSES: &[&str] = &[
    "pending",
    "active",
    "running",
    "completed",
    "failed",
    "paused",
    "cancelled",
    "aborted",
];

/// Escapes SQL LIKE wildcards (% and _) and backslashes for exact substring matching.
fn escape_like_pattern(input: &str) -> String {
    let escaped = input
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("%{}%", escaped)
}

// ─────────────────────────────────────────────────────────
//  DATA CONTRACTS
// ─────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CatalogQuery {
    pub page: Option<u32>,
    pub per_page: Option<u32>,
    pub status: Option<String>,
    pub search: Option<String>,
    pub is_pinned: Option<bool>,
    pub pinned_only: Option<bool>,
}

#[derive(Debug, Deserialize, Default)]
pub struct PinMissionRequest {
    pub is_pinned: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MissionSummaryItem {
    pub id: String,
    pub agent_id: String,
    pub title: String,
    pub status: String,
    pub budget_usd: f64,
    pub cost_usd: f64,
    pub is_pinned: bool,
    pub created_at: String,
    pub updated_at: String,
    pub log_count: i64,
    pub diff_count: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MerkleStepRecord {
    pub id: String,
    pub mission_id: String,
    pub agent_id: String,
    pub source: String,
    pub text: String,
    pub severity: String,
    pub timestamp: String,
    pub metadata: Option<serde_json::Value>,
    pub hash: Option<String>,
    pub prev_hash: Option<String>,
    pub is_verified: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CasDiffSummary {
    pub additions: usize,
    pub deletions: usize,
    pub changes: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CasDiffRecord {
    pub id: i64,
    pub file_path: String,
    pub version_num: i64,
    pub hash: String,
    pub size_bytes: i64,
    pub created_at: String,
    pub pre_mutation_content: String,
    pub current_content: String,
    pub diff_summary: CasDiffSummary,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SwarmFindingRecord {
    pub id: String,
    pub mission_id: String,
    pub agent_id: String,
    pub topic: String,
    pub finding: String,
    pub timestamp: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TrustGraphNode {
    pub id: String,
    pub name: String,
    pub node_type: String,
    pub description: Option<String>,
    pub mission_id: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TrustGraphEdge {
    pub id: String,
    pub source_entity_id: String,
    pub target_entity_id: String,
    pub relation_type: String,
    pub weight: f64,
    pub mission_id: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TrustGraphPayload {
    pub entities: Vec<TrustGraphNode>,
    pub relations: Vec<TrustGraphEdge>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TransactionLedgerRecord {
    pub id: String,
    pub transaction_type: String,
    pub buyer_address: Option<String>,
    pub seller_address: Option<String>,
    pub asset_id: Option<String>,
    pub amount: i64,
    pub status: String,
    pub audit_signature: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EconomicsPayload {
    pub total_cost_usd: f64,
    pub budget_usd: f64,
    #[serde(default)]
    pub total_tokens: u64,
    #[serde(default)]
    pub total_tokens_source: String,
    pub transactions: Vec<TransactionLedgerRecord>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DebriefPayload {
    pub agent99_notes: Option<String>,
    pub drift_score: Option<f64>,
    pub verdict: Option<String>,
    pub recommendations: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MissionDossier {
    pub mission: MissionSummaryItem,
    pub logs: Vec<MerkleStepRecord>,
    pub diffs: Vec<CasDiffRecord>,
    pub memory_palace: Vec<SwarmFindingRecord>,
    pub trust_graph: TrustGraphPayload,
    pub economics: EconomicsPayload,
    pub debrief: DebriefPayload,
    #[serde(default)]
    pub logs_total: u64,
    #[serde(default)]
    pub logs_truncated: bool,
    #[serde(default)]
    pub diffs_total: u64,
    #[serde(default)]
    pub diffs_truncated: bool,
    #[serde(default)]
    pub partial: bool,
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CatalogResponse {
    pub success: bool,
    pub data: Vec<MissionSummaryItem>,
    pub total: i64,
    pub page: u32,
    pub per_page: u32,
}

// ─────────────────────────────────────────────────────────
//  CATALOG FILTER BUILDER & MERKLE HELPERS
// ─────────────────────────────────────────────────────────

struct MissionCatalogFilters {
    status: Option<String>,
    is_pinned: Option<bool>,
    search_pattern: Option<String>,
}

impl MissionCatalogFilters {
    fn from_query(query: &CatalogQuery) -> Result<Self, AppError> {
        let raw_status = query.status.as_deref().unwrap_or("").trim();
        let status = if raw_status.is_empty() || raw_status.eq_ignore_ascii_case("all") {
            None
        } else {
            let normalized = raw_status.to_ascii_lowercase();
            if !VALID_MISSION_STATUSES.contains(&normalized.as_str()) {
                return Err(AppError::BadRequest(format!(
                    "Invalid mission status filter '{}'. Expected 'all' or one of: {}",
                    raw_status,
                    VALID_MISSION_STATUSES.join(", ")
                )));
            }
            Some(normalized)
        };

        let is_pinned = query.is_pinned.or(match query.pinned_only {
            Some(true) => Some(true),
            _ => None,
        });

        let raw_search = query.search.as_deref().unwrap_or("").trim();
        let search_pattern = if raw_search.is_empty() {
            None
        } else {
            Some(escape_like_pattern(raw_search))
        };

        Ok(Self {
            status,
            is_pinned,
            search_pattern,
        })
    }

    fn apply(&self, qb: &mut QueryBuilder<'_, Sqlite>) {
        if let Some(ref st) = self.status {
            qb.push(" AND status = ");
            qb.push_bind(st.clone());
        }
        if let Some(pinned) = self.is_pinned {
            qb.push(" AND COALESCE(is_pinned, 0) = ");
            qb.push_bind(if pinned { 1 } else { 0 });
        }
        if let Some(ref pattern) = self.search_pattern {
            qb.push(" AND (title LIKE ");
            qb.push_bind(pattern.clone());
            qb.push(" ESCAPE '\\' OR id LIKE ");
            qb.push_bind(pattern.clone());
            qb.push(" ESCAPE '\\' OR agent_id LIKE ");
            qb.push_bind(pattern.clone());
            qb.push(" ESCAPE '\\')");
        }
    }
}

/// Cursor tracking the expected previous hash across a mission's log chain.
enum ChainCursor {
    NotStarted,
    Active(String),
}

struct RawLogRow {
    id: String,
    agent_id: String,
    source: String,
    raw_text: String,
    severity: String,
    timestamp_str: String,
    meta_raw: Option<String>,
    hash: Option<String>,
    prev_hash: Option<String>,
}

/// Strictly decodes all columns participating in Merkle verification so NULLs or type
/// mismatches never silently fabricate default values into `compute_log_hash`.
fn read_log_fields(r: &SqliteRow) -> Result<RawLogRow, sqlx::Error> {
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

/// Decodes a mission summary row strictly for mandatory identity/state columns.
fn decode_mission_summary(r: &SqliteRow) -> Result<MissionSummaryItem, AppError> {
    let is_pinned_raw: i64 = r.try_get("is_pinned").unwrap_or(0);
    let created_at: Option<String> = r.try_get("created_at_iso")?;
    let updated_at: Option<String> = r.try_get("updated_at_iso")?;

    Ok(MissionSummaryItem {
        id: r.try_get("id")?,
        agent_id: r.try_get("agent_id")?,
        title: r.try_get("title")?,
        status: r.try_get("status")?,
        budget_usd: r.try_get("budget_usd").unwrap_or(0.0),
        cost_usd: r.try_get("cost_usd").unwrap_or(0.0),
        is_pinned: is_pinned_raw == 1,
        created_at: created_at.unwrap_or_default(),
        updated_at: updated_at.unwrap_or_default(),
        log_count: r.try_get("log_count").unwrap_or(0),
        diff_count: r.try_get("diff_count").unwrap_or(0),
    })
}

// ─────────────────────────────────────────────────────────
//  DOSSIER DOMAIN PIPELINE HELPERS
// ─────────────────────────────────────────────────────────

struct VerifiedLogsResult {
    logs: Vec<MerkleStepRecord>,
    total_logs: i64,
    chain_truncated: bool,
    has_tamper: bool,
    has_unverified: bool,
}

struct CasDiffsResult {
    diffs: Vec<CasDiffRecord>,
    diffs_total: u64,
    diffs_truncated: bool,
}

async fn fetch_mission_header(
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

async fn fetch_and_verify_mission_logs(
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

/// Safely resolves and reads the live workspace file preview within `MAX_FILE_PREVIEW_BYTES`,
/// enforcing strict workspace path containment and rejecting symlinks.
async fn read_live_workspace_file_preview(
    state: &AppState,
    workspace_id: &str,
    file_path: &str,
    mission_id: &str,
) -> String {
    let ws_trimmed = workspace_id.trim();
    let safe_res = if !ws_trimmed.is_empty() {
        let ws_path = StdPath::new(ws_trimmed);
        if ws_path.is_dir() {
            crate::utils::security::validate_path(ws_path, file_path)
        } else {
            Err(AppError::Forbidden(
                "Mission workspace directory is unavailable".to_string(),
            ))
        }
    } else {
        crate::utils::security::validate_path(&state.base_dir, file_path)
    };

    let safe_p = match safe_res {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(
                mission_id = %mission_id,
                file_path = %file_path,
                error = %e,
                "Workspace path validation rejected file in dossier diff"
            );
            return "[File Access Denied]".to_string();
        }
    };

    let p = safe_p.as_path();
    let meta = match tokio::fs::symlink_metadata(p).await {
        Ok(m) => m,
        Err(_) => return "[File not present on active disk]".to_string(),
    };

    if meta.file_type().is_symlink() {
        tracing::warn!(mission_id = %mission_id, path = ?p, "Refusing to read symlink in dossier diff");
        return "[Symlink Skipped]".to_string();
    }
    if !meta.is_file() {
        return "[File not present on active disk]".to_string();
    }
    if meta.len() > (MAX_FILE_PREVIEW_BYTES as u64) {
        return "[File exceeds 256 KB preview ceiling]".to_string();
    }

    let file = match tokio::fs::File::open(p).await {
        Ok(f) => f,
        Err(e) => {
            tracing::warn!(mission_id = %mission_id, path = ?p, error = %e, "Failed opening live file for dossier diff");
            return "[File Unreadable]".to_string();
        }
    };

    use tokio::io::AsyncReadExt;
    let mut limited = file.take((MAX_FILE_PREVIEW_BYTES as u64) + 1);
    let mut buf = Vec::new();
    match limited.read_to_end(&mut buf).await {
        Ok(n) if n > MAX_FILE_PREVIEW_BYTES => "[File exceeds 256 KB preview ceiling]".to_string(),
        Ok(_) => String::from_utf8(buf)
            .unwrap_or_else(|_| "[Binary or Unreadable File Content]".to_string()),
        Err(e) => {
            tracing::warn!(mission_id = %mission_id, path = ?p, error = %e, "Failed reading live file for dossier diff");
            "[File Unreadable]".to_string()
        }
    }
}

async fn fetch_and_diff_cas_revisions(
    state: &AppState,
    mission_id: &str,
    mission_diff_count: i64,
    has_tamper: &mut bool,
    partial: &mut bool,
    warnings: &mut Vec<String>,
) -> CasDiffsResult {
    let pool = &state.resources.pool;
    let preview_ceiling = MAX_FILE_PREVIEW_BYTES as i64;

    // Uses ROW_NUMBER() window partitioning so duplicate version_num entries never emit duplicate file rows.
    let rev_rows = match sqlx::query(
        r#"WITH ranked_revs AS (
            SELECT 
                r.id, r.workspace_id, r.file_path, r.hash, r.size_bytes, r.version_num, 
                CAST(r.created_at AS TEXT) as created_at_str,
                CASE WHEN length(r.content) > ?1 THEN NULL ELSE r.content END as pre_blob,
                ROW_NUMBER() OVER (PARTITION BY r.file_path ORDER BY r.version_num ASC, r.id ASC) as rn
            FROM file_revisions r
            WHERE r.mission_id = ?2
        )
        SELECT id, workspace_id, file_path, hash, size_bytes, version_num, created_at_str, pre_blob
        FROM ranked_revs
        WHERE rn = 1
        ORDER BY file_path ASC
        LIMIT ?3"#,
    )
    .bind(preview_ceiling)
    .bind(mission_id)
    .bind(MAX_REVISION_ROWS)
    .fetch_all(pool)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            *partial = true;
            let msg = "Failed to fetch CAS file_revisions for mission".to_string();
            tracing::error!(error = %e, mission_id = %mission_id, "{}", msg);
            warnings.push(msg);
            Vec::new()
        }
    };

    let diffs_total = (mission_diff_count.max(0) as u64).max(rev_rows.len() as u64);
    let mut diffs_truncated = diffs_total > (MAX_REVISION_ROWS as u64);
    let mut cumulative_diff_bytes: usize = 0;
    let mut diffs = Vec::with_capacity(rev_rows.len());

    for r in rev_rows {
        let rev_id: i64 = r.try_get("id").unwrap_or(0);
        let workspace_id: String = r.try_get("workspace_id").unwrap_or_default();
        let file_path: String = r.try_get("file_path").unwrap_or_default();
        let hash: String = r.try_get("hash").unwrap_or_default();
        let size_bytes: i64 = r.try_get("size_bytes").unwrap_or(0);
        let version_num: i64 = r.try_get("version_num").unwrap_or(0);
        let created_at: String = r.try_get("created_at_str").unwrap_or_default();
        let pre_blob: Option<Vec<u8>> = r.try_get("pre_blob").ok().flatten();

        // Enforce aggregate 4 MB diff payload budget across the dossier
        if cumulative_diff_bytes >= MAX_DOSSIER_DIFF_BYTES {
            diffs_truncated = true;
            diffs.push(CasDiffRecord {
                id: rev_id,
                file_path,
                version_num,
                hash,
                size_bytes,
                created_at,
                pre_mutation_content: "[Dossier diff payload budget (4 MB) exceeded]".to_string(),
                current_content: "[Dossier diff payload budget (4 MB) exceeded]".to_string(),
                diff_summary: CasDiffSummary {
                    additions: 0,
                    deletions: 0,
                    changes: 0,
                },
            });
            continue;
        }

        let pre_mutation_content = if size_bytes > preview_ceiling || pre_blob.is_none() {
            "[CAS snapshot exceeds 256 KB preview ceiling]".to_string()
        } else if let Some(blob) = pre_blob {
            if !hash.is_empty() {
                let actual_hash = hex::encode(Sha256::digest(&blob));
                if actual_hash != hash {
                    *has_tamper = true;
                    let msg = format!(
                        "CAS snapshot SHA-256 mismatch for file '{}' (revision {})",
                        file_path, rev_id
                    );
                    tracing::warn!(mission_id = %mission_id, file_path = %file_path, "{}", msg);
                    warnings.push(msg);
                }
            }
            String::from_utf8(blob).unwrap_or_else(|_| "[Binary CAS Snapshot]".to_string())
        } else {
            String::new()
        };

        let current_content =
            read_live_workspace_file_preview(state, &workspace_id, &file_path, mission_id).await;

        // Compute line diff summary on raw content BEFORE secret redaction so rotated secrets are counted
        let diff_summary = compute_line_diff_summary(&pre_mutation_content, &current_content);
        let safe_pre = state.security.secret_redactor.redact(&pre_mutation_content);
        let safe_cur = state.security.secret_redactor.redact(&current_content);
        cumulative_diff_bytes = cumulative_diff_bytes
            .saturating_add(safe_pre.len())
            .saturating_add(safe_cur.len());

        diffs.push(CasDiffRecord {
            id: rev_id,
            file_path,
            version_num,
            hash,
            size_bytes,
            created_at,
            pre_mutation_content: safe_pre,
            current_content: safe_cur,
            diff_summary,
        });
    }

    CasDiffsResult {
        diffs,
        diffs_total,
        diffs_truncated,
    }
}

async fn fetch_memory_palace(
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

async fn fetch_trust_graph(
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

async fn build_economics_payload(
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
async fn synthesize_debrief_payload(
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

// ─────────────────────────────────────────────────────────
//  HANDLERS
// ─────────────────────────────────────────────────────────

/// GET /v1/missions
/// Retrieves a paginated catalog of all missions with aggregated log and diff counts.
#[tracing::instrument(skip(state), name = "missions::get_missions_catalog")]
pub async fn get_missions_catalog(
    State(state): State<Arc<AppState>>,
    Query(query): Query<CatalogQuery>,
) -> Result<(StatusCode, Json<CatalogResponse>), AppError> {
    let pool = &state.resources.pool;
    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(25).clamp(1, 100);
    let offset = (page.saturating_sub(1) as i64).saturating_mul(per_page as i64);

    let filters = MissionCatalogFilters::from_query(&query)?;

    // Run COUNT(*) and page SELECT in a single read transaction for snapshot consistency
    let mut tx = pool.begin().await?;

    let mut count_qb =
        QueryBuilder::<Sqlite>::new("SELECT COUNT(*) FROM mission_history WHERE 1=1");
    filters.apply(&mut count_qb);

    let total: i64 = count_qb
        .build_query_scalar()
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to execute mission catalog count query");
            AppError::from(e)
        })?;

    let mut sel_qb = QueryBuilder::<Sqlite>::new(
        r#"SELECT 
            id, 
            agent_id, 
            title, 
            status, 
            budget_usd, 
            cost_usd, 
            COALESCE(is_pinned, 0) as is_pinned,
            strftime('%Y-%m-%dT%H:%M:%SZ', created_at) as created_at_iso,
            strftime('%Y-%m-%dT%H:%M:%SZ', updated_at) as updated_at_iso,
            (SELECT COUNT(*) FROM mission_logs l WHERE l.mission_id = mission_history.id) as log_count,
            (SELECT COUNT(DISTINCT r.file_path) FROM file_revisions r WHERE r.mission_id = mission_history.id) as diff_count
        FROM mission_history
        WHERE 1=1"#,
    );
    filters.apply(&mut sel_qb);

    sel_qb.push(" ORDER BY is_pinned DESC, created_at DESC, id DESC LIMIT ");
    sel_qb.push_bind(per_page as i64);
    sel_qb.push(" OFFSET ");
    sel_qb.push_bind(offset);

    let rows = sel_qb.build().fetch_all(&mut *tx).await?;
    tx.commit().await?;

    let mut data = Vec::with_capacity(rows.len());
    for r in rows {
        data.push(decode_mission_summary(&r)?);
    }

    Ok((
        StatusCode::OK,
        Json(CatalogResponse {
            success: true,
            data,
            total,
            page,
            per_page,
        }),
    ))
}

/// GET /v1/missions/:id/dossier
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

/// POST /v1/missions/:id/pin
/// Sets or toggles the `is_pinned` retention flag for a mission atomically.
/// Accepts an optional `{ "is_pinned": true | false }` JSON payload for idempotent updates.
#[tracing::instrument(skip(state, operator, payload), name = "missions::toggle_mission_pin")]
pub async fn toggle_mission_pin(
    State(state): State<Arc<AppState>>,
    operator: Option<Extension<AuthenticatedOperator>>,
    Path(id): Path<String>,
    payload: Option<Json<PinMissionRequest>>,
) -> Result<impl IntoResponse, AppError> {
    let pool = &state.resources.pool;
    let mission_id = id.trim();
    if mission_id.is_empty() {
        return Err(AppError::BadRequest(
            "Mission ID cannot be empty".to_string(),
        ));
    }

    let desired_pin = payload.and_then(|Json(req)| req.is_pinned);

    // Atomic update without corrupting updated_at
    let updated: Option<(i64,)> = if let Some(pin_target) = desired_pin {
        sqlx::query_as(
            r#"UPDATE mission_history 
            SET is_pinned = ?1 
            WHERE id = ?2 
            RETURNING is_pinned"#,
        )
        .bind(if pin_target { 1_i64 } else { 0_i64 })
        .bind(mission_id)
        .fetch_optional(pool)
        .await?
    } else {
        sqlx::query_as(
            r#"UPDATE mission_history 
            SET is_pinned = CASE WHEN COALESCE(is_pinned, 0) = 0 THEN 1 ELSE 0 END 
            WHERE id = ?1 
            RETURNING is_pinned"#,
        )
        .bind(mission_id)
        .fetch_optional(pool)
        .await?
    };

    let (is_pinned_val,) = match updated {
        Some(v) => v,
        None => {
            return Err(AppError::NotFound(format!(
                "Mission not found: {}",
                mission_id
            )))
        }
    };

    let new_pinned = is_pinned_val == 1;
    // Gate 4 Trust Boundary: Never trust client-supplied X-Actor-Id / X-Principal headers.
    let actor = if operator.is_some() {
        "authenticated_operator"
    } else {
        "local_operator"
    };

    state.emit_event(serde_json::json!({
        "type": "mission_update",
        "mission_id": mission_id,
        "action": "pin_toggled",
        "is_pinned": new_pinned,
        "actor": actor,
        "timestamp": chrono::Utc::now().to_rfc3339()
    }));

    Ok((
        StatusCode::OK,
        Json(json!({
            "success": true,
            "mission_id": mission_id,
            "is_pinned": new_pinned,
            "actor": actor
        })),
    ))
}

// ─────────────────────────────────────────────────────────
//  DIFF COMPUTATION HELPER
// ─────────────────────────────────────────────────────────

/// Multiset frequency counter for accurate line addition/deletion counting.
fn compute_line_diff_summary(pre: &str, current: &str) -> CasDiffSummary {
    let mut pre_counts: HashMap<&str, usize> = HashMap::new();
    for line in pre.lines() {
        *pre_counts.entry(line).or_insert(0) += 1;
    }

    let mut cur_counts: HashMap<&str, usize> = HashMap::new();
    for line in current.lines() {
        *cur_counts.entry(line).or_insert(0) += 1;
    }

    let mut additions = 0;
    for (line, &cur_count) in &cur_counts {
        let pre_count = pre_counts.get(line).copied().unwrap_or(0);
        if cur_count > pre_count {
            additions += cur_count - pre_count;
        }
    }

    let mut deletions = 0;
    for (line, &pre_count) in &pre_counts {
        let cur_count = cur_counts.get(line).copied().unwrap_or(0);
        if pre_count > cur_count {
            deletions += pre_count - cur_count;
        }
    }

    let changes = additions.min(deletions);

    CasDiffSummary {
        additions,
        deletions,
        changes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

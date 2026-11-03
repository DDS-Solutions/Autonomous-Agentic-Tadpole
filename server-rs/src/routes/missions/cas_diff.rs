//! @docs ARCHITECTURE:Storage
//!
//! ### AI Assist Note
//! **Missions CAS Diff Engine**: Computes line diffs between content-addressed snapshots
//! and live workspace files within memory and size bounds, redacting secrets.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: CAS snapshot hash corruption, live file unreadable, or 4MB diff ceiling overflow.
//! - **Telemetry Link**: Search `[missions_cas_diff]` in tracing logs.
//! - **Trace Scope**: `server-rs::routes::missions::cas_diff`

use sha2::{Digest, Sha256};
use sqlx::Row;
use std::collections::HashMap;
use std::path::Path as StdPath;

use crate::error::AppError;
use crate::state::AppState;
use super::models::{
    CasDiffRecord, CasDiffSummary, MAX_DOSSIER_DIFF_BYTES, MAX_FILE_PREVIEW_BYTES,
    MAX_REVISION_ROWS,
};

pub struct CasDiffsResult {
    pub diffs: Vec<CasDiffRecord>,
    pub diffs_total: u64,
    pub diffs_truncated: bool,
}

/// Multiset frequency counter for accurate line addition/deletion counting.
pub fn compute_line_diff_summary(pre: &str, current: &str) -> CasDiffSummary {
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

/// Safely resolves and reads the live workspace file preview within `MAX_FILE_PREVIEW_BYTES`,
/// enforcing strict workspace path containment and rejecting symlinks.
pub async fn read_live_workspace_file_preview(
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

pub async fn fetch_and_diff_cas_revisions(
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

// Metadata: [missions_cas_diff]

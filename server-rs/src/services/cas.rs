//! @docs ARCHITECTURE:Core
//!
//! ### AI Assist Note
//! **cas**: Lean Content-Addressable Revision Service for the Sovereign Observability Core (SOC).
//! Captures pre-mutation file snapshots (capped at 1 MB) before agent workspace writes or deletions
//! to power read-only diff inspection in the Mission Flight Recorder without SQLite WAL bloat.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Path traversal outside workspace root, I/O read failure, or SQLite lock contention.
//! - **Telemetry Link**: Search `[cas]` in tracing logs.
//! - **Trace Scope**: `server-rs::services::cas`

use crate::error::AppError;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{Row, SqlitePool};
use std::path::Path;

/// Maximum size allowed for inline pre-mutation text snapshot storage (1 MB safety ceiling).
pub const MAX_CAS_FILE_SIZE: usize = 1_048_576;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevisionSummary {
    pub id: i64,
    pub workspace_id: String,
    pub file_path: String,
    pub hash: String,
    pub size_bytes: usize,
    pub version_num: i64,
    pub mission_id: Option<String>,
    pub agent_id: Option<String>,
    pub created_at: String,
}

/// Computes the SHA-256 hexadecimal hash string for arbitrary raw byte slices.
pub fn compute_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

/// Normalizes relative workspace file paths to use forward slashes for clean DB indexing.
pub fn normalize_rel_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Captures the pre-mutation state of a workspace file into `file_revisions` before an agent
/// modifies, creates, or deletes it.
///
/// - For existing files <= 1 MB: stores the exact pre-mutation bytes.
/// - For existing files > 1 MB: records metadata and a bounded ceiling placeholder to avoid DB bloat.
/// - For newly created files (not yet on disk): records an empty baseline (`v0`, `size_bytes = 0`)
///   so Overlord Entity (0) can inspect additions for newly created files in the Flight Recorder.
pub async fn capture_pre_mutation(
    pool: &SqlitePool,
    workspace_root: &Path,
    target_path: &Path,
    mission_id: Option<&str>,
    agent_id: Option<&str>,
) -> Result<Option<String>, AppError> {
    // Reject any explicit parent traversal components
    if target_path
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        tracing::warn!(
            target: "cas_service",
            path = %target_path.display(),
            "🚫 [cas] Denied pre-mutation capture: path contains parent traversal"
        );
        return Ok(None);
    }

    // Verify that target_path (or its parent for a new file) is strictly within workspace_root
    let canon_ws =
        std::fs::canonicalize(workspace_root).unwrap_or_else(|_| workspace_root.to_path_buf());
    let ws_str = canon_ws.to_string_lossy().replace(r"\\?\", "");

    let rel_path = if target_path.exists() {
        if !target_path.is_file() {
            return Ok(None);
        }
        let canon_target =
            std::fs::canonicalize(target_path).unwrap_or_else(|_| target_path.to_path_buf());
        let target_str = canon_target.to_string_lossy().replace(r"\\?\", "");
        if !canon_target.starts_with(&canon_ws) && !target_str.starts_with(&ws_str) {
            tracing::warn!(
                target: "cas_service",
                path = %target_path.display(),
                workspace = %workspace_root.display(),
                "🚫 [cas] Denied pre-mutation capture: existing target is outside workspace"
            );
            return Ok(None);
        }
        match target_path.strip_prefix(workspace_root) {
            Ok(p) => normalize_rel_path(p),
            Err(_) => match canon_target.strip_prefix(&canon_ws) {
                Ok(p) => normalize_rel_path(p),
                Err(_) => return Ok(None),
            },
        }
    } else {
        // File does not exist yet (newly created file by agent)
        if let Ok(p) = target_path.strip_prefix(workspace_root) {
            normalize_rel_path(p)
        } else if let Ok(p) = target_path.strip_prefix(&canon_ws) {
            normalize_rel_path(p)
        } else {
            tracing::warn!(
                target: "cas_service",
                path = %target_path.display(),
                workspace = %workspace_root.display(),
                "🚫 [cas] Denied pre-mutation capture: new file path is outside workspace"
            );
            return Ok(None);
        }
    };

    if rel_path.is_empty() {
        return Ok(None);
    }

    let (stored_bytes, file_size, hash) = if target_path.exists() {
        let raw_bytes = match tokio::fs::read(target_path).await {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!(
                    target: "cas_service",
                    path = %target_path.display(),
                    error = %e,
                    "⚠️ [cas] Failed to read pre-mutation file"
                );
                return Err(AppError::Io(e));
            }
        };
        let actual_size = raw_bytes.len();
        let digest = compute_sha256(&raw_bytes);
        if actual_size > MAX_CAS_FILE_SIZE {
            tracing::warn!(
                target: "cas_service",
                path = %target_path.display(),
                size_bytes = actual_size,
                "⚠️ [cas] File exceeds 1 MB preview ceiling; storing marker stub"
            );
            (
                b"[CAS snapshot exceeds 1 MB preview ceiling]".to_vec(),
                actual_size,
                digest,
            )
        } else {
            (raw_bytes, actual_size, digest)
        }
    } else {
        let empty: Vec<u8> = Vec::new();
        let digest = compute_sha256(&empty);
        (empty, 0, digest)
    };

    let workspace_id = normalize_rel_path(workspace_root);

    // Atomic transaction for check-and-insert
    let mut tx = pool.begin().await?;

    let latest_opt: Option<(i64, String, Option<String>)> = sqlx::query_as(
        "SELECT version_num, hash, mission_id FROM file_revisions WHERE workspace_id = ? AND file_path = ? ORDER BY version_num DESC LIMIT 1",
    )
    .bind(&workspace_id)
    .bind(&rel_path)
    .fetch_optional(&mut *tx)
    .await?;

    let (next_ver, is_duplicate) = match latest_opt {
        Some((latest_ver, latest_hash, latest_mission)) => {
            let same_mission = latest_mission.as_deref() == mission_id;
            (latest_ver + 1, latest_hash == hash && same_mission)
        }
        None => (0, false),
    };

    if is_duplicate {
        tx.rollback().await?;
        return Ok(Some(hash));
    }

    let created_at = chrono::Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO file_revisions (workspace_id, file_path, hash, content, size_bytes, version_num, mission_id, agent_id, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&workspace_id)
    .bind(&rel_path)
    .bind(&hash)
    .bind(&stored_bytes)
    .bind(file_size as i64)
    .bind(next_ver)
    .bind(mission_id)
    .bind(agent_id)
    .bind(&created_at)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    tracing::info!(
        target: "cas_service",
        version = next_ver,
        path = %rel_path,
        hash = &hash[..8.min(hash.len())],
        size_bytes = file_size,
        "📸 [cas] Captured pre-mutation file revision"
    );
    Ok(Some(hash))
}

/// Retrieves the read-only revision history summary list for a file in descending version order.
pub async fn get_file_history(
    pool: &SqlitePool,
    workspace_root: &Path,
    rel_path_str: &str,
) -> Result<Vec<RevisionSummary>, AppError> {
    let workspace_id = normalize_rel_path(workspace_root);
    let rel_path = rel_path_str.replace('\\', "/");

    let rows = sqlx::query(
        "SELECT id, workspace_id, file_path, hash, size_bytes, version_num, mission_id, agent_id, created_at 
         FROM file_revisions 
         WHERE workspace_id = ? AND file_path = ? 
         ORDER BY version_num DESC 
         LIMIT 500",
    )
    .bind(&workspace_id)
    .bind(&rel_path)
    .fetch_all(pool)
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(RevisionSummary {
            id: row.get("id"),
            workspace_id: row.get("workspace_id"),
            file_path: row.get("file_path"),
            hash: row.get("hash"),
            size_bytes: row.get::<i64, _>("size_bytes").max(0) as usize,
            version_num: row.get("version_num"),
            mission_id: row.get("mission_id"),
            agent_id: row.get("agent_id"),
            created_at: row.get("created_at"),
        });
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;
    use tempfile::tempdir;

    async fn setup_test_db() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS file_revisions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                workspace_id TEXT NOT NULL,
                file_path TEXT NOT NULL,
                hash TEXT NOT NULL,
                content BLOB NOT NULL,
                size_bytes INTEGER NOT NULL,
                version_num INTEGER NOT NULL,
                mission_id TEXT,
                agent_id TEXT,
                created_at TEXT NOT NULL
            );",
        )
        .execute(&pool)
        .await
        .unwrap();

        pool
    }

    #[tokio::test]
    async fn test_cas_capture_and_history_deduplication() {
        let pool = setup_test_db().await;
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("code.rs");

        // Write initial version
        tokio::fs::write(&file_path, b"fn main() { println!(\"v0\"); }")
            .await
            .unwrap();

        let hash_v0 =
            capture_pre_mutation(&pool, dir.path(), &file_path, Some("m1"), Some("agent-1"))
                .await
                .unwrap()
                .expect("Must capture initial version");

        // Duplicate capture in same mission with identical content should deduplicate
        let hash_dup =
            capture_pre_mutation(&pool, dir.path(), &file_path, Some("m1"), Some("agent-1"))
                .await
                .unwrap()
                .expect("Must return hash on duplicate");
        assert_eq!(hash_v0, hash_dup);

        // Modify file
        tokio::fs::write(&file_path, b"fn main() { println!(\"v1\"); }")
            .await
            .unwrap();

        let hash_v1 =
            capture_pre_mutation(&pool, dir.path(), &file_path, Some("m2"), Some("agent-2"))
                .await
                .unwrap()
                .expect("Must capture modified version");

        assert_ne!(hash_v0, hash_v1);

        // Check history (should have exactly 2 entries: v1 and v0)
        let history = get_file_history(&pool, dir.path(), "code.rs")
            .await
            .unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].version_num, 1);
        assert_eq!(history[1].version_num, 0);
    }

    #[tokio::test]
    async fn test_cas_new_file_baseline_capture() {
        let pool = setup_test_db().await;
        let dir = tempdir().unwrap();
        let new_file_path = dir.path().join("newly_created.rs");

        // Capture before file exists on disk
        let hash = capture_pre_mutation(
            &pool,
            dir.path(),
            &new_file_path,
            Some("m-new"),
            Some("agent-1"),
        )
        .await
        .unwrap()
        .expect("Must capture empty baseline for new file");

        assert_eq!(hash, compute_sha256(b""));

        let history = get_file_history(&pool, dir.path(), "newly_created.rs")
            .await
            .unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].size_bytes, 0);
        assert_eq!(history[0].version_num, 0);
    }

    #[tokio::test]
    async fn test_cas_capture_outside_workspace_rejected() {
        let pool = setup_test_db().await;
        let ws_dir = tempdir().unwrap();
        let outside_dir = tempdir().unwrap();
        let outside_file = outside_dir.path().join("secret.txt");

        tokio::fs::write(&outside_file, b"super secret")
            .await
            .unwrap();

        let res = capture_pre_mutation(
            &pool,
            ws_dir.path(),
            &outside_file,
            Some("m1"),
            Some("agent-1"),
        )
        .await
        .unwrap();

        assert!(res.is_none());
    }
}


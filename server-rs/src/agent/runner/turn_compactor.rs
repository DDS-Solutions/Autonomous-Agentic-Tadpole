//! @docs ARCHITECTURE:Core
//!
//! ### AI Assist Note
//! **Turn Compactor**: Implements context-engineered compaction and restorable
//! tool observation offloading. Prevents multi-turn prompt token bloat by
//! offloading observations exceeding 300 characters to disk (`.tmp/tool_overflow/`).
//! Preserves recent task context via a pinned-tail policy and enforces failure
//! visibility retention so the agent does not repeat recent mistakes.
//!
//! ### ⚠️ Invariants & Non-Negotiables
//! - `[Structural]` Safe character-boundary string slicing without panics.
//! - `[Behavioral]` Pinned tail (last 3 turns) and recent failure messages are never compressed.
//!
//! ### 🔍 Debugging & Observability
//! - **Telemetry Link**: Search `[compactor]` in tracing logs.
//! - **Trace Scope**: `server-rs::agent::runner::turn_compactor`

use std::path::{Path, PathBuf};

/// Safely truncates a string to at most `max_chars` UTF-8 characters without splitting codepoints.
pub fn safe_truncate_str(s: &str, max_chars: usize) -> &str {
    match s.char_indices().nth(max_chars) {
        None => s,
        Some((idx, _)) => &s[..idx],
    }
}

/// Compacts a tool execution observation. If the raw output exceeds 300 characters,
/// writes the complete output to `.tmp/tool_overflow/` and returns a compacted reference pointer.
pub async fn compact_and_offload_observation(
    tool_name: &str,
    raw_output: &str,
    workspace_root: &Path,
) -> (String, Option<PathBuf>) {
    if raw_output.chars().count() <= 300 {
        return (format!("\nTool {} Result: {}", tool_name, raw_output), None);
    }

    let overflow_dir = workspace_root.join(".tmp").join("tool_overflow");
    if let Err(e) = tokio::fs::create_dir_all(&overflow_dir).await {
        tracing::warn!("[compactor] Failed to create overflow directory: {}", e);
        let preview = safe_truncate_str(raw_output, 300);
        return (
            format!(
                "\nTool {} Result: {}...\n[TRUNCATED TOOL OUTPUT: {} chars total]",
                tool_name,
                preview,
                raw_output.len()
            ),
            None,
        );
    }

    let timestamp = chrono::Utc::now().timestamp_millis();
    let short_id = uuid::Uuid::new_v4().to_string().chars().take(8).collect::<String>();
    let filename = format!("tool_output_{}_{}.txt", timestamp, short_id);
    let target_path = overflow_dir.join(&filename);

    if let Err(e) = tokio::fs::write(&target_path, raw_output).await {
        tracing::warn!("[compactor] Failed to write overflow file: {}", e);
        let preview = safe_truncate_str(raw_output, 300);
        return (
            format!(
                "\nTool {} Result: {}...\n[TRUNCATED TOOL OUTPUT: {} chars total]",
                tool_name,
                preview,
                raw_output.len()
            ),
            None,
        );
    }

    let rel_path = format!(".tmp/tool_overflow/{}", filename);
    let preview = safe_truncate_str(raw_output, 300);
    let compacted = format!(
        "\nTool {} Result: {}...\n[TRUNCATED TOOL OUTPUT: {} chars total. Full output saved to: {}]\nGuidance: Use file read tools on {} with specific line ranges if exact details are needed.",
        tool_name,
        preview,
        raw_output.len(),
        rel_path,
        rel_path
    );

    (compacted, Some(target_path))
}

/// Applies pinned-tail, failure-retention, and role-based visibility filtering to conversation history.
pub fn compact_conversation_history(role: &str, raw_history: &[String]) -> Vec<String> {
    tracing::debug!("[compactor] Compacting transcript turns for role={}", role);
    let len = raw_history.len();
    let pinned_tail_count = 3;
    let mut clean = Vec::with_capacity(len);

    for (idx, msg) in raw_history.iter().enumerate() {
        // Event Isolation Scope: restrict visibility of internal/private events to supervisor nodes
        if (msg.contains("[SystemOnly]") || msg.contains("[Private]"))
            && !role.eq_ignore_ascii_case("CEO")
            && !role.eq_ignore_ascii_case("Alpha")
        {
            continue;
        }

        let clean_msg = msg
            .replace("<halting_signal/>", "")
            .replace("<halt/>", "")
            .replace("<thinking>", "")
            .replace("</thinking>", "")
            .trim()
            .to_string();

        let is_in_pinned_tail = len > pinned_tail_count && idx >= (len - pinned_tail_count);
        let is_failure = clean_msg.contains("[TOOL_ERROR]")
            || clean_msg.contains("FAILED:")
            || clean_msg.contains("Exception:")
            || clean_msg.contains("SyntaxError:")
            || clean_msg.contains("Error:")
            || clean_msg.contains("ERROR:")
            || clean_msg.contains("Command failed with exit code");

        // Never compress items in the pinned tail, or failures (failure visibility retention)
        if is_in_pinned_tail || is_failure {
            clean.push(clean_msg);
            continue;
        }

        if (clean_msg.starts_with("OBSERVATION:") || clean_msg.starts_with("TOOL OUTPUT:"))
            && clean_msg.chars().count() > 300
        {
            // If the observation already points to an offloaded file, preserve pointer intact
            if !clean_msg.contains("tool_overflow") && !clean_msg.contains("Full output saved to:") {
                let truncated = format!(
                    "{}... [TRUNCATED TOOL OUTPUT: {} chars total]",
                    safe_truncate_str(&clean_msg, 300),
                    clean_msg.len()
                );
                clean.push(truncated);
                continue;
            }
        }

        clean.push(clean_msg);
    }

    clean
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_compact_and_offload_short_output() {
        let dir = tempdir().unwrap();
        let (out, path) = compact_and_offload_observation("test_tool", "short output", dir.path()).await;
        assert!(path.is_none());
        assert!(out.contains("Tool test_tool Result: short output"));
    }

    #[tokio::test]
    async fn test_compact_and_offload_long_output() {
        let dir = tempdir().unwrap();
        let long_text = "A".repeat(1000);
        let (out, path) = compact_and_offload_observation("big_tool", &long_text, dir.path()).await;
        
        assert!(path.is_some());
        let written_path = path.unwrap();
        assert!(written_path.exists());
        let disk_content = tokio::fs::read_to_string(&written_path).await.unwrap();
        assert_eq!(disk_content, long_text);

        assert!(out.contains("[TRUNCATED TOOL OUTPUT: 1000 chars total"));
        assert!(out.contains(".tmp/tool_overflow/"));
    }

    #[test]
    fn test_pinned_tail_retention() {
        let mut history = Vec::new();
        for i in 0..6 {
            history.push(format!("OBSERVATION: Turn {} - {}", i, "X".repeat(400)));
        }

        let compacted = compact_conversation_history("Specialist", &history);
        assert_eq!(compacted.len(), 6);

        // First 3 turns should be truncated
        assert!(compacted[0].contains("[TRUNCATED TOOL OUTPUT"));
        assert!(compacted[1].contains("[TRUNCATED TOOL OUTPUT"));
        assert!(compacted[2].contains("[TRUNCATED TOOL OUTPUT"));

        // Last 3 turns (pinned tail) must NOT be truncated
        assert!(!compacted[3].contains("[TRUNCATED TOOL OUTPUT"));
        assert!(!compacted[4].contains("[TRUNCATED TOOL OUTPUT"));
        assert!(!compacted[5].contains("[TRUNCATED TOOL OUTPUT"));
    }

    #[test]
    fn test_failure_visibility_retention() {
        let history = vec![
            format!("OBSERVATION: Step 0 failed with Error: {}", "E".repeat(400)),
            "Turn 1".to_string(),
            "Turn 2".to_string(),
            "Turn 3".to_string(),
            "Turn 4".to_string(),
        ];

        let compacted = compact_conversation_history("Specialist", &history);
        // Turn 0 is an error, so despite being outside the pinned tail, it MUST NOT be truncated
        assert!(!compacted[0].contains("[TRUNCATED TOOL OUTPUT"));
        assert!(compacted[0].contains("failed with Error"));
    }

    #[test]
    fn test_role_based_privacy_filter() {
        let history = vec![
            "[Private] Alpha directive secret".to_string(),
            "Normal conversation message".to_string(),
        ];

        // Non-supervisor role strips [Private]
        let worker_view = compact_conversation_history("Worker", &history);
        assert_eq!(worker_view.len(), 1);
        assert_eq!(worker_view[0], "Normal conversation message");

        // Supervisor role (CEO or Alpha) preserves [Private]
        let supervisor_view = compact_conversation_history("CEO", &history);
        assert_eq!(supervisor_view.len(), 2);
    }

    #[tokio::test]
    async fn test_cleanup_overflow_directory() {
        let dir = tempdir().unwrap();
        let overflow = dir.path().join(".tmp").join("tool_overflow");
        tokio::fs::create_dir_all(&overflow).await.unwrap();
        let old_file = overflow.join("tool_output_old.txt");
        tokio::fs::write(&old_file, "old data").await.unwrap();

        // Immediate cleanup with 0 max_age should remove file
        let removed = cleanup_overflow_directory(dir.path(), std::time::Duration::from_secs(0)).await.unwrap();
        assert_eq!(removed, 1);
        assert!(!old_file.exists());
    }
}

/// Prunes overflow files older than max_age to prevent disk exhaustion.
pub async fn cleanup_overflow_directory(workspace_root: &Path, max_age: std::time::Duration) -> Result<usize, std::io::Error> {
    let overflow_dir = workspace_root.join(".tmp").join("tool_overflow");
    if !overflow_dir.exists() {
        return Ok(0);
    }
    let mut entries = tokio::fs::read_dir(&overflow_dir).await?;
    let mut removed = 0;
    let now = std::time::SystemTime::now();

    while let Some(entry) = entries.next_entry().await? {
        if let Ok(metadata) = entry.metadata().await {
            if let Ok(modified) = metadata.modified() {
                if let Ok(age) = now.duration_since(modified) {
                    if age >= max_age {
                        if let Ok(()) = tokio::fs::remove_file(entry.path()).await {
                            removed += 1;
                        }
                    }
                }
            }
        }
    }
    Ok(removed)
}

// Metadata: [turn_compactor]

//! @docs ARCHITECTURE:Agent:Blackboard
//!
//! ### AI Assist Note
//! **Swarm Shared Blackboard Engine**: Provides high-performance, thread-safe,
//! in-memory key-value scratchpad storage for multi-agent missions. Decouples
//! large data exchanges from prompt context histories, slashing input token consumption.
//! Features O(1) Arc pointer sharing, generic tag collection, and UTF-8 safe truncation.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Key collision, mission isolation breach, or stale read/write races.
//! - **Telemetry Link**: Search `[blackboard]` in tracing logs.

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::info;

/// An individual entry on the shared blackboard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlackboardEntry {
    pub key: String,
    pub value: serde_json::Value,
    pub author_agent_id: String,
    pub tags: Vec<String>,
    pub version: u64,
    pub updated_at: DateTime<Utc>,
}

/// Swarm-wide shared blackboard partitioned by `mission_id`.
#[derive(Debug, Clone, Default)]
pub struct SharedBlackboard {
    // mission_id -> (key -> Arc<BlackboardEntry>)
    missions: Arc<DashMap<String, Arc<DashMap<String, Arc<BlackboardEntry>>>>>,
}

impl SharedBlackboard {
    pub fn new() -> Self {
        Self {
            missions: Arc::new(DashMap::new()),
        }
    }

    /// Sets or updates a key-value entry on the mission blackboard with flexible tag collection.
    pub fn set<T, S>(
        &self,
        mission_id: &str,
        key: &str,
        value: serde_json::Value,
        author_agent_id: &str,
        tags: T,
    ) -> Arc<BlackboardEntry>
    where
        T: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mission_map = self
            .missions
            .entry(mission_id.to_string())
            .or_insert_with(|| Arc::new(DashMap::new()))
            .clone();

        let now = Utc::now();
        let new_version = mission_map
            .get(key)
            .map(|entry| entry.version + 1)
            .unwrap_or(1);

        let entry = Arc::new(BlackboardEntry {
            key: key.to_string(),
            value,
            author_agent_id: author_agent_id.to_string(),
            tags: tags.into_iter().map(Into::into).collect(),
            version: new_version,
            updated_at: now,
        });

        mission_map.insert(key.to_string(), entry.clone());
        info!(
            "📋 [Blackboard] Mission '{}' key '{}' updated to v{} by '{}'",
            mission_id, key, new_version, author_agent_id
        );
        entry
    }

    /// Retrieves an entry by key from a mission blackboard (O(1) Arc pointer clone).
    pub fn get(&self, mission_id: &str, key: &str) -> Option<Arc<BlackboardEntry>> {
        let mission_map = self.missions.get(mission_id)?;
        mission_map.get(key).map(|v| v.clone())
    }

    /// Lists all entries matching an optional tag filter.
    pub fn list(&self, mission_id: &str, tag_filter: Option<&str>) -> Vec<Arc<BlackboardEntry>> {
        let Some(mission_map) = self.missions.get(mission_id) else {
            return Vec::new();
        };

        mission_map
            .iter()
            .filter_map(|entry| {
                if let Some(tag) = tag_filter {
                    if entry.tags.iter().any(|t| t.eq_ignore_ascii_case(tag)) {
                        Some(entry.value().clone())
                    } else {
                        None
                    }
                } else {
                    Some(entry.value().clone())
                }
            })
            .collect()
    }

    /// Generates a compact Markdown summary of keys on the blackboard for prompt context injection.
    /// Safely handles arbitrary UTF-8 multi-byte characters and emojis without panicking.
    pub fn export_summary(&self, mission_id: &str) -> String {
        let Some(mission_map) = self.missions.get(mission_id) else {
            return String::new();
        };

        if mission_map.is_empty() {
            return String::new();
        }

        let mut lines = Vec::new();
        lines.push("### 📋 Shared Mission Blackboard:".to_string());
        for entry in mission_map.iter() {
            let val_summary = match &entry.value {
                serde_json::Value::String(s) => {
                    let char_count = s.chars().count();
                    if char_count > 60 {
                        let truncated: String = s.chars().take(57).collect();
                        format!("\"{}...\"", truncated)
                    } else {
                        format!("\"{}\"", s)
                    }
                }
                serde_json::Value::Array(arr) => format!("[{} items]", arr.len()),
                serde_json::Value::Object(obj) => format!("{{{}}}", obj.keys().cloned().collect::<Vec<_>>().join(", ")),
                other => other.to_string(),
            };
            lines.push(format!(
                "- **`{}`** (v{}, by `{}`): {}",
                entry.key, entry.version, entry.author_agent_id, val_summary
            ));
        }

        lines.join("\n")
    }

    /// Clears blackboard entries for a terminated mission to prevent memory accumulation.
    pub fn clear_mission(&self, mission_id: &str) {
        self.missions.remove(mission_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blackboard_concurrency_and_versioning() {
        let bb = SharedBlackboard::new();
        let m_id = "mission_swarm_001";

        let e1 = bb.set(
            m_id,
            "target_files",
            serde_json::json!(["src/main.rs", "src/lib.rs"]),
            "agent_alpha",
            vec!["ast"],
        );
        assert_eq!(e1.version, 1);

        let e2 = bb.set(
            m_id,
            "target_files",
            serde_json::json!(["src/main.rs", "src/lib.rs", "src/db.rs"]),
            "agent_beta",
            vec!["ast", "expanded"],
        );
        assert_eq!(e2.version, 2);

        let fetched = bb.get(m_id, "target_files").unwrap();
        assert_eq!(fetched.version, 2);
        assert_eq!(fetched.author_agent_id, "agent_beta");

        let ast_items = bb.list(m_id, Some("ast"));
        assert_eq!(ast_items.len(), 1);

        let summary = bb.export_summary(m_id);
        assert!(summary.contains("target_files"));
        assert!(summary.contains("agent_beta"));
    }

    #[test]
    fn test_blackboard_utf8_multibyte_truncation_safety() {
        let bb = SharedBlackboard::new();
        let m_id = "mission_utf8_test";

        // Multi-byte characters: emojis (4 bytes each) and CJK characters (3 bytes each)
        let complex_unicode = "🦀⚡🚀🎯🔥 Sovereign Reality Tadpole OS: 日本語と絵文字のテスト文字列が正しく処理されることを確認します。";
        bb.set(
            m_id,
            "unicode_key",
            serde_json::json!(complex_unicode),
            "agent_alpha",
            ["unicode"],
        );

        // export_summary must not panic on multi-byte boundaries
        let summary = bb.export_summary(m_id);
        assert!(summary.contains("unicode_key"));
        assert!(summary.contains("..."));
    }
}

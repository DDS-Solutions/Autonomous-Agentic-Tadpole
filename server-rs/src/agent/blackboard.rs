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

pub type MissionBoard = Arc<DashMap<String, Arc<BlackboardEntry>>>;

/// Swarm-wide shared blackboard partitioned by `mission_id`.
#[derive(Debug, Clone, Default)]
pub struct SharedBlackboard {
    // mission_id -> (key -> Arc<BlackboardEntry>)
    missions: Arc<DashMap<String, MissionBoard>>,
}

impl SharedBlackboard {
    pub fn new() -> Self {
        Self {
            missions: Arc::new(DashMap::new()),
        }
    }

    /// Parses a composite key into a base key and an optional facet.
    /// Format: `base_key#facet` (e.g., `code_review#ast` -> (`code_review`, Some(`ast`))).
    pub fn parse_facet_key(key: &str) -> (&str, Option<&str>) {
        if let Some(pos) = key.find('#') {
            let base = &key[..pos];
            let facet = &key[pos + 1..];
            if !facet.is_empty() {
                return (base, Some(facet));
            }
        }
        (key, None)
    }

    /// Sets or updates a key-value entry on the mission blackboard with flexible tag collection.
    /// If `key` includes a facet delimiter (e.g. `base#facet`), it updates only that specific
    /// facet within the root JSON object, preserving peer facets and bumping the version.
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

        let tags_vec: Vec<String> = tags.into_iter().map(Into::into).collect();
        let now = Utc::now();
        let (base_key, facet_opt) = Self::parse_facet_key(key);

        let mut map_entry = mission_map.entry(base_key.to_string());
        let entry = match map_entry {
            dashmap::mapref::entry::Entry::Occupied(ref mut occ) => {
                let next_version = occ.get().version + 1;
                let final_value = match facet_opt {
                    Some(facet) => {
                        let mut map = match &occ.get().value {
                            serde_json::Value::Object(existing) => existing.clone(),
                            _ => serde_json::Map::new(),
                        };
                        map.insert(facet.to_string(), value);
                        serde_json::Value::Object(map)
                    }
                    None => value,
                };

                let new_entry = Arc::new(BlackboardEntry {
                    key: base_key.to_string(),
                    value: final_value,
                    author_agent_id: author_agent_id.to_string(),
                    tags: tags_vec,
                    version: next_version,
                    updated_at: now,
                });
                occ.insert(new_entry.clone());
                new_entry
            }
            dashmap::mapref::entry::Entry::Vacant(vac) => {
                let final_value = match facet_opt {
                    Some(facet) => {
                        let mut map = serde_json::Map::new();
                        map.insert(facet.to_string(), value);
                        serde_json::Value::Object(map)
                    }
                    None => value,
                };

                let new_entry = Arc::new(BlackboardEntry {
                    key: base_key.to_string(),
                    value: final_value,
                    author_agent_id: author_agent_id.to_string(),
                    tags: tags_vec,
                    version: 1,
                    updated_at: now,
                });
                vac.insert(new_entry.clone());
                new_entry
            }
        };

        info!(
            "📋 [Blackboard] Mission '{}' key '{}' updated to v{} by '{}'",
            mission_id, key, entry.version, author_agent_id
        );
        entry
    }

    /// Retrieves an entry by key from a mission blackboard (O(1) Arc pointer clone).
    /// If `key` addresses a facet (e.g. `base#facet`), projects that specific sub-field
    /// from the root JSON object into a virtual BlackboardEntry.
    pub fn get(&self, mission_id: &str, key: &str) -> Option<Arc<BlackboardEntry>> {
        let mission_map = self.missions.get(mission_id)?;
        let (base_key, facet_opt) = Self::parse_facet_key(key);
        let entry = mission_map.get(base_key)?.clone();

        match facet_opt {
            Some(facet) => match &entry.value {
                serde_json::Value::Object(map) => {
                    let facet_value = map.get(facet)?.clone();
                    Some(Arc::new(BlackboardEntry {
                        key: key.to_string(),
                        value: facet_value,
                        author_agent_id: entry.author_agent_id.clone(),
                        tags: entry.tags.clone(),
                        version: entry.version,
                        updated_at: entry.updated_at,
                    }))
                }
                _ => None,
            },
            None => Some(entry),
        }
    }

    /// Sets a specific facet within a base key on the blackboard.
    pub fn set_facet<T, S>(
        &self,
        mission_id: &str,
        base_key: &str,
        facet: &str,
        value: serde_json::Value,
        author_agent_id: &str,
        tags: T,
    ) -> Arc<BlackboardEntry>
    where
        T: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let composite_key = format!("{}#{}", base_key, facet);
        self.set(mission_id, &composite_key, value, author_agent_id, tags)
    }

    /// Retrieves a specific facet's JSON value directly.
    pub fn get_facet(&self, mission_id: &str, base_key: &str, facet: &str) -> Option<serde_json::Value> {
        let composite_key = format!("{}#{}", base_key, facet);
        self.get(mission_id, &composite_key).map(|e| e.value.clone())
    }

    /// Computes a deterministic SHA-256 fingerprint for a specific facet.
    /// Returns `None` if the mission, base key, or facet is not found.
    pub fn facet_fingerprint(&self, mission_id: &str, base_key: &str, facet: &str) -> Option<String> {
        let val = self.get_facet(mission_id, base_key, facet)?;
        Some(crate::agent::durable::hash_payload(&val))
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

    /// Creates an RAII scoped mission handle that cleans up the mission blackboard when dropped.
    pub fn scoped_mission(self: &Arc<Self>, mission_id: impl Into<String>) -> MissionBlackboardScope {
        MissionBlackboardScope::new(Arc::clone(self), mission_id)
    }
}

/// RAII Drop guard that deterministically frees mission blackboard entries upon scope termination.
pub struct MissionBlackboardScope {
    blackboard: Arc<SharedBlackboard>,
    mission_id: String,
}

impl MissionBlackboardScope {
    pub fn new(blackboard: Arc<SharedBlackboard>, mission_id: impl Into<String>) -> Self {
        Self {
            blackboard,
            mission_id: mission_id.into(),
        }
    }

    pub fn mission_id(&self) -> &str {
        &self.mission_id
    }
}

impl Drop for MissionBlackboardScope {
    fn drop(&mut self) {
        self.blackboard.clear_mission(&self.mission_id);
        tracing::debug!("🧹 [Blackboard] Deterministically pruned mission blackboard for '{}'", self.mission_id);
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

    #[test]
    fn test_blackboard_clear_and_isolation() {
        let bb = SharedBlackboard::new();
        bb.set("m_1", "k1", serde_json::json!("v1"), "agent_1", Vec::<String>::new());
        bb.set("m_2", "k2", serde_json::json!("v2"), "agent_2", Vec::<String>::new());

        assert!(bb.get("m_1", "k1").is_some());
        assert!(bb.get("m_2", "k2").is_some());

        // Clear mission 1 only
        bb.clear_mission("m_1");

        assert!(bb.get("m_1", "k1").is_none());
        assert!(bb.get("m_2", "k2").is_some(), "Mission 2 entries must remain unaffected");
    }

    #[tokio::test]
    async fn test_blackboard_concurrent_versioning() {
        let bb = Arc::new(SharedBlackboard::new());
        let m_id = "concurrent_mission";
        let key = "shared_counter";

        let mut handles = Vec::new();
        for i in 0..20 {
            let bb_clone = bb.clone();
            let handle = tokio::spawn(async move {
                bb_clone.set(
                    m_id,
                    key,
                    serde_json::json!({ "worker": i }),
                    &format!("agent_{}", i),
                    Vec::<String>::new(),
                );
            });
            handles.push(handle);
        }

        for h in handles {
            h.await.unwrap();
        }

        let entry = bb.get(m_id, key).expect("Entry must exist");
        assert_eq!(entry.version, 20, "Version must be exactly 20 after 20 atomic writes");
    }

    #[test]
    fn test_blackboard_sub_key_facet_addressing() {
        let bb = SharedBlackboard::new();
        let m_id = "mission_facet_test";

        // 1. Set individual facets under base key "analysis"
        bb.set(
            m_id,
            "analysis#ast",
            serde_json::json!({ "node_count": 42 }),
            "agent_parser",
            ["ast"],
        );

        bb.set(
            m_id,
            "analysis#summary",
            serde_json::json!("Clean code structure with 0 lint errors"),
            "agent_summarizer",
            ["summary"],
        );

        // 2. Querying base key returns the combined object containing both facets
        let base_entry = bb.get(m_id, "analysis").expect("Base entry must exist");
        assert_eq!(base_entry.version, 2, "Version must increment with each facet update");
        let obj = base_entry.value.as_object().expect("Value must be an object");
        assert!(obj.contains_key("ast"), "Must contain ast facet");
        assert!(obj.contains_key("summary"), "Must contain summary facet");

        // 3. Querying specific facets directly via get() with composite key
        let ast_facet = bb.get(m_id, "analysis#ast").expect("Facet entry must exist");
        assert_eq!(ast_facet.key, "analysis#ast");
        assert_eq!(ast_facet.value["node_count"], 42);

        // 4. Querying via helper get_facet()
        let summary_val = bb.get_facet(m_id, "analysis", "summary").expect("Summary facet must exist");
        assert_eq!(summary_val, "Clean code structure with 0 lint errors");

        // 5. Updating an existing facet preserves other facets
        bb.set_facet(
            m_id,
            "analysis",
            "ast",
            serde_json::json!({ "node_count": 108 }),
            "agent_parser_v2",
            ["ast", "updated"],
        );

        let updated_base = bb.get(m_id, "analysis").unwrap();
        assert_eq!(updated_base.version, 3);
        assert_eq!(updated_base.value["ast"]["node_count"], 108);
        assert_eq!(updated_base.value["summary"], "Clean code structure with 0 lint errors");
    }

    #[test]
    fn test_blackboard_facet_fingerprinting() {
        let bb = SharedBlackboard::new();
        let m_id = "mission_fingerprint_test";

        bb.set(
            m_id,
            "metrics#perf",
            serde_json::json!({
                "latency_ms": 15,
                "timestamp": "2026-10-22T10:00:00Z"
            }),
            "agent_bench",
            Vec::<String>::new(),
        );

        let fp1 = bb.facet_fingerprint(m_id, "metrics", "perf").expect("Fingerprint must be computable");

        // Update with changed immaterial field only (timestamp)
        bb.set(
            m_id,
            "metrics#perf",
            serde_json::json!({
                "latency_ms": 15,
                "timestamp": "2026-10-22T19:00:00Z"
            }),
            "agent_bench",
            Vec::<String>::new(),
        );

        let fp2 = bb.facet_fingerprint(m_id, "metrics", "perf").expect("Fingerprint must be computable");
        assert_eq!(fp1, fp2, "Fingerprints must remain identical when only immaterial fields change");

        // Update with material field change
        bb.set(
            m_id,
            "metrics#perf",
            serde_json::json!({
                "latency_ms": 99,
                "timestamp": "2026-10-22T19:00:00Z"
            }),
            "agent_bench",
            Vec::<String>::new(),
        );

        let fp3 = bb.facet_fingerprint(m_id, "metrics", "perf").expect("Fingerprint must be computable");
        assert_ne!(fp1, fp3, "Fingerprints must differ when material facet values change");
    }
}

// Metadata: [blackboard]

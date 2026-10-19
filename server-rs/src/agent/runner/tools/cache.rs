//! @docs ARCHITECTURE:Registry
//!
//! ### AI Context Alignment
//! - **Subsystem**: Sovereign Engine / Agent Runner / cache
//!
//! ### ⚠️ Invariants & Non-Negotiables
//! - `[Behavioral]` Tool outputs cached by (tool, args, root) are invalidated on file mtime modification or explicit path invalidation.
//! - `[Security]` Non-idempotent tools (commands, tests, mutations) are strictly rejected by `is_cacheable`.
//!
//! ### 🔍 Debugging & Observability
//! - **Local Errors**: none
//! - **Telemetry Targets**: none declared
//! - **Witness Tests**: `test_shared_tool_cache_basic`, `test_shared_tool_cache_mtime_invalidation`, `test_shared_tool_cache_invalidate_path`

use crate::agent::types::TokenUsage;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Clone, Debug)]
pub struct CacheValue {
    pub output: String,
    pub usage: Option<TokenUsage>,
    pub file_path: Option<PathBuf>,
    pub mtime: Option<SystemTime>,
    pub size: u64,
}

#[derive(Default)]
pub struct SharedToolCache {
    cache: HashMap<(String, String, String), CacheValue>,
}

impl SharedToolCache {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
        }
    }

    /// Determines if a tool name is strictly idempotent and safe for replay caching.
    /// Commands, test suites, mission completion, and filesystem mutations are strictly excluded.
    pub fn is_cacheable(tool_name: &str) -> bool {
        matches!(
            tool_name,
            "read_file"
                | "list_files"
                | "list_file_symbols"
                | "search_mission_knowledge"
                | "fetch_status"
        )
    }

    pub fn get(
        &mut self,
        tool_name: &str,
        args_str: &str,
        workspace_root: &str,
    ) -> Option<(String, Option<TokenUsage>)> {
        if !Self::is_cacheable(tool_name) {
            return None;
        }

        let key = (
            tool_name.to_string(),
            args_str.to_string(),
            workspace_root.to_string(),
        );

        if let Some(val) = self.cache.get(&key) {
            if let Some(ref path) = val.file_path {
                if let Ok(metadata) = std::fs::metadata(path) {
                    let current_mtime = metadata.modified().ok();
                    let current_size = metadata.len();

                    if val.mtime != current_mtime || val.size != current_size {
                        self.cache.remove(&key);
                        return None;
                    }
                } else {
                    self.cache.remove(&key);
                    return None;
                }
            }
            return Some((val.output.clone(), val.usage.clone()));
        }
        None
    }

    pub fn insert(
        &mut self,
        tool_name: &str,
        args_str: &str,
        workspace_root: &str,
        output: String,
        usage: Option<TokenUsage>,
        file_path: Option<PathBuf>,
    ) {
        if !Self::is_cacheable(tool_name) {
            return;
        }

        let key = (
            tool_name.to_string(),
            args_str.to_string(),
            workspace_root.to_string(),
        );
        let mut mtime = None;
        let mut size = 0;

        if let Some(ref path) = file_path {
            if let Ok(metadata) = std::fs::metadata(path) {
                mtime = metadata.modified().ok();
                size = metadata.len();
            }
        }

        self.cache.insert(
            key,
            CacheValue {
                output,
                usage,
                file_path,
                mtime,
                size,
            },
        );
    }

    pub fn invalidate_path(&mut self, path: &Path) {
        let canon_target = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let keys_to_remove: Vec<_> = self
            .cache
            .iter()
            .filter(|(_, val)| {
                val.file_path.as_ref().is_some_and(|p| {
                    if p == path {
                        return true;
                    }
                    let canon_p = std::fs::canonicalize(p).unwrap_or_else(|_| p.clone());
                    canon_p == canon_target
                })
            })
            .map(|(k, _)| k.clone())
            .collect();

        for k in keys_to_remove {
            self.cache.remove(&k);
        }
    }

    pub fn clear(&mut self) {
        self.cache.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_shared_tool_cache_basic() {
        let mut cache = SharedToolCache::new();
        let tool = "read_file";
        let args = r#"{"path": "test.txt"}"#;
        let root = "ws";

        assert!(cache.get(tool, args, root).is_none());

        cache.insert(tool, args, root, "file content".to_string(), None, None);

        let hit = cache.get(tool, args, root);
        assert!(hit.is_some());
        assert_eq!(hit.unwrap().0, "file content");
    }

    #[test]
    fn test_non_idempotent_tools_not_cached() {
        let mut cache = SharedToolCache::new();
        let root = "ws";

        for tool in &["run_command", "run_tests", "complete_mission", "write_to_file", "patch_file"] {
            assert!(!SharedToolCache::is_cacheable(tool));
            cache.insert(tool, "{}", root, "output".to_string(), None, None);
            assert!(cache.get(tool, "{}", root).is_none());
        }
    }

    #[test]
    fn test_shared_tool_cache_mtime_invalidation() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");

        let mut file = File::create(&file_path).unwrap();
        write!(file, "hello").unwrap();

        let mut cache = SharedToolCache::new();
        let tool = "read_file";
        let args = r#"{"path": "test.txt"}"#;
        let root = dir.path().to_string_lossy().to_string();

        cache.insert(
            tool,
            args,
            &root,
            "hello".to_string(),
            None,
            Some(file_path.clone()),
        );

        let hit = cache.get(tool, args, &root);
        assert!(hit.is_some());

        // Modify file
        std::thread::sleep(std::time::Duration::from_millis(20));
        let mut file = File::create(&file_path).unwrap();
        write!(file, "hello modified").unwrap();

        let hit_after_mod = cache.get(tool, args, &root);
        assert!(hit_after_mod.is_none());
    }

    #[test]
    fn test_shared_tool_cache_invalidate_path() {
        let mut cache = SharedToolCache::new();
        let tool = "read_file";
        let args = r#"{"path": "test.txt"}"#;
        let root = "ws";
        let path = PathBuf::from("ws/test.txt");

        cache.insert(
            tool,
            args,
            root,
            "hello".to_string(),
            None,
            Some(path.clone()),
        );

        cache.invalidate_path(&path);

        assert!(cache.get(tool, args, root).is_none());
    }
}

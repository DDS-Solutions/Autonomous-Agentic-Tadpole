//! @docs ARCHITECTURE:Core
//!
//! ### AI Assist Note
//! **cache**: Core technical resource for the Tadpole OS infrastructure.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Unhandled errors, lock contention, or connection staling.
//! - **Telemetry Link**: Search `[cache]` in tracing logs.
//! - **Trace Scope**: `server-rs::intelligence::graph::cache`

//! Incremental cache management for the `CodeSymbolGraph`.
//!
//! Compares the current filesystem state against stored metadata to
//! determine which files need re-parsing and which have been deleted.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::intelligence::graph::error::GraphError;

/// Service trait to detect changed and deleted files relative to the cache.
pub trait CacheManager: Send + Sync {
    /// Returns `(files_to_parse, deleted_paths)`.
    ///
    /// - `files_to_parse`: any file whose `mtime` or `size` differs from the cached value,
    ///   plus any file not yet in `metadata` (new files).
    /// - `deleted_paths`: files present in `metadata` but absent from `files`.
    fn check_changes(
        &self,
        files: &[PathBuf],
        metadata: &HashMap<PathBuf, (std::time::SystemTime, u64)>,
        root: &Path,
    ) -> (Vec<PathBuf>, Vec<PathBuf>);
}

/// Default implementation of [`CacheManager`].
pub struct CacheManagementService;

impl CacheManager for CacheManagementService {
    fn check_changes(
        &self,
        files: &[PathBuf],
        metadata: &HashMap<PathBuf, (std::time::SystemTime, u64)>,
        _root: &Path,
    ) -> (Vec<PathBuf>, Vec<PathBuf>) {
        let active_paths: std::collections::HashSet<&PathBuf> = files.iter().collect();
        let deleted_paths: Vec<PathBuf> = metadata
            .keys()
            .filter(|p| !active_paths.contains(p))
            .cloned()
            .collect();

        let mut files_to_parse = Vec::new();
        for path in files {
            let mut needs_parse = true;
            if let Ok(m) = std::fs::metadata(path) {
                if let (Ok(mtime), size) = (m.modified(), m.len()) {
                    if let Some(&(cached_mtime, cached_size)) = metadata.get(path) {
                        if cached_mtime == mtime && cached_size == size {
                            needs_parse = false;
                        }
                    }
                }
            }
            if needs_parse {
                files_to_parse.push(path.clone());
            }
        }

        (files_to_parse, deleted_paths)
    }
}

// Unused import guard – GraphError is referenced by the trait bounds in engine.rs
#[allow(unused_imports)]
use GraphError as _;

// Metadata: [cache]

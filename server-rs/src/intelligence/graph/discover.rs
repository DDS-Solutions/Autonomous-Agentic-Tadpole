//! File discovery service for the `CodeSymbolGraph`.
//!
//! Walks the workspace root, applies exclusion rules, enforces a 2 MB
//! per-file DoS guard, and validates symlink boundary containment.

use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use crate::intelligence::graph::{
    error::GraphError,
    path::sanitize_log_path,
};

/// Service trait to discover parseable source files within a workspace root.
pub trait FileDiscoverer: Send + Sync {
    fn discover(&self, root: &Path) -> Result<Vec<PathBuf>, GraphError>;
}

/// Default implementation: walks the directory tree excluding common
/// non-source directories, skips oversized files (> 2 MB), and
/// rejects paths that resolve outside the workspace root (symlink guard).
pub struct FileDiscoveryService {
    pub exclusions: Vec<String>,
}

impl Default for FileDiscoveryService {
    fn default() -> Self {
        Self {
            exclusions: vec![
                "target".to_string(),
                "node_modules".to_string(),
                ".git".to_string(),
                "dist".to_string(),
                "scratch".to_string(),
                "3rdparty".to_string(),
                ".tmp".to_string(),
                "tmp".to_string(),
                "workspaces".to_string(),
                ".agent".to_string(),
            ],
        }
    }
}

impl FileDiscoverer for FileDiscoveryService {
    fn discover(&self, root: &Path) -> Result<Vec<PathBuf>, GraphError> {
        let canonical_root = root.canonicalize().map_err(|e| {
            GraphError::WorkspaceRootNotFound(format!(
                "Failed to canonicalize root {}: {}",
                root.display(),
                e
            ))
        })?;

        let exclusions = &self.exclusions;
        let mut files = Vec::new();

        for entry in WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| {
                let name = e.file_name().to_string_lossy();
                !exclusions.iter().any(|ex| name == *ex)
            })
        {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    tracing::warn!("⚠️ [graph] Directory traversal warning: {}", e);
                    continue;
                }
            };

            if !entry.path().is_file() {
                continue;
            }

            let path = entry.path();
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if ext != "rs" && ext != "ts" && ext != "tsx" {
                continue;
            }

            // 🛡️ [DoS Protection] Enforce 2MB size limit
            let metadata = match std::fs::metadata(path) {
                Ok(m) => m,
                Err(e) => {
                    tracing::warn!(
                        "⚠️ [graph] Failed to read metadata for {}: {}",
                        sanitize_log_path(path, root),
                        e
                    );
                    continue;
                }
            };

            if metadata.len() > 2 * 1024 * 1024 {
                tracing::warn!(
                    "⚠️ [graph] Skipping oversized file ({} bytes): {}",
                    metadata.len(),
                    sanitize_log_path(path, root)
                );
                continue;
            }

            // 🛡️ Path Boundary Verification (Symlink Protection)
            let canonical_path = match path.canonicalize() {
                Ok(p) => p,
                Err(e) => {
                    tracing::warn!(
                        "⚠️ [graph] Failed to canonicalize path {}: {}",
                        sanitize_log_path(path, root),
                        e
                    );
                    continue;
                }
            };

            if !canonical_path.starts_with(&canonical_root) {
                tracing::warn!(
                    "⚠️ [graph] Security Violation: Path {} points outside workspace root {}",
                    sanitize_log_path(&canonical_path, root),
                    sanitize_log_path(&canonical_root, root)
                );
                continue;
            }

            files.push(path.to_path_buf());
        }

        Ok(files)
    }
}

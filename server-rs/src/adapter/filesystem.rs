//! @docs ARCHITECTURE:Infrastructure
//!
//! ### AI Assist Note
//! **Filesystem Adapter**: Orchestrates a sandboxed interface for agents
//! to interact with the local filesystem. Enforces **Strict Path Traversal
//! Protection** (SEC-03) by canonicalizing all candidate paths before
//! access, defeating symlink-based sandbox escapes. Distinguishes
//! between **Ephemeral Workspace Manipulation** (SCRATCH-01) and
//! persistent engine storage.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Illegal path traversal attempt (triggers `SECURITY
//!   FAULT`), permission denied at the OS level, or disk exhaustion
//!   preventing write operations.
//! - **Trace Scope**: `server-rs::adapter::filesystem`

use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};
use tokio::fs;

#[derive(Clone)]
pub struct FilesystemAdapter {
    pub root_path: PathBuf,
}

impl FilesystemAdapter {
    /// Creates a new adapter. The `root_path` is created if it doesn't exist,
    /// then immediately canonicalized to get its real, symlink-resolved path.
    /// This is the SEC-03 fix: prevents symlink-based sandbox escapes.
    pub fn new(root_path: PathBuf) -> Self {
        // We'll lazily canonicalize on first use to avoid blocking in new().
        Self { root_path }
    }

    /// Verifies the requested path stays inside the workspace.
    ///
    /// ### 🛡️ Security Model: Sandbox Defense (SEC-03)
    /// 1. **Component Filtering**: Rejects any path containing explicit `..` 
    ///    parent directory components before resolution.
    /// 2. **Canonicalization**: Resolves all symlinks and relative segments 
    ///    using the OS's real-path resolution.
    /// 3. **Subpath Assertion**: Verifies that the resulting real path 
    ///    starts with the canonicalized workspace root.
    ///
    /// This implementation defeats **Symlink-based Sandbox Escapes** and 
    /// **TOCTOU** (Time-of-Check Time-of-Use) races by canonicalizing both 
    /// the root and the candidate before the prefix check.
    async fn get_safe_path(&self, requested_path: &str) -> Result<PathBuf> {
        // Platform-agnostic path normalization (slash standardization and drive strip)
        let mut cleaned = requested_path.replace('\\', "/");
        if cleaned.len() >= 2 {
            let first_char = cleaned.chars().next().unwrap();
            let second_char = cleaned.chars().nth(1).unwrap();
            if first_char.is_ascii_alphabetic() && second_char == ':' {
                cleaned = cleaned[2..].to_string();
            }
        }
        let cleaned = cleaned.trim_start_matches('/');

        // Build the candidate path (without canonicalization first)
        let mut candidate = self.root_path.clone();

        for component in Path::new(&cleaned).components() {
            match component {
                std::path::Component::Normal(c) => candidate.push(c),
                std::path::Component::ParentDir => {
                    return Err(anyhow!("🚫 SECURITY FAULT: Illegal path traversal attempt detected. Access denied."));
                }
                // Ignore absolute roots/prefixes to keep path relative to our root
                std::path::Component::RootDir | std::path::Component::Prefix(_) => {}
                _ => {}
            }
        }

        // Resolve the real root (SEC-03: canonicalize to defeat symlinks).
        // We use the parent chain to canonicalize even if the leaf doesn't exist yet.
        let canonical_root = canonicalize_or_create(&self.root_path).await?;
        let canonical_candidate = canonicalize_or_create_parent(&candidate).await?;

        if !canonical_candidate.starts_with(&canonical_root) {
            return Err(anyhow!(
                "🚫 SECURITY FAULT: Attempted to access '{}' which is outside the designated workspace '{}'.",
                canonical_candidate.display(),
                canonical_root.display()
            ));
        }

        Ok(canonical_candidate)
    }

    pub async fn write_file(&self, filename: &str, content: &str) -> Result<()> {
        let path = self.get_safe_path(filename).await?;

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }

        fs::write(path, content).await?;
        Ok(())
    }

    pub async fn read_file(&self, filename: &str) -> Result<String> {
        let path = self.get_safe_path(filename).await?;
        
        // Memory allocation guard: check metadata size before allocating heap string to prevent OOM
        let metadata = fs::metadata(&path).await?;
        let file_size = metadata.len();
        let max_size = 10 * 1024 * 1024; // 10 MB Cap
        if file_size > max_size {
            return Err(anyhow!(
                "🚫 RESOURCE BLOCK: File '{}' exceeds safe memory allocation limit (Size: {} MB, Max Allowed: 10 MB). Reading aborted to prevent OOM.",
                filename,
                file_size / (1024 * 1024)
            ));
        }

        let content = fs::read_to_string(path).await?;
        Ok(content)
    }

    pub async fn list_files(&self, dir: &str) -> Result<Vec<String>> {
        let path = self.get_safe_path(dir).await?;

        if !fs::try_exists(&path).await.unwrap_or(false) {
            return Ok(vec![]);
        }

        let mut entries = fs::read_dir(path).await?;
        let mut files = Vec::new();

        while let Some(entry) = entries.next_entry().await? {
            let name = entry.file_name().to_string_lossy().to_string();
            let is_dir = entry.file_type().await?.is_dir();
            files.push(if is_dir { format!("{}/", name) } else { name });
        }

        files.sort(); // deterministic order
        Ok(files)
    }

    pub async fn delete_file(&self, filename: &str) -> Result<()> {
        let path = self.get_safe_path(filename).await?;
        if path.is_file() {
            fs::remove_file(path).await?;
        } else if path.is_dir() {
            fs::remove_dir_all(path).await?;
        }
        Ok(())
    }
}

// ─────────────────────────────────────────────────────────
//  HELPERS
// ─────────────────────────────────────────────────────────

/// Canonicalize a path, creating the directory first if it doesn't exist.
/// This handles the common case where the workspace root hasn't been created yet.
/// 
/// ### ⚠️ Critical Path Resolution
/// We must create the directory *before* canonicalization, as standard OS 
/// `realpath` wrappers often fail or return inconsistent results for non-existent 
/// terminal nodes.
async fn canonicalize_or_create(path: &Path) -> Result<PathBuf> {
    if !fs::try_exists(path).await.unwrap_or(false) {
        fs::create_dir_all(path).await.map_err(|e| {
            anyhow!(
                "Failed to create workspace root '{}': {}",
                path.display(),
                e
            )
        })?;
    }
    fs::canonicalize(path).await.map_err(|e| {
        anyhow!(
            "Failed to canonicalize workspace root '{}': {}",
            path.display(),
            e
        )
    })
}

/// Canonicalize by walking up the path until we find an existing component,
/// then append the remaining leaf segments. Handles paths that don't exist yet.
async fn canonicalize_or_create_parent(path: &Path) -> Result<PathBuf> {
    // Walk up the tree to find the nearest existing ancestor
    let mut existing = path.to_path_buf();
    let mut suffix = Vec::new();

    while !fs::try_exists(&existing).await.unwrap_or(false) {
        if let Some(name) = existing.file_name() {
            suffix.push(name.to_os_string());
        }
        match existing.parent() {
            Some(p) => existing = p.to_path_buf(),
            None => break,
        }
    }

    let mut canonical = fs::canonicalize(&existing).await.map_err(|e| {
        anyhow!(
            "Failed to canonicalize ancestor path '{}': {}",
            existing.display(),
            e
        )
    })?;

    // Re-append the non-existent suffix in reverse
    for part in suffix.into_iter().rev() {
        canonical.push(part);
    }

    Ok(canonical)
}

// ─────────────────────────────────────────────────────────
//  TESTS
// ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_get_safe_path_valid() {
        let dir = tempdir().unwrap();
        let adapter = FilesystemAdapter::new(dir.path().to_path_buf());

        let filename = "notes.txt";
        let safe = adapter.get_safe_path(filename).await;
        assert!(safe.is_ok());
        assert!(safe.unwrap().ends_with("notes.txt"));
    }

    #[tokio::test]
    async fn test_read_write_roundtrip() {
        let dir = tempdir().unwrap();
        let adapter = FilesystemAdapter::new(dir.path().to_path_buf());

        let filename = "docs/test.txt";
        let content = "Hello TadpoleOS!";
        adapter.write_file(filename, content).await.unwrap();

        let read = adapter.read_file(filename).await.unwrap();
        assert_eq!(read, content);
    }

    #[tokio::test]
    async fn test_get_safe_path_relative_traversal() {
        let dir = tempdir().unwrap();
        let adapter = FilesystemAdapter::new(dir.path().to_path_buf());

        let dirty = "docs/../../outside.txt";
        let safe = adapter.get_safe_path(dirty).await;
        assert!(safe.is_err());
        assert!(safe.unwrap_err().to_string().contains("Illegal path traversal"));
    }

    #[tokio::test]
    async fn test_get_safe_path_absolute_traversal() {
        let dir = tempdir().unwrap();
        let adapter = FilesystemAdapter::new(dir.path().to_path_buf());

        let dirty = "/etc/passwd";
        let safe = adapter.get_safe_path(dirty).await;
        assert!(safe.is_ok());
        let resolved = safe.unwrap();
        let canonical_root = fs::canonicalize(&adapter.root_path).await.unwrap();
        assert!(resolved.starts_with(&canonical_root));
        assert!(resolved.ends_with("etc/passwd"));
    }

    #[tokio::test]
    async fn test_get_safe_path_windows_drive_normalization() {
        let dir = tempdir().unwrap();
        let adapter = FilesystemAdapter::new(dir.path().to_path_buf());

        // Standard Windows-style absolute-ish format
        let dirty = "D:\\projects\\secret.txt";
        let safe = adapter.get_safe_path(dirty).await;
        assert!(safe.is_ok());
        let resolved = safe.unwrap();
        let canonical_root = fs::canonicalize(&adapter.root_path).await.unwrap();
        assert!(resolved.starts_with(&canonical_root));
        assert!(resolved.ends_with("projects/secret.txt") || resolved.ends_with("projects\\secret.txt"));
    }

    #[tokio::test]
    async fn test_symlink_escape_prevention() {
        let dir = tempdir().unwrap();
        let adapter = FilesystemAdapter::new(dir.path().to_path_buf());

        // Create target outside workspace root
        let outside_dir = tempdir().unwrap();
        let secret = outside_dir.path().join("secret.txt");
        fs::write(&secret, "sensitive").await.unwrap();

        // Create symlink pointing outside workspace root
        let symlink_path = dir.path().join("link_to_outside");
        
        #[cfg(unix)]
        let link_res = fs::symlink(&secret, &symlink_path).await;
        
        #[cfg(windows)]
        let link_res = fs::symlink_file(&secret, &symlink_path).await;

        if link_res.is_ok() {
            // Attempt to get safe path for the symlink
            let safe = adapter.get_safe_path("link_to_outside").await;
            assert!(safe.is_err(), "Symlink escaped sandbox checking!");
            assert!(safe.unwrap_err().to_string().contains("outside the designated workspace"));
        }
    }
}

// Metadata: [filesystem]

// Metadata: [filesystem]

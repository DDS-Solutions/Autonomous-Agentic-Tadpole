//! @docs ARCHITECTURE:Infrastructure
//!
//! ### AI Assist Note
//! **Vault Adapter**: Orchestrates the persistence of mission findings
//! and high-priority system logs to the local vault. Automatically
//! injects **Horizontal Rules** (`---`) and **UTC Timestamps** before
//! content blocks to maintain a human-readable discovery ledger.
//! Enforces **Path Traversal Protection** to ensure all logs remain
//! within the designated vault boundaries (VLT-01).
//!
//! ### 🔍 Debugging & Observability
//! - **Telemetry Link**: Search `[vault]` in tracing logs.
//! - **Failure Path**: Illegal path traversal attempt (`Illegal path
//!   traversal detected`), permission denied on vault root, or disk
//!   exhaustion preventing log append operations.
//! - **Trace Scope**: `server-rs::adapter::vault`

use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::io::AsyncWriteExt;
use super::filesystem::{canonicalize_or_create, canonicalize_or_create_parent};

pub struct VaultAdapter {
    pub root_path: PathBuf,
}

impl VaultAdapter {
    pub fn new(root_path: PathBuf) -> Self {
        Self { root_path }
    }

    /// Verifies that the path is within the vault and contains no traversal attempts.
    async fn get_safe_path(&self, filename: &str) -> Result<PathBuf> {
        let mut cleaned = filename.replace('\\', "/");
        if cleaned.len() >= 2 {
            let first_char = cleaned.chars().next().unwrap();
            let second_char = cleaned.chars().nth(1).unwrap();
            if first_char.is_ascii_alphabetic() && second_char == ':' {
                cleaned = cleaned[2..].to_string();
            }
        }
        let cleaned = cleaned.trim_start_matches('/');

        let mut candidate = self.root_path.clone();
        for component in Path::new(&cleaned).components() {
            match component {
                std::path::Component::Normal(c) => candidate.push(c),
                std::path::Component::ParentDir => {
                    return Err(anyhow!("Illegal path traversal detected in vault adapter"));
                }
                std::path::Component::RootDir | std::path::Component::Prefix(_) => {}
                _ => {}
            }
        }

        let canonical_root = canonicalize_or_create(&self.root_path).await?;
        let canonical_candidate = canonicalize_or_create_parent(&candidate).await?;

        if !canonical_candidate.starts_with(&canonical_root) {
            return Err(anyhow!("Attempted to access file outside of vault"));
        }

        Ok(canonical_candidate)
    }

    /// Appends findings to a markdown file in the vault using atomic OS append.
    pub async fn append_to_file(&self, filename: &str, content: &str) -> Result<()> {
        let path = self.get_safe_path(filename).await?;

        // Ensure directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }

        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .await?;

        let entry = format!("\n\n---\n### Logged at: {}\n{}", chrono::Utc::now(), content);
        file.write_all(entry.as_bytes()).await?;
        file.flush().await?;
        Ok(())
    }

    #[allow(dead_code)]
    pub async fn read_file(&self, filename: &str) -> Result<String> {
        let path = self.get_safe_path(filename).await?;
        Ok(fs::read_to_string(path).await?)
    }
}





// Metadata: [vault]

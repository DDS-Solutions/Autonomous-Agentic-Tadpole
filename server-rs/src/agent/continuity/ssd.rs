use std::path::PathBuf;
use serde::{Serialize, Deserialize};
use crate::error::AppError;

/// SSCP Context Block: The atomic unit of swapping.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ContextBlock {
    pub id: String,
    pub agent_id: String,
    pub mission_id: String,
    pub tokens: Vec<String>,
    pub metadata: std::collections::HashMap<String, String>,
    pub timestamp: i64,
}

fn sanitize_id(id: &str) -> String {
    id.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').collect()
}

/// Manages high-speed SSD persistence for agent context tiers.
pub struct SSDManager {
    base_path: PathBuf,
}

impl SSDManager {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        let base_path = path.into();
        Self { base_path }
    }

    /// Ensures the cache directory exists on the specified drive.
    pub async fn ensure_layout(&self) -> Result<(), AppError> {
        if !self.base_path.exists() {
            tokio::fs::create_dir_all(&self.base_path).await
                .map_err(|e| AppError::InternalServerError(format!("Failed to create SSCP cache dir on drive: {}", e)))?;
        }
        Ok(())
    }

    /// Serializes and flushes a context block to the SSD "Cold Tier".
    pub async fn flush_block(&self, block: &ContextBlock) -> Result<(), AppError> {
        self.ensure_layout().await?;
        
        let agent_dir = self.base_path.join(sanitize_id(&block.agent_id));
        if !agent_dir.exists() {
            tokio::fs::create_dir_all(&agent_dir).await.ok();
        }

        let file_path = agent_dir.join(format!("{}_{}.mpk", sanitize_id(&block.mission_id), sanitize_id(&block.id)));
        
        // Use MessagePack for high-performance binary serialization (Low I/O overhead)
        let bytes = rmp_serde::to_vec(block)
            .map_err(|e| AppError::InternalServerError(format!("SSCP Serialization failed: {}", e)))?;

        tokio::fs::write(file_path, bytes).await
            .map_err(|e| AppError::InternalServerError(format!("SSCP Write failed: {}", e)))?;

        Ok(())
    }

    /// Re-hydrates a context block from the SSD into RAM.
    pub async fn load_block(&self, agent_id: &str, mission_id: &str, block_id: &str) -> Result<ContextBlock, AppError> {
        let file_path = self.base_path
            .join(sanitize_id(agent_id))
            .join(format!("{}_{}.mpk", sanitize_id(mission_id), sanitize_id(block_id)));

        if !file_path.exists() {
            return Err(AppError::NotFound(format!("SSCP Block {} not found on SSD", block_id)));
        }

        let meta = tokio::fs::metadata(&file_path).await.map_err(AppError::Io)?;
        if meta.len() > 50 * 1024 * 1024 { // 50MB bound
            return Err(AppError::InternalServerError("SSCP Block exceeds max deserialization size (50MB)".into()));
        }

        let bytes = tokio::fs::read(file_path).await
            .map_err(|e| AppError::InternalServerError(format!("SSCP Read failed: {}", e)))?;

        let block: ContextBlock = rmp_serde::from_slice(&bytes)
            .map_err(|e| AppError::InternalServerError(format!("SSCP Deserialization failed: {}", e)))?;

        Ok(block)
    }

    /// Purges mission-specific cache blocks.
    #[allow(dead_code)]
    pub async fn purge_mission(&self, agent_id: &str, mission_id: &str) -> Result<(), AppError> {
        let agent_dir = self.base_path.join(sanitize_id(agent_id));
        if !agent_dir.exists() { return Ok(()); }

        let s_mission_id = sanitize_id(mission_id);
        let mut entries = tokio::fs::read_dir(agent_dir).await.map_err(AppError::Io)?;
        while let Some(entry) = entries.next_entry().await.map_err(AppError::Io)? {
            let filename = entry.file_name().to_string_lossy().to_string();
            if filename.starts_with(&s_mission_id) {
                let _ = tokio::fs::remove_file(entry.path()).await;
            }
        }
        Ok(())
    }

    /// Reads all .mpk blocks for an agent into the OS page cache (real pre-fetch).
    /// This ensures the data is in RAM-backed page cache before the agent needs it.
    pub async fn preload_agent_blocks(&self, agent_id: &str) -> Result<usize, AppError> {
        let agent_dir = self.base_path.join(sanitize_id(agent_id));
        if !agent_dir.exists() { return Ok(0); }

        let mut count = 0;
        let mut entries = tokio::fs::read_dir(&agent_dir).await.map_err(AppError::Io)?;
        while let Some(entry) = entries.next_entry().await.map_err(AppError::Io)? {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "mpk") {
                // Reading the file using memmap2 forces the OS to pull it into the page cache without allocating large heap vectors
                let _ = tokio::task::spawn_blocking(move || {
                    if let Ok(file) = std::fs::File::open(&path) {
                        if let Ok(mmap) = unsafe { memmap2::MmapOptions::new().map(&file) } {
                            let mut _black_box = 0;
                            for chunk in mmap.chunks(4096) {
                                if let Some(&b) = chunk.first() {
                                    _black_box ^= b;
                                }
                            }
                        }
                    }
                }).await;
                count += 1;
            }
        }
        
        if count > 0 {
            tracing::debug!("❄️ [SSCP] Pre-loaded {} blocks into page cache for agent {}", count, agent_id);
        }
        Ok(count)
    }

    /// Returns total bytes consumed by cache on disk.
    #[allow(dead_code)]
    pub async fn cache_size_bytes(&self) -> u64 {
        let mut total: u64 = 0;
        if !self.base_path.exists() { return 0; }

        let Ok(mut top_entries) = tokio::fs::read_dir(&self.base_path).await else { return 0 };
        while let Ok(Some(agent_entry)) = top_entries.next_entry().await {
            if agent_entry.file_type().await.map(|ft| ft.is_dir()).unwrap_or(false) {
                let Ok(mut files) = tokio::fs::read_dir(agent_entry.path()).await else { continue };
                while let Ok(Some(file)) = files.next_entry().await {
                    if let Ok(meta) = file.metadata().await {
                        total += meta.len();
                    }
                }
            }
        }
        total
    }

    /// Garbage collects oldest blocks when cache exceeds `max_bytes`.
    /// Returns the number of blocks deleted.
    #[allow(dead_code)]
    pub async fn gc_oldest(&self, max_bytes: u64) -> Result<usize, AppError> {
        let current = self.cache_size_bytes().await;
        if current <= max_bytes { return Ok(0); }

        tracing::warn!("🗑️ [SSCP] Cache size {} bytes exceeds limit {} bytes. Running GC...", current, max_bytes);

        // Collect all .mpk files with their timestamps
        let mut files: Vec<(std::path::PathBuf, i64)> = Vec::new();
        if !self.base_path.exists() { return Ok(0); }

        let Ok(mut top_entries) = tokio::fs::read_dir(&self.base_path).await else { return Ok(0) };
        while let Ok(Some(agent_entry)) = top_entries.next_entry().await {
            if !agent_entry.file_type().await.map(|ft| ft.is_dir()).unwrap_or(false) { continue; }
            let Ok(mut agent_files) = tokio::fs::read_dir(agent_entry.path()).await else { continue };
            while let Ok(Some(file)) = agent_files.next_entry().await {
                let path = file.path();
                if path.extension().is_some_and(|ext| ext == "mpk") {
                    // Use file modified time as a proxy for age
                    let modified = file.metadata().await
                        .and_then(|m| m.modified())
                        .map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as i64)
                        .unwrap_or(0);
                    files.push((path, modified));
                }
            }
        }

        // Sort oldest first
        files.sort_by_key(|(_, ts)| *ts);

        let mut deleted = 0;
        let mut freed: u64 = 0;
        let target_free = current - max_bytes;

        for (path, _) in &files {
            if freed >= target_free { break; }
            if let Ok(meta) = tokio::fs::metadata(&path).await {
                freed += meta.len();
            }
            let _ = tokio::fs::remove_file(&path).await;
            deleted += 1;
        }

        tracing::info!("🗑️ [SSCP] GC complete: deleted {} blocks, freed {} bytes", deleted, freed);
        Ok(deleted)
    }

    /// Purges blocks older than `max_age` seconds (TTL sweep).
    #[allow(dead_code)]
    pub async fn purge_stale(&self, max_age_secs: u64) -> Result<usize, AppError> {
        if !self.base_path.exists() { return Ok(0); }

        let cutoff = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() - max_age_secs;

        let mut deleted = 0;
        let Ok(mut top_entries) = tokio::fs::read_dir(&self.base_path).await else { return Ok(0) };
        while let Ok(Some(agent_entry)) = top_entries.next_entry().await {
            if !agent_entry.file_type().await.map(|ft| ft.is_dir()).unwrap_or(false) { continue; }
            let Ok(mut agent_files) = tokio::fs::read_dir(agent_entry.path()).await else { continue };
            while let Ok(Some(file)) = agent_files.next_entry().await {
                let path = file.path();
                if !path.extension().is_some_and(|ext| ext == "mpk") { continue; }
                let modified = file.metadata().await
                    .and_then(|m| m.modified())
                    .map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs())
                    .unwrap_or(0);
                if modified < cutoff {
                    let _ = tokio::fs::remove_file(&path).await;
                    deleted += 1;
                }
            }
        }

        if deleted > 0 {
            tracing::info!("🗑️ [SSCP] TTL sweep: purged {} stale blocks (older than {}s)", deleted, max_age_secs);
        }
        Ok(deleted)
    }
}

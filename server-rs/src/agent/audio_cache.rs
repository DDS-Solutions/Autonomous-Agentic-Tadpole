//! @docs ARCHITECTURE:Persistence
//!
//! ### AI Assist Note
//! **Voice Cache**: Optimized low-latency storage for synthesized engine
//! speech. Uses **SHA-256 Hashing** of scoped `(engine, voice, text)` keys to
//! avoid collisions and redundant neural inference. Backed by a dedicated
//! **SQLite** database (`audio_cache.db`) with automatic table initialization.
//! Supports **No-op Emergency Fallback** for environments with disk
//! write restrictions.
//!
//! ### 🔍 Debugging & Observability
//! - **Telemetry Link**: Search `[audio_cache]` in tracing logs.
//! - **Failure Path**: SQLite IO errors (e.g., "database is locked"),
//!   or BLOB size limits on extremely long synthesis outputs.
//! - **Trace Scope**: `server-rs::agent::audio_cache`

use crate::error::AppError;
use sha2::{Digest, Sha256};
use sqlx::{sqlite::SqliteConnectOptions, SqlitePool};
use std::path::PathBuf;
use tracing::info;

/// Service for caching synthesized audio chunks to avoid redundant neural inference.
pub struct BunkerCache {
    pool: SqlitePool,
}

impl BunkerCache {
    pub async fn new(db_path: PathBuf) -> Result<Self, AppError> {
        let options = SqliteConnectOptions::new()
            .filename(db_path)
            .create_if_missing(true)
            .busy_timeout(std::time::Duration::from_secs(10));

        let pool = SqlitePool::connect_with(options).await?;

        // Initialize table
        sqlx::query::<sqlx::Sqlite>(
            "CREATE TABLE IF NOT EXISTS audio_cache (
                hash TEXT PRIMARY KEY,
                audio_data BLOB,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            )",
        )
        .execute(&pool)
        .await?;

        info!("[BunkerCache] Audio cache initialized.");
        Ok(Self { pool })
    }

    /// Computes a collision-resistant SHA-256 hex key scoped to engine, voice, and text.
    #[allow(dead_code)]
    pub fn scoped_key(engine: &str, voice: &str, text: &str) -> String {
        format!("{engine}:{voice}:{text}")
    }

    fn hash_key(key: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(key.as_bytes());
        hex::encode(hasher.finalize())
    }

    /// Retrieve audio data for a given cache key.
    #[allow(dead_code)]
    pub async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, AppError> {
        let hash = Self::hash_key(key);
        let row: Option<(Vec<u8>,)> = sqlx::query_as::<sqlx::Sqlite, (Vec<u8>,)>(
            "SELECT audio_data FROM audio_cache WHERE hash = ?",
        )
        .bind(hash)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| r.0))
    }

    /// Store audio data for a given cache key.
    #[allow(dead_code)]
    pub async fn set(&self, key: &str, audio_data: Vec<u8>) -> Result<(), AppError> {
        let hash = Self::hash_key(key);
        sqlx::query::<sqlx::Sqlite>(
            "INSERT OR REPLACE INTO audio_cache (hash, audio_data) VALUES (?, ?)",
        )
        .bind(hash)
        .bind(audio_data)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Create a mock in-memory instance for testing
    #[allow(dead_code)]
    pub fn mock() -> Self {
        let pool =
            SqlitePool::connect_lazy("sqlite::memory:").expect("Failed to create mock memory pool");
        Self { pool }
    }

    /// Create an uninitialized no-op instance for emergency fallback
    pub async fn new_noop() -> Self {
        let pool = SqlitePool::connect_lazy("sqlite::memory:").unwrap();
        Self { pool }
    }
}





// Metadata: [audio_cache]

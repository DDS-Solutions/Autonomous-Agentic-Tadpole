/*
@docs ARCHITECTURE:SovereignKernel

### AI Assist Note
**🛡️ Tadpole OS: Arbiter**
Core system module providing specialized functionality for the agent swarm.

### 🔍 Debugging & Observability
- **Failure Path**: Unexpected execution drift or type compatibility issues.
- **Telemetry Link**: Traced via active system logging channels.
*/

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use sysinfo::System;
use crate::error::AppError;
use super::ssd::{SSDManager, ContextBlock};


/// SSCP Telemetry: Cache performance counters.
#[derive(Debug)]
#[allow(dead_code)]
pub struct CacheStats {
    pub flushes: u64,
    pub evictions: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub hot_agents: usize,
}

/// Sovereign Context Arbiter: Manages the lifecycle of agent working memory.
/// Coordinates Hot (RAM) ↔ Cold (SSD) tiering for the swarm.
pub struct ContextArbiter {
    /// LRU cache of active agents mapping agent_id to estimated token count
    pub hot_registry: parking_lot::Mutex<lru::LruCache<String, usize>>,
    /// High-performance SSD interface
    pub ssd: Arc<SSDManager>,
    /// Background updated memory pressure percentage (0-100)
    memory_pressure: Arc<AtomicU64>,
    /// Telemetry: total blocks flushed to SSD
    flush_count: AtomicU64,
    /// Telemetry: total evictions triggered
    eviction_count: AtomicU64,
    /// Telemetry: successful cache loads (re-hydrations)
    cache_hit_count: AtomicU64,
    /// Telemetry: failed cache loads (block not found)
    cache_miss_count: AtomicU64,
}

impl ContextArbiter {
    pub fn new(ssd: Arc<SSDManager>) -> Self {
        let memory_pressure = Arc::new(AtomicU64::new(0));
        let pressure_clone = memory_pressure.clone();

        tokio::spawn(async move {
            let mut sys = System::new_all();
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(1000));
            loop {
                interval.tick().await;
                sys.refresh_memory();
                let total = sys.total_memory();
                let used = sys.used_memory();
                if total > 0 {
                    let percent = ((used as f64 / total as f64) * 100.0) as u64;
                    pressure_clone.store(percent, Ordering::Relaxed);
                }
            }
        });

        Self {
            hot_registry: parking_lot::Mutex::new(lru::LruCache::new(std::num::NonZeroUsize::new(10000).unwrap())),
            ssd,
            memory_pressure,
            flush_count: AtomicU64::new(0),
            eviction_count: AtomicU64::new(0),
            cache_hit_count: AtomicU64::new(0),
            cache_miss_count: AtomicU64::new(0),
        }
    }

    /// Checks system health and determines if an eviction is required.
    /// Returns true if RAM usage exceeds 85% (OMLX Threshold).
    pub fn check_vram_pressure(&self) -> bool {
        self.memory_pressure.load(Ordering::Relaxed) > 85
    }

    /// Registers active token usage for an agent.
    pub fn update_agent_load(&self, agent_id: &str, token_count: usize) {
        self.hot_registry.lock().put(agent_id.to_string(), token_count);
    }

    /// Selects the best eviction candidate using LRU ordering.
    /// Returns the least recently used agent.
    pub fn select_eviction_target(&self) -> Option<String> {
        let cache = self.hot_registry.lock();
        cache.peek_lru().map(|(id, _)| id.clone())
    }

    /// Forcefully migrates an agent's working memory to the SSD Cold Tier.
    /// This actually serializes the context data to disk before removing from hot tier.
    pub async fn evict_to_ssd(&self, agent_id: &str, working_memory: Option<&serde_json::Value>) -> Result<(), AppError> {
        tracing::info!("❄️ [SSCP] Evicting agent {} context to Cold Tier (SSD)...", agent_id);
        
        // Flush working memory to SSD if provided
        if let Some(memory) = working_memory {
            let memory_str = serde_json::to_string(memory)
                .map_err(|e| AppError::InternalServerError(format!("Failed to serialize working memory: {}", e)))?;
                
            if !memory_str.is_empty() && memory_str != "{}" {
                let block = ContextBlock {
                    id: "working-memory".to_string(),
                    agent_id: agent_id.to_string(),
                    mission_id: "eviction-snapshot".to_string(),
                    tokens: vec![memory_str],
                    metadata: {
                        let mut m = std::collections::HashMap::new();
                        m.insert("type".to_string(), "eviction".to_string());
                        m.insert("reason".to_string(), "memory_pressure".to_string());
                        m
                    },
                    timestamp: chrono::Utc::now().timestamp(),
                };
                self.ssd.flush_block(&block).await?;
                self.flush_count.fetch_add(1, Ordering::Relaxed);
                tracing::info!("❄️ [SSCP] Flushed {} bytes of working memory for agent {}", 
                    block.tokens[0].len(), agent_id);
            }
        }
        
        // Remove from hot registry
        self.hot_registry.lock().pop(agent_id);
        self.eviction_count.fetch_add(1, Ordering::Relaxed);
        
        Ok(())
    }

    /// Backward-compatible eviction without working memory (signal-only).
    #[allow(dead_code)]
    pub async fn evict_signal(&self, agent_id: &str) -> Result<(), AppError> {
        self.evict_to_ssd(agent_id, None).await
    }

    /// Re-hydrates an agent's evicted working memory from the SSD.
    /// Returns the deserialized working memory if found.
    pub async fn rehydrate(&self, agent_id: &str) -> Option<serde_json::Value> {
        match self.ssd.load_block(agent_id, "eviction-snapshot", "working-memory").await {
            Ok(block) => {
                self.cache_hit_count.fetch_add(1, Ordering::Relaxed);
                if let Some(text) = block.tokens.first() {
                    serde_json::from_str(text).ok()
                } else {
                    None
                }
            }
            Err(_) => {
                self.cache_miss_count.fetch_add(1, Ordering::Relaxed);
                None
            }
        }
    }

    /// Predictive Context Warming: Reads actual block data from SSD into the OS page cache.
    pub async fn warm_up(&self, agent_id: &str) {
        tracing::debug!("❄️ [SSCP] Predictively warming context for agent {}", agent_id);
        let _ = self.ssd.preload_agent_blocks(agent_id).await;
    }

    /// Returns telemetry stats for the SSCP cache.
    #[allow(dead_code)]
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            flushes: self.flush_count.load(Ordering::Relaxed),
            evictions: self.eviction_count.load(Ordering::Relaxed),
            cache_hits: self.cache_hit_count.load(Ordering::Relaxed),
            cache_misses: self.cache_miss_count.load(Ordering::Relaxed),
            hot_agents: self.hot_registry.lock().len(),
        }
    }

    /// Increment flush counter (called by external flush paths like context.rs)
    #[allow(dead_code)]
    pub fn record_flush(&self) {
        self.flush_count.fetch_add(1, Ordering::Relaxed);
    }
}

// Metadata: [arbiter]

// Metadata: [arbiter]

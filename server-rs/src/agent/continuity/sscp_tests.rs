/*
@docs ARCHITECTURE:SovereignKernel

### AI Assist Note
**🛡️ Tadpole OS: Sscp Tests**
Core system module providing specialized functionality for the agent swarm.

### 🔍 Debugging & Observability
- **Failure Path**: Unexpected execution drift or type compatibility issues.
- **Telemetry Link**: Traced via active system logging channels.
*/

//! SSCP Phase 1, 2 & 3 Verification Tests — Drive G
//! 
//! All tests write to G:\Tadpole_Intelligence_Cache to validate
//! real SSD integration. Each test uses a unique subdirectory.

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use crate::agent::continuity::{SSDManager, ContextBlock, ContextArbiter, partition_context};

    const CACHE_ROOT: &str = r"G:\Tadpole_Intelligence_Cache";

    fn test_ssd(name: &str) -> (SSDManager, std::path::PathBuf) {
        let path = std::path::PathBuf::from(CACHE_ROOT).join(format!("test-{}", name));
        (SSDManager::new(&path), path)
    }

    // ─────────────────────────────────────────────────────────────
    //  PHASE 1: SSDManager — Binary I/O on Drive G
    // ─────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_ssd_flush_and_load_roundtrip() {
        let (ssd, root) = test_ssd("roundtrip");

        let block = ContextBlock {
            id: "block-0".to_string(),
            agent_id: "agent-42".to_string(),
            mission_id: "mission-alpha".to_string(),
            tokens: vec!["The quick brown fox jumps over the lazy dog.".to_string()],
            metadata: {
                let mut m = std::collections::HashMap::new();
                m.insert("source".to_string(), "test".to_string());
                m
            },
            timestamp: chrono::Utc::now().timestamp(),
        };

        ssd.flush_block(&block).await.expect("flush_block should succeed");

        let expected_path = root.join("agent-42").join("mission-alpha_block-0.mpk");
        assert!(expected_path.exists(), "MessagePack file must exist at {:?}", expected_path);
        println!("✅ File on disk: {} ({} bytes)", expected_path.display(), std::fs::metadata(&expected_path).unwrap().len());

        let loaded = ssd.load_block("agent-42", "mission-alpha", "block-0").await.unwrap();
        assert_eq!(loaded.id, "block-0");
        assert_eq!(loaded.agent_id, "agent-42");
        assert_eq!(loaded.tokens, vec!["The quick brown fox jumps over the lazy dog."]);
        assert_eq!(loaded.metadata.get("source").unwrap(), "test");
        assert_eq!(loaded.timestamp, block.timestamp);
        println!("✅ Roundtrip integrity verified");

        let _ = tokio::fs::remove_dir_all(&root).await;
    }

    #[tokio::test]
    async fn test_ssd_load_missing_block_returns_not_found() {
        let (ssd, root) = test_ssd("missing");
        let result = ssd.load_block("nonexistent", "no-mission", "block-99").await;
        assert!(result.is_err());
        println!("✅ Missing block correctly returns NotFound");
        let _ = tokio::fs::remove_dir_all(&root).await;
    }

    #[tokio::test]
    async fn test_ssd_purge_mission() {
        let (ssd, root) = test_ssd("purge");

        for i in 0..3 {
            let block = ContextBlock {
                id: format!("block-{}", i),
                agent_id: "agent-7".to_string(),
                mission_id: "target-mission".to_string(),
                tokens: vec![format!("Content {}", i)],
                metadata: std::collections::HashMap::new(),
                timestamp: chrono::Utc::now().timestamp(),
            };
            ssd.flush_block(&block).await.unwrap();
        }

        let keeper = ContextBlock {
            id: "block-0".to_string(),
            agent_id: "agent-7".to_string(),
            mission_id: "other-mission".to_string(),
            tokens: vec!["I should survive".to_string()],
            metadata: std::collections::HashMap::new(),
            timestamp: chrono::Utc::now().timestamp(),
        };
        ssd.flush_block(&keeper).await.unwrap();

        ssd.purge_mission("agent-7", "target-mission").await.unwrap();

        for i in 0..3 {
            assert!(ssd.load_block("agent-7", "target-mission", &format!("block-{}", i)).await.is_err());
        }
        assert!(ssd.load_block("agent-7", "other-mission", "block-0").await.is_ok());
        println!("✅ Purge: 3 target deleted, 1 keeper survived");

        let _ = tokio::fs::remove_dir_all(&root).await;
    }

    // ─────────────────────────────────────────────────────────────
    //  PHASE 1: ContextArbiter — Hot Registry, LRU, Eviction
    // ─────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_arbiter_hot_registry_tracking() {
        let (ssd, root) = test_ssd("arb-track");
        let arbiter = ContextArbiter::new(Arc::new(ssd));

        arbiter.update_agent_load("agent-small", 500);
        arbiter.update_agent_load("agent-medium", 3000);
        arbiter.update_agent_load("agent-large", 8000);

        assert_eq!(arbiter.hot_registry.lock().len(), 3);
        assert_eq!(*arbiter.hot_registry.lock().get("agent-small").unwrap(), 500);
        assert_eq!(*arbiter.hot_registry.lock().get("agent-large").unwrap(), 8000);
        println!("✅ Hot registry: 3 agents tracked with token counts");

        let _ = tokio::fs::remove_dir_all(&root).await;
    }

    #[tokio::test]
    async fn test_arbiter_lru_eviction_prefers_idle() {
        let (ssd, root) = test_ssd("arb-lru");
        let arbiter = ContextArbiter::new(Arc::new(ssd));

        // Register agent-old first (it will have an older last_access)
        arbiter.update_agent_load("agent-old", 3000);
        // Small delay to ensure different timestamps
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        arbiter.update_agent_load("agent-new", 3000);

        // Both have same tokens (3000). LRU score = tokens * idle_secs.
        // agent-old has been idle longer → higher score → eviction target
        let target = arbiter.select_eviction_target();
        assert_eq!(target, Some("agent-old".to_string()),
            "With equal token counts, the older (more idle) agent should be evicted");
        println!("✅ LRU correctly prefers idle agent over fresh one");

        let _ = tokio::fs::remove_dir_all(&root).await;
    }

    #[tokio::test]
    async fn test_arbiter_eviction_empty_registry() {
        let (ssd, root) = test_ssd("arb-empty");
        let arbiter = ContextArbiter::new(Arc::new(ssd));
        assert!(arbiter.select_eviction_target().is_none());
        println!("✅ Empty registry returns None");
        let _ = tokio::fs::remove_dir_all(&root).await;
    }

    #[tokio::test]
    async fn test_arbiter_evict_saves_working_memory() {
        let (ssd, root) = test_ssd("arb-save");
        let ssd = Arc::new(ssd);
        let arbiter = ContextArbiter::new(ssd.clone());

        arbiter.update_agent_load("agent-x", 5000);

        // Simulate working memory
        let memory = serde_json::json!({
            "findings": ["CVE-2026-1234 found", "SSL cert expired"],
            "progress": 0.75
        });

        arbiter.evict_to_ssd("agent-x", Some(&memory)).await.unwrap();

        // Verify agent removed from hot registry
        assert!(arbiter.hot_registry.lock().get("agent-x").is_none());

        // Verify working memory was ACTUALLY written to Drive G
        let loaded = ssd.load_block("agent-x", "eviction-snapshot", "working-memory").await
            .expect("Evicted working memory must be on SSD");
        assert_eq!(loaded.metadata.get("type").unwrap(), "eviction");
        assert_eq!(loaded.metadata.get("reason").unwrap(), "memory_pressure");
        
        let restored: serde_json::Value = serde_json::from_str(&loaded.tokens[0]).unwrap();
        assert_eq!(restored["progress"], 0.75);
        assert_eq!(restored["findings"][0], "CVE-2026-1234 found");
        println!("✅ evict_to_ssd ACTUALLY saved working memory to G:");

        let _ = tokio::fs::remove_dir_all(&root).await;
    }

    #[tokio::test]
    async fn test_arbiter_rehydrate_after_eviction() {
        let (ssd, root) = test_ssd("arb-rehydrate");
        let ssd = Arc::new(ssd);
        let arbiter = ContextArbiter::new(ssd.clone());

        let memory = serde_json::json!({"task": "infrastructure audit", "step": 3});
        arbiter.update_agent_load("agent-r", 2000);
        arbiter.evict_to_ssd("agent-r", Some(&memory)).await.unwrap();

        // Re-hydrate
        let restored = arbiter.rehydrate("agent-r").await;
        assert!(restored.is_some());
        let val = restored.unwrap();
        assert_eq!(val["task"], "infrastructure audit");
        assert_eq!(val["step"], 3);
        println!("✅ Rehydration from SSD successful");

        // Miss case
        let miss = arbiter.rehydrate("never-existed").await;
        assert!(miss.is_none());
        println!("✅ Rehydrate miss returns None");

        let _ = tokio::fs::remove_dir_all(&root).await;
    }

    #[tokio::test]
    async fn test_arbiter_telemetry_counters() {
        let (ssd, root) = test_ssd("arb-telemetry");
        let ssd = Arc::new(ssd);
        let arbiter = ContextArbiter::new(ssd.clone());

        let stats_before = arbiter.stats();
        assert_eq!(stats_before.flushes, 0);
        assert_eq!(stats_before.evictions, 0);

        let memory = serde_json::json!({"data": "test"});
        arbiter.update_agent_load("a1", 100);
        arbiter.evict_to_ssd("a1", Some(&memory)).await.unwrap();
        arbiter.rehydrate("a1").await;
        arbiter.rehydrate("nonexistent").await;

        let stats = arbiter.stats();
        assert_eq!(stats.evictions, 1, "1 eviction");
        assert_eq!(stats.flushes, 1, "1 flush (from eviction)");
        assert_eq!(stats.cache_hits, 1, "1 hit (rehydrate a1)");
        assert_eq!(stats.cache_misses, 1, "1 miss (rehydrate nonexistent)");
        println!("✅ Telemetry: evictions={}, flushes={}, hits={}, misses={}", 
            stats.evictions, stats.flushes, stats.cache_hits, stats.cache_misses);

        let _ = tokio::fs::remove_dir_all(&root).await;
    }

    #[tokio::test]
    async fn test_arbiter_vram_pressure_check() {
        let (ssd, root) = test_ssd("arb-pressure");
        let arbiter = ContextArbiter::new(Arc::new(ssd));
        let pressure = arbiter.check_vram_pressure();
        println!("✅ VRAM pressure: {} (>85% triggers eviction)", pressure);
        let _ = tokio::fs::remove_dir_all(&root).await;
    }

    // ─────────────────────────────────────────────────────────────
    //  PHASE 2: Context Partitioning
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn test_partition_context_small_input() {
        let blocks = partition_context("agent-1", "mission-1", "Agent completed the task.");
        assert_eq!(blocks.len(), 1);
        assert!(blocks[0].tokens[0].contains("Agent completed"));
        println!("✅ Small input → 1 block");
    }

    #[test]
    fn test_partition_context_large_input() {
        let history = "The agent analyzed the system. ".repeat(500);
        let blocks = partition_context("agent-99", "m-audit", &history);
        assert!(blocks.len() > 1);

        let reconstructed: String = blocks.iter()
            .flat_map(|b| b.tokens.iter()).cloned().collect::<Vec<_>>().join("");
        assert_eq!(reconstructed.trim(), history.trim());
        println!("✅ Large input → {} blocks, lossless", blocks.len());
    }

    #[test]
    fn test_partition_context_empty_input() {
        assert!(partition_context("a", "m", "").is_empty());
        println!("✅ Empty → 0 blocks");
    }

    // ─────────────────────────────────────────────────────────────
    //  PHASE 2: SSD Disk Management (GC + TTL + Stats)
    // ─────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_ssd_cache_size_and_gc() {
        let (ssd, root) = test_ssd("gc");

        // Write 5 blocks
        for i in 0..5 {
            let block = ContextBlock {
                id: format!("block-{}", i),
                agent_id: "gc-agent".to_string(),
                mission_id: "gc-mission".to_string(),
                tokens: vec!["x".repeat(1000)],
                metadata: std::collections::HashMap::new(),
                timestamp: chrono::Utc::now().timestamp(),
            };
            ssd.flush_block(&block).await.unwrap();
        }

        let size = ssd.cache_size_bytes().await;
        assert!(size > 0, "Cache size must be non-zero after writes");
        println!("  Cache size: {} bytes", size);

        // GC with a very low limit should delete some blocks
        let deleted = ssd.gc_oldest(100).await.unwrap();
        assert!(deleted > 0, "GC should delete blocks when over limit");
        println!("✅ GC deleted {} blocks to get under 100 bytes", deleted);

        let _ = tokio::fs::remove_dir_all(&root).await;
    }

    #[tokio::test]
    async fn test_ssd_preload_agent_blocks() {
        let (ssd, root) = test_ssd("preload");

        for i in 0..3 {
            let block = ContextBlock {
                id: format!("block-{}", i),
                agent_id: "preload-agent".to_string(),
                mission_id: "m1".to_string(),
                tokens: vec![format!("Data {}", i)],
                metadata: std::collections::HashMap::new(),
                timestamp: 0,
            };
            ssd.flush_block(&block).await.unwrap();
        }

        let count = ssd.preload_agent_blocks("preload-agent").await.unwrap();
        assert_eq!(count, 3, "Should preload all 3 blocks");
        println!("✅ Preloaded {} blocks into page cache", count);

        let zero = ssd.preload_agent_blocks("nobody").await.unwrap();
        assert_eq!(zero, 0);

        let _ = tokio::fs::remove_dir_all(&root).await;
    }

    // ─────────────────────────────────────────────────────────────
    //  PHASE 2: E2E + Concurrent Flood on Drive G
    // ─────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_e2e_partition_flush_load_on_drive_g() {
        let (ssd, root) = test_ssd("e2e");
        let history = "Mission log entry with critical intelligence data. ".repeat(100);
        let blocks = partition_context("agent-e2e", "mission-final", &history);

        for block in &blocks {
            ssd.flush_block(block).await.unwrap();
            let path = root.join("agent-e2e").join(format!("mission-final_{}.mpk", block.id));
            println!("  📄 {} ({} bytes)", path.display(), std::fs::metadata(&path).unwrap().len());
        }

        for block in &blocks {
            let loaded = ssd.load_block("agent-e2e", "mission-final", &block.id).await.unwrap();
            assert_eq!(loaded.tokens, block.tokens);
        }

        ssd.purge_mission("agent-e2e", "mission-final").await.unwrap();
        for block in &blocks {
            assert!(ssd.load_block("agent-e2e", "mission-final", &block.id).await.is_err());
        }
        println!("✅ E2E lifecycle on Drive G complete");

        let _ = tokio::fs::remove_dir_all(&root).await;
    }

    #[tokio::test]
    async fn test_concurrent_swarm_flood_on_drive_g() {
        let path = std::path::PathBuf::from(CACHE_ROOT).join("test-flood");
        let ssd = Arc::new(SSDManager::new(&path));
        let arbiter = Arc::new(ContextArbiter::new(ssd.clone()));

        let mut handles = Vec::new();
        for i in 0..10u64 {
            let ssd_c = ssd.clone();
            let arb_c = arbiter.clone();
            handles.push(tokio::spawn(async move {
                let aid = format!("swarm-{}", i);
                let mid = format!("flood-{}", i);
                arb_c.update_agent_load(&aid, (1000 + i * 500) as usize);

                let block = ContextBlock {
                    id: "block-0".to_string(),
                    agent_id: aid.clone(),
                    mission_id: mid.clone(),
                    tokens: vec![format!("Agent {} data", i)],
                    metadata: std::collections::HashMap::new(),
                    timestamp: chrono::Utc::now().timestamp(),
                };
                ssd_c.flush_block(&block).await.unwrap();
                let loaded = ssd_c.load_block(&aid, &mid, "block-0").await.unwrap();
                assert_eq!(loaded.tokens[0], format!("Agent {} data", i));
            }));
        }

        for h in handles { h.await.unwrap(); }

        assert_eq!(arbiter.hot_registry.lock().len(), 10);
        println!("✅ 10-agent concurrent flood on Drive G passed");

        let _ = tokio::fs::remove_dir_all(&path).await;
    }
}

// Metadata: [sscp_tests]

// Metadata: [sscp_tests]

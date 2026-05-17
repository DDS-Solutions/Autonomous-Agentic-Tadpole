/*
@docs ARCHITECTURE:SovereignKernel

### AI Assist Note
**🛡️ Tadpole OS: Sscp Smoke**
Core system module providing specialized functionality for the agent swarm.

### 🔍 Debugging & Observability
- **Failure Path**: Unexpected execution drift or type compatibility issues.
- **Telemetry Link**: Traced via active system logging channels.
*/

//! Quick smoke test that writes real SSCP blocks to Drive G.
//! Run with: cargo test sscp_drive_g_smoke -- --nocapture --ignored

#[cfg(test)]
mod tests {
    use crate::agent::continuity::{SSDManager, partition_context};

    #[tokio::test]
    #[ignore] // Only run manually — requires Drive G
    async fn sscp_drive_g_smoke() {
        let cache_dir = std::path::PathBuf::from(r"G:\Tadpole_Intelligence_Cache");
        let ssd = SSDManager::new(&cache_dir);

        // 1. Partition a realistic mission history
        let history = "Agent 42 performed a comprehensive infrastructure audit. \
            Checked 47 endpoints, validated SSL certificates, scanned open ports. \
            Found 3 critical vulnerabilities in the authentication layer. \
            Recommended immediate patching of CVE-2026-1234 and CVE-2026-5678. ".repeat(50);

        let blocks = partition_context("agent-42", "mission-infra-audit", &history);
        println!("📦 Partitioned into {} blocks", blocks.len());

        // 2. Flush all blocks to G:\
        for block in &blocks {
            ssd.flush_block(block).await.expect("Drive G flush failed");
            println!("  ✅ Flushed {}", block.id);
        }

        // 3. Verify files exist
        for block in &blocks {
            let path = cache_dir
                .join("agent-42")
                .join(format!("mission-infra-audit_{}.mpk", block.id));
            assert!(path.exists(), "Expected file at {:?}", path);
            let size = std::fs::metadata(&path).unwrap().len();
            println!("  📄 {} — {} bytes", path.display(), size);
        }

        // 4. Load one back to verify integrity
        let loaded = ssd.load_block("agent-42", "mission-infra-audit", "block-0").await.unwrap();
        println!("  🔄 Loaded block-0: {} chars of text", loaded.tokens[0].len());
        assert!(!loaded.tokens[0].is_empty());

        println!("\n✅ Drive G smoke test PASSED. Check G:\\Tadpole_Intelligence_Cache\\agent-42\\");
    }
}

// Metadata: [sscp_smoke]

// Metadata: [sscp_smoke]

/*
@docs ARCHITECTURE:SovereignKernel

### AI Assist Note
**🛡️ Tadpole OS: Hot Reload**
Core system module providing specialized functionality for the agent swarm.

### 🔍 Debugging & Observability
- **Failure Path**: Unexpected execution drift or type compatibility issues.
- **Telemetry Link**: Traced via active system logging channels.
*/

//! Background hot-reload loop for zero-downtime capability synchronization.

use std::sync::Arc;
use tokio::time::{interval_at, Duration, Instant, MissedTickBehavior};
use super::registry::ScriptSkillsRegistry;

/// Spawns a background hot-reload loop that periodically scans for newly synthesized
/// skills, workflows, and hooks, performing zero-downtime atomic state reloads.
pub fn spawn_hot_reload_loop(registry: Arc<ScriptSkillsRegistry>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = interval_at(
            Instant::now() + Duration::from_secs(10),
            Duration::from_secs(10),
        );
        interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            interval.tick().await;
            if let Err(e) = registry.reload_all().await {
                tracing::warn!("⚠️ [ScriptSkillsRegistry] Hot-reload scan error: {}", e);
            }
        }
    })
}

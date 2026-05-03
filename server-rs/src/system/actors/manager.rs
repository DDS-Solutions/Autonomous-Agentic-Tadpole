//! @docs ARCHITECTURE:SovereignKernel
//!
//! ### Actor Manager (Lifecycle Orchestrator)
//! Responsible for spawning system actors and initializing their 
//! communication channels.

use crate::state::AppState;
use crate::system::actors::{ActorRegistry, SystemMessage};
use crate::system::actors::audit_actor::AuditActor;
use tokio::sync::mpsc;
use std::sync::Arc;
use tracing::info;

/// Spawns the core system actors and returns the registry of senders.
pub async fn spawn_system_actors(app_state: &Arc<AppState>) -> ActorRegistry {
    info!("🚀 [Kernel] Spawning System Actors...");

    // 1. Audit Actor
    let (audit_tx, audit_rx) = mpsc::unbounded_channel::<SystemMessage>();
    let audit_trail = (*app_state.security.audit_trail).clone();
    let audit_actor = AuditActor::new(audit_rx, audit_trail);
    tokio::spawn(audit_actor.run());

    // 2. Memory Actor (Placeholder for now)
    let (memory_tx, _memory_rx) = mpsc::unbounded_channel::<SystemMessage>();
    // tokio::spawn(MemoryActor::new(memory_rx, ...).run());

    ActorRegistry {
        audit: audit_tx,
        memory: memory_tx,
    }
}

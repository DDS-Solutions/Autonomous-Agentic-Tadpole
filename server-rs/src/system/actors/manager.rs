//! @docs ARCHITECTURE:SovereignKernel
//!
//! ### Actor Manager (Lifecycle Orchestrator)
//! Responsible for spawning system actors and initializing their 
//! communication channels.

use crate::state::AppState;
use crate::system::actors::{ActorRegistry, SystemMessage};
use crate::system::actors::audit_actor::AuditActor;
<<<<<<< HEAD
use crate::system::actors::memory_actor::MemoryActor;
use crate::system::actors::security_actor::SecurityActor;
use tokio::sync::mpsc;
use std::sync::Arc;
use tracing::{info, error};
=======
use tokio::sync::mpsc;
use std::sync::Arc;
use tracing::info;
>>>>>>> 25d8ad2cee17df5bb53efec00f6716d4f03d43a7

/// Spawns the core system actors and returns the registry of senders.
pub async fn spawn_system_actors(app_state: &Arc<AppState>) -> ActorRegistry {
    info!("🚀 [Kernel] Spawning System Actors...");

    // 1. Audit Actor
    let (audit_tx, audit_rx) = mpsc::unbounded_channel::<SystemMessage>();
    let audit_trail = (*app_state.security.audit_trail).clone();
    let audit_actor = AuditActor::new(audit_rx, audit_trail);
    tokio::spawn(audit_actor.run());

<<<<<<< HEAD
    // 2. Memory Actor
    let (memory_tx, memory_rx) = mpsc::unbounded_channel::<SystemMessage>();
    let base_dir = app_state.base_dir.clone();
    match MemoryActor::new(memory_rx, &base_dir).await {
        Ok(memory_actor) => {
            tokio::spawn(memory_actor.run());
        },
        Err(e) => {
            error!("🚨 [Kernel] Failed to spawn MemoryActor: {}", e);
        }
    }

    // 3. Security Actor
    let (security_tx, security_rx) = mpsc::channel::<SystemMessage>(1024);
    let budget_guard = app_state.security.budget_guard.clone();
    let shell_scanner = app_state.security.shell_scanner.clone();
    let security_actor = SecurityActor::new(security_rx, budget_guard, shell_scanner);
    tokio::spawn(security_actor.run());
=======
    // 2. Memory Actor (Placeholder for now)
    let (memory_tx, _memory_rx) = mpsc::unbounded_channel::<SystemMessage>();
    // tokio::spawn(MemoryActor::new(memory_rx, ...).run());
>>>>>>> 25d8ad2cee17df5bb53efec00f6716d4f03d43a7

    ActorRegistry {
        audit: audit_tx,
        memory: memory_tx,
<<<<<<< HEAD
        security: security_tx,
=======
>>>>>>> 25d8ad2cee17df5bb53efec00f6716d4f03d43a7
    }
}

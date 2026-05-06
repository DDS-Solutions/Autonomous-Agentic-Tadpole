//! @docs ARCHITECTURE:SovereignKernel
//!
//! ### Actor Manager (Lifecycle Orchestrator)
//! Responsible for spawning system actors and initializing their 
//! communication channels.

use crate::state::AppState;
use crate::system::actors::{ActorRegistry, SystemMessage};
use crate::system::actors::audit_actor::AuditActor;
use crate::system::actors::memory_actor::MemoryActor;
use crate::system::actors::security_actor::SecurityActor;
use crate::system::actors::skill_actor::SkillScannerActor;
use tokio::sync::mpsc;
use std::sync::Arc;
use tracing::{info, error};

/// Spawns the core system actors and returns the registry of senders.
pub async fn spawn_system_actors(app_state: &Arc<AppState>) -> ActorRegistry {
    info!("🚀 [Kernel] Spawning System Actors...");
    const CHANNEL_CAPACITY: usize = 1024;

    // 1. Audit Actor
    let (audit_tx, audit_rx) = mpsc::channel::<SystemMessage>(CHANNEL_CAPACITY);
    let audit_trail = (*app_state.security.audit_trail).clone();
    let audit_actor = AuditActor::new(audit_rx, audit_trail);
    tokio::spawn(audit_actor.run());

    // 2. Memory Actor
    let (memory_tx, memory_rx) = mpsc::channel::<SystemMessage>(CHANNEL_CAPACITY);
    let base_dir = app_state.base_dir.clone();
    let pool = app_state.resources.pool.clone();
    match MemoryActor::new(memory_rx, &base_dir, pool).await {
        Ok(memory_actor) => {
            tokio::spawn(memory_actor.run());
        },
        Err(e) => {
            error!("🚨 [Kernel] Failed to spawn MemoryActor: {}", e);
        }
    }

    // 3. Security Actor
    let (security_tx, security_rx) = mpsc::channel::<SystemMessage>(CHANNEL_CAPACITY);
    let budget_guard = app_state.security.budget_guard.clone();
    let shell_scanner = app_state.security.shell_scanner.clone();
    let security_actor = SecurityActor::new(security_rx, budget_guard, shell_scanner);
    tokio::spawn(security_actor.run());

    // 4. Skill Actor
    let (skill_tx, skill_rx) = mpsc::channel::<SystemMessage>(CHANNEL_CAPACITY);
    let skill_actor = SkillScannerActor::new(app_state.clone(), skill_rx);
    tokio::spawn(skill_actor.run());

    ActorRegistry {
        audit: audit_tx,
        memory: memory_tx,
        security: security_tx,
        skill: skill_tx,
    }
}

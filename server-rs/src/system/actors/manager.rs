//! @docs ARCHITECTURE:Infrastructure
//! 
//! ### AI Assist Note
//! **! @docs ARCHITECTURE:SovereignKernel**
//! This module implements high-fidelity logic for the Sovereign Reality layer.
//! 
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Runtime logic error, state desynchronization, or resource exhaustion.
//! - **Telemetry Link**: Search `[manager]` in tracing logs.
//! 
//! @docs ARCHITECTURE:SovereignKernel
//!
//! ### Actor Manager (Lifecycle Orchestrator)
//! Responsible for spawning system actors and initializing their 
//! communication channels.
// Scaffolded actor manager: will be wired into the kernel in a subsequent phase.
#![allow(dead_code)]

use crate::state::AppState;
use crate::system::actors::{ActorRegistry, SystemMessage};
use crate::system::actors::supervisor::{Supervisor, SupervisorStrategy, RestartPolicy};
use crate::system::actors::audit_actor::AuditActor;
use crate::system::actors::memory_actor::MemoryActor;
use crate::system::actors::security_actor::SecurityActor;
use crate::system::actors::skill_actor::SkillScannerActor;
use tokio::sync::{mpsc, Mutex};
use std::sync::Arc;
use tracing::{info, error};

/// Spawns the core system actors under an OTP-style Supervisor.
pub async fn spawn_system_actors(app_state: &Arc<AppState>) -> ActorRegistry {
    info!("🚀 [Kernel] Initializing OTP-style System Actor Supervisor...");
    const CHANNEL_CAPACITY: usize = 1024;

    let supervisor = Supervisor::new(
        "kernel_system_supervisor",
        SupervisorStrategy::OneForOne,
        RestartPolicy::default(),
    );

    // 1. Audit Actor
    let (audit_tx, audit_rx) = mpsc::channel::<SystemMessage>(CHANNEL_CAPACITY);
    let audit_rx = Arc::new(Mutex::new(audit_rx));
    let audit_trail = (*app_state.security.audit_trail).clone();
    {
        let audit_rx = audit_rx.clone();
        let audit_trail = audit_trail.clone();
        supervisor.supervise("audit_actor", move || {
            let actor = AuditActor::new(audit_rx.clone(), audit_trail.clone());
            actor.run()
        });
    }

    // 2. Memory Actor
    let (memory_tx, memory_rx) = mpsc::channel::<SystemMessage>(CHANNEL_CAPACITY);
    let memory_rx = Arc::new(Mutex::new(memory_rx));
    let base_dir = app_state.base_dir.clone();
    let pool = app_state.resources.pool.clone();
    {
        let memory_rx = memory_rx.clone();
        let base_dir = base_dir.clone();
        let pool = pool.clone();
        supervisor.supervise("memory_actor", move || {
            let memory_rx = memory_rx.clone();
            let base_dir = base_dir.clone();
            let pool = pool.clone();
            async move {
                match MemoryActor::new(memory_rx, &base_dir, pool).await {
                    Ok(actor) => actor.run().await,
                    Err(e) => {
                        error!("🚨 [Kernel] Failed to initialize MemoryActor: {}", e);
                    }
                }
            }
        });
    }

    // 3. Security Actor
    let (security_tx, security_rx) = mpsc::channel::<SystemMessage>(CHANNEL_CAPACITY);
    let security_rx = Arc::new(Mutex::new(security_rx));
    let budget_guard = app_state.security.budget_guard.clone();
    let shell_scanner = app_state.security.shell_scanner.clone();
    {
        let security_rx = security_rx.clone();
        let budget_guard = budget_guard.clone();
        let shell_scanner = shell_scanner.clone();
        supervisor.supervise("security_actor", move || {
            let actor = SecurityActor::new(security_rx.clone(), budget_guard.clone(), shell_scanner.clone());
            actor.run()
        });
    }

    // 4. Skill Actor
    let (skill_tx, skill_rx) = mpsc::channel::<SystemMessage>(CHANNEL_CAPACITY);
    let skill_rx = Arc::new(Mutex::new(skill_rx));
    {
        let skill_rx = skill_rx.clone();
        let state_clone = app_state.clone();
        supervisor.supervise("skill_actor", move || {
            let actor = SkillScannerActor::new(state_clone.clone(), skill_rx.clone());
            actor.run()
        });
    }

    ActorRegistry {
        audit: audit_tx,
        memory: memory_tx,
        security: security_tx,
        skill: skill_tx,
    }
}

// Metadata: [manager]


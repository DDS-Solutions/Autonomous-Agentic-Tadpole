//! @docs ARCHITECTURE:SecurityAudit
//!
//! ### Audit Actor (Sequential Sink)
//! Orchestrates the background recording of audit entries. 
//! By isolating the MerkleAuditTrail logic in a single task, we eliminate 
//! lock contention and ensure the hash chain is never forked.

use crate::security::audit::MerkleAuditTrail;
use crate::system::actors::SystemMessage;
use tokio::sync::mpsc::UnboundedReceiver;
use tracing::{info, error};

pub struct AuditActor {
    receiver: UnboundedReceiver<SystemMessage>,
    audit_trail: MerkleAuditTrail,
}

impl AuditActor {
    pub fn new(receiver: UnboundedReceiver<SystemMessage>, audit_trail: MerkleAuditTrail) -> Self {
        Self {
            receiver,
            audit_trail,
        }
    }

    /// Primary execution loop for the Audit Actor.
    pub async fn run(mut self) {
        info!("🛡️ [AuditActor] Initialized and listening for events.");

        while let Some(msg) = self.receiver.recv().await {
            match msg {
                SystemMessage::AuditRecord { 
                    agent_id, 
                    mission_id, 
                    user_id, 
                    action, 
                    params, 
                    resp 
                } => {
                    let result = self.audit_trail.record(
                        &agent_id, 
                        mission_id.as_deref(), 
                        user_id.as_deref(), 
                        &action, 
                        &params
                    ).await.map_err(|e| crate::error::AppError::InternalServerError(e.to_string()));

                    let _ = resp.send(result);
                }
                SystemMessage::Shutdown => {
                    info!("🛑 [AuditActor] Received shutdown signal. Draining...");
                    break;
                }
                _ => {
                    // Ignore non-audit messages
                }
            }
        }

        info!("🛡️ [AuditActor] Shutdown complete.");
    }
}

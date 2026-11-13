//! @docs ARCHITECTURE:Agent
//!
//! ### AI Context Alignment
//! - **Subsystem**: Sovereign Engine / Cooperative Cancellation & Child Task Supervision
//! - **Primary Entrypoints**: `MissionHandle`, `SpawnedChildGuard`, `MissionStatus`, `MissionOutcome`
//!
//! ### ⚠️ Invariants & Non-Negotiables
//! - `[Scheduler]` Structured concurrency: child worker tasks register their `AbortHandle` before execution begins and abort on parent drop or cancellation.
//!
//! ### 🔍 Debugging & Observability
//! - **Telemetry Link**: Search `[swarm_orchestrator]` in tracing logs.

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::Notify;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MissionStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MissionOutcome {
    Success { summary: String, total_tokens: u64 },
    Exhausted { reason: String },
    Aborted { reason: String },
}

/// Cooperative cancellation handle shared across a root mission and its spawned sub-agent tree.
#[derive(Clone, Debug)]
pub struct MissionHandle {
    pub mission_id: String,
    cancelled: Arc<AtomicBool>,
    notify: Arc<Notify>,
}

impl MissionHandle {
    pub fn new(mission_id: impl Into<String>) -> Self {
        Self {
            mission_id: mission_id.into(),
            cancelled: Arc::new(AtomicBool::new(false)),
            notify: Arc::new(Notify::new()),
        }
    }

    /// Creates a child handle sharing the parent's cancellation signal so aborting the root
    /// immediately propagates down the entire swarm tree.
    pub fn child(&self, child_mission_id: impl Into<String>) -> Self {
        Self {
            mission_id: child_mission_id.into(),
            cancelled: Arc::clone(&self.cancelled),
            notify: Arc::clone(&self.notify),
        }
    }

    pub fn abort(&self) {
        if !self.cancelled.swap(true, Ordering::SeqCst) {
            tracing::info!(
                "🛑 [swarm_orchestrator] Cooperative abort triggered for mission '{}'",
                self.mission_id
            );
        }
        self.notify.notify_waiters();
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    /// Resolves once this mission handle (or its shared root) is aborted.
    pub async fn cancelled(&self) {
        loop {
            let notified = self.notify.notified();
            if self.is_cancelled() {
                return;
            }
            notified.await;
        }
    }
}

/// Structured concurrency guard that aborts all tracked child worker `AbortHandle`s on drop.
#[derive(Default)]
pub struct SpawnedChildGuard {
    handles: Vec<tokio::task::AbortHandle>,
}

impl SpawnedChildGuard {
    pub fn new() -> Self {
        Self {
            handles: Vec::new(),
        }
    }

    pub fn register(&mut self, handle: tokio::task::AbortHandle) {
        self.handles.push(handle);
    }

    pub fn abort_all(&mut self) {
        for handle in self.handles.drain(..) {
            handle.abort();
        }
    }
}

impl Drop for SpawnedChildGuard {
    fn drop(&mut self) {
        self.abort_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mission_handle_parent_child_cancellation() {
        let root = MissionHandle::new("root-mission");
        let child = root.child("child-mission");
        assert!(!root.is_cancelled());
        assert!(!child.is_cancelled());

        let waiter = tokio::spawn({
            let child = child.clone();
            async move {
                child.cancelled().await;
                true
            }
        });
        tokio::task::yield_now().await;
        root.abort();

        assert!(root.is_cancelled());
        assert!(child.is_cancelled());
        assert!(waiter.await.unwrap());
    }

    #[tokio::test]
    async fn test_spawned_child_guard_aborts_on_drop() {
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let mut guard = SpawnedChildGuard::new();
        let join = tokio::spawn(async move {
            let _tx = tx;
            futures::future::pending::<()>().await;
        });
        guard.register(join.abort_handle());
        drop(guard);
        assert!(rx.await.is_err(), "Dropping SpawnedChildGuard must abort pending child tasks");
    }
}

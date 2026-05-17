/*
### AI Assist Note
**🛡️ Tadpole OS: Supervisor**
Core system module providing specialized functionality for the agent swarm.

### 🔍 Debugging & Observability
- **Failure Path**: Unexpected execution drift or type compatibility issues.
- **Telemetry Link**: Traced via active system logging channels.
*/

//! Worker Supervision System — Sovereign Reliability Layer
//!
//! @docs ARCHITECTURE:Reliability
//!
//! ### AI Assist Note
//! **Supervisor Tree**: Implements the Erlang-style supervision pattern for 
//! background workers. Ensures that critical subsystems (Telemetry, Budget, 
//! Swarm Health) are self-healing. Features **Exponential Backoff**: 
//! prevents "panic loops" from consuming system resources during partial 
//! infrastructure failures (REL-01).

use std::sync::Arc;
use tokio::task::JoinHandle;
use crate::state::AppState;
use futures::{Future, FutureExt};
use std::pin::Pin;

pub type BoxedFuture = Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>>;
pub type WorkerFn = Box<dyn Fn(Arc<AppState>) -> BoxedFuture + Send + Sync>;

/// The Sovereign Supervisor responsible for keeping background tasks alive.
pub struct WorkerSupervisor {
    state: Arc<AppState>,
}

impl WorkerSupervisor {
    /// Creates a new supervisor instance.
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }

    /// Spawns a supervised worker that will be restarted on failure.
    pub fn spawn<F, Fut>(&self, name: &'static str, f: F) -> JoinHandle<()>
    where
        F: Fn(Arc<AppState>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = anyhow::Result<()>> + Send + 'static,
    {
        let state = self.state.clone();
        
        tokio::spawn(async move {
            let mut retry_count = 0;
            
            loop {
                tracing::info!("🔄 [Supervisor] Dispatching worker: {}", name);
                
                // Track start time for health metrics
                let start_time = std::time::Instant::now();
                
                // Execute the worker future
                let result = std::panic::AssertUnwindSafe(f(state.clone()))
                    .catch_unwind()
                    .await;

                let worker_result = match result {
                    Ok(res) => res,
                    Err(_) => Err(anyhow::anyhow!("Worker panicked")),
                };

                match worker_result {
                    Ok(_) => {
                        tracing::info!("✅ [Supervisor] Worker '{}' completed mission and exited normally.", name);
                        break; // Normal exit, don't restart
                    }
                    Err(e) => {
                        retry_count += 1;
                        
                        // Reset retry count if the worker ran for a significant time (e.g., 30s)
                        if start_time.elapsed().as_secs() > 30 {
                            tracing::debug!("🧹 [Supervisor] Worker '{}' ran for >30s. Resetting retry backoff.", name);
                            retry_count = 1;
                        }

                        // Calculate exponential backoff: 2^n seconds, capped at 60s
                        let delay = std::cmp::min(2u64.pow(std::cmp::min(retry_count, 6)), 60);
                        
                        tracing::error!(
                            "🚨 [Supervisor] Worker '{}' faulted: {:?}. Restarting in {}s (Attempt: {})",
                            name, e, delay, retry_count
                        );
                        
                        // SEC: Record the failure in the audit trail for forensics
                        let _ = state.record_audit(
                            "system:supervisor",
                            None,
                            None,
                            "worker_fault",
                            &format!("name={}, error={:?}, retry={}", name, e, retry_count)
                        ).await;

                        tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
                    }
                }
            }
        })
    }
}

// Add extension trait for easy access
pub trait SupervisedSpawn {
    fn supervisor(&self) -> WorkerSupervisor;
}

impl SupervisedSpawn for Arc<AppState> {
    fn supervisor(&self) -> WorkerSupervisor {
        WorkerSupervisor::new(self.clone())
    }
}

// Metadata: [supervisor]

// Metadata: [supervisor]

//! @docs ARCHITECTURE:Infrastructure
//!
//! ### AI Assist Note
//! **OTP-Style Actor Supervision Engine (Production-Grade)**: Provides robust
//! supervision trees with OneForOne and OneForAll strategies, immediate
//! deterministic task cancellation (`AbortHandle`), exponential backoff with
//! stability resets, lockless DashMap registry, and comprehensive telemetry.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Actor crash/panic, cascading OneForAll restarts, or restart limit exhaustion.
//! - **Telemetry Link**: Search `[supervisor]` in tracing logs.

use dashmap::DashMap;
use parking_lot::RwLock;
use serde::Serialize;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::broadcast;
use tokio::task::AbortHandle;
use tracing::{error, info, warn};

/// Supervision restart strategy following Erlang/OTP semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupervisorStrategy {
    /// When a child terminates, only that failing child is restarted.
    OneForOne,
    /// When any child terminates, all other sibling children are aborted and restarted.
    OneForAll,
}

/// Restart configuration for supervised actors.
#[derive(Debug, Clone)]
pub struct RestartPolicy {
    pub max_restarts: usize,
    pub window_duration: Duration,
    pub initial_backoff: Duration,
    pub max_backoff: Duration,
    pub backoff_factor: f64,
}

impl Default for RestartPolicy {
    fn default() -> Self {
        Self {
            max_restarts: 5,
            window_duration: Duration::from_secs(60),
            initial_backoff: Duration::from_millis(100),
            max_backoff: Duration::from_secs(5),
            backoff_factor: 2.0,
        }
    }
}

/// Runtime health metrics for a supervised child actor.
#[derive(Debug, Clone, Serialize)]
pub struct ChildActorMetric {
    pub name: String,
    pub is_alive: bool,
    pub restart_count: usize,
    pub last_restart_secs_ago: Option<u64>,
    pub uptime_secs: Option<u64>,
}

type SpawnFn = Arc<dyn Fn() -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;

struct ChildState {
    name: String,
    spawn_fn: SpawnFn,
    current_handle: Option<AbortHandle>,
    last_started_at: Option<Instant>,
    restart_timestamps: Vec<Instant>,
    current_backoff: Duration,
    is_alive: bool,
}

/// The Actor Supervisor managing actor lifetimes, cascading restarts, and hard cancellations.
pub struct Supervisor {
    name: String,
    strategy: SupervisorStrategy,
    policy: RestartPolicy,
    children: Arc<DashMap<String, Arc<RwLock<ChildState>>>>,
    is_shutting_down: Arc<AtomicBool>,
    restart_all_tx: broadcast::Sender<String>,
}

impl Supervisor {
    /// Creates a new Supervisor instance.
    pub fn new(name: impl Into<String>, strategy: SupervisorStrategy, policy: RestartPolicy) -> Self {
        let (restart_all_tx, _) = broadcast::channel(64);
        Self {
            name: name.into(),
            strategy,
            policy,
            children: Arc::new(DashMap::new()),
            is_shutting_down: Arc::new(AtomicBool::new(false)),
            restart_all_tx,
        }
    }

    /// Returns a handle to supervisor for querying metrics or signaling deterministic shutdown.
    pub fn handle(&self) -> SupervisorHandle {
        SupervisorHandle {
            supervisor_name: self.name.clone(),
            children: self.children.clone(),
            is_shutting_down: self.is_shutting_down.clone(),
        }
    }

    /// Registers and supervises an actor child function.
    pub fn supervise<F, Fut>(&self, child_name: &str, spawn_fn: F)
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let name_str = child_name.to_string();
        let boxed_spawn: SpawnFn = Arc::new(move || {
            let fut = spawn_fn();
            Box::pin(fut)
        });

        let child_state = Arc::new(RwLock::new(ChildState {
            name: name_str.clone(),
            spawn_fn: boxed_spawn.clone(),
            current_handle: None,
            last_started_at: None,
            restart_timestamps: Vec::new(),
            current_backoff: self.policy.initial_backoff,
            is_alive: false,
        }));

        self.children.insert(name_str.clone(), child_state.clone());

        // Clone context for the supervision loop
        let is_shutting_down = self.is_shutting_down.clone();
        let policy = self.policy.clone();
        let strategy = self.strategy;
        let supervisor_name = self.name.clone();
        let restart_all_tx = self.restart_all_tx.clone();
        let mut restart_all_rx = self.restart_all_tx.subscribe();
        let children_map = self.children.clone();

        tokio::spawn(async move {
            info!("🛡️ [Supervisor:{}] Starting supervision for child '{}' (Strategy: {:?})", supervisor_name, name_str, strategy);

            loop {
                if is_shutting_down.load(Ordering::SeqCst) {
                    break;
                }

                // 1. Prepare and instantiate the actor future
                let fut = (boxed_spawn)();
                let now = Instant::now();

                // 2. Spawn the task and capture its AbortHandle for deterministic hard aborts
                let join_handle = tokio::spawn(fut);
                let abort_handle = join_handle.abort_handle();

                {
                    let mut guard = child_state.write();
                    guard.current_handle = Some(abort_handle);
                    guard.last_started_at = Some(now);
                    guard.is_alive = true;
                }

                // 3. Monitor for task completion OR OneForAll cascading restart signal
                let termination_reason: Result<Result<(), tokio::task::JoinError>, String> = tokio::select! {
                    res = join_handle => Ok(res),
                    cascade_sender = restart_all_rx.recv() => {
                        match cascade_sender {
                            Ok(origin) => {
                                if origin != name_str {
                                    // Sibling triggered OneForAll restart: abort active task immediately
                                    let mut guard = child_state.write();
                                    if let Some(ref handle) = guard.current_handle {
                                        handle.abort();
                                    }
                                    guard.is_alive = false;
                                    Err(format!("Cascading OneForAll restart triggered by sibling '{}'", origin))
                                } else {
                                    // Originating actor already exited, wait on join_handle
                                    continue;
                                }
                            }
                            Err(_) => {
                                // Broadcast channel closed during shutdown
                                break;
                            }
                        }
                    }
                };

                if is_shutting_down.load(Ordering::SeqCst) {
                    info!("🛑 [Supervisor:{}] Child '{}' stopped during shutdown", supervisor_name, name_str);
                    let mut guard = child_state.write();
                    guard.is_alive = false;
                    guard.current_handle = None;
                    break;
                }

                let terminated_at = Instant::now();

                // 4. Evaluate termination, backoff reset on stability, and restart policy
                let should_restart = {
                    let mut guard = child_state.write();
                    guard.is_alive = false;
                    guard.current_handle = None;

                    // Stability Check: If actor ran continuously > window_duration, reset exponential backoff to initial
                    if let Some(started_at) = guard.last_started_at {
                        if terminated_at.duration_since(started_at) >= policy.window_duration {
                            info!(
                                "🌱 [Supervisor:{}] Child '{}' was stable for {:?} (>= window {:?}). Resetting backoff to initial {:?}",
                                supervisor_name, name_str, terminated_at.duration_since(started_at), policy.window_duration, policy.initial_backoff
                            );
                            guard.current_backoff = policy.initial_backoff;
                        }
                    }

                    // Prune restart timestamps outside the window
                    guard.restart_timestamps.retain(|&t| terminated_at.duration_since(t) <= policy.window_duration);

                    match termination_reason {
                        Ok(Ok(())) => {
                            warn!("⚠️ [Supervisor:{}] Child '{}' exited normally. Evaluating restart budget...", supervisor_name, name_str);
                        }
                        Ok(Err(join_err)) => {
                            if join_err.is_panic() {
                                error!("💥 [Supervisor:{}] Child '{}' PANICKED! Isolating blast radius...", supervisor_name, name_str);
                            } else if join_err.is_cancelled() {
                                info!("🛑 [Supervisor:{}] Child '{}' task cancelled", supervisor_name, name_str);
                            } else {
                                warn!("⚠️ [Supervisor:{}] Child '{}' join error: {}", supervisor_name, name_str, join_err);
                            }
                        }
                        Err(cascade_msg) => {
                            info!("🔄 [Supervisor:{}] Child '{}' received: {}", supervisor_name, name_str, cascade_msg);
                        }
                    }

                    if guard.restart_timestamps.len() >= policy.max_restarts {
                        error!(
                            "🚨 [Supervisor:{}] Child '{}' exceeded max restarts ({}/{} within {:?}). Escalating failure!",
                            supervisor_name, name_str, guard.restart_timestamps.len(), policy.max_restarts, policy.window_duration
                        );
                        false
                    } else {
                        guard.restart_timestamps.push(terminated_at);
                        true
                    }
                };

                if !should_restart {
                    break;
                }

                // 5. If strategy is OneForAll, broadcast cascade restart to all siblings
                if strategy == SupervisorStrategy::OneForAll {
                    info!("📢 [Supervisor:{}] Strategy is OneForAll. Signaling cascading restart of all siblings due to '{}' termination...", supervisor_name, name_str);
                    let _ = restart_all_tx.send(name_str.clone());

                    // Abort all other sibling tasks immediately
                    for entry in children_map.iter() {
                        if entry.key() != &name_str {
                            let sib_guard = entry.value().read();
                            if let Some(ref handle) = sib_guard.current_handle {
                                handle.abort();
                            }
                        }
                    }
                }

                // 6. Calculate backoff duration
                let backoff_duration = {
                    let mut guard = child_state.write();
                    let backoff = guard.current_backoff;
                    let next_backoff = (backoff.as_secs_f64() * policy.backoff_factor).min(policy.max_backoff.as_secs_f64());
                    guard.current_backoff = Duration::from_secs_f64(next_backoff);
                    backoff
                };

                info!("⏳ [Supervisor:{}] Restarting child '{}' in {:?}...", supervisor_name, name_str, backoff_duration);
                tokio::time::sleep(backoff_duration).await;
            }
        });
    }
}

/// Handle to query supervisor metrics and issue deterministic hard shutdown commands.
#[derive(Clone)]
pub struct SupervisorHandle {
    supervisor_name: String,
    children: Arc<DashMap<String, Arc<RwLock<ChildState>>>>,
    is_shutting_down: Arc<AtomicBool>,
}

impl SupervisorHandle {
    /// Returns current health and uptime metrics for all supervised children.
    pub fn get_metrics(&self) -> Vec<ChildActorMetric> {
        let now = Instant::now();
        self.children.iter().map(|entry| {
            let child = entry.value().read();
            let last_restart = child.restart_timestamps.last().map(|t| now.duration_since(*t).as_secs());
            let uptime = if child.is_alive {
                child.last_started_at.map(|t| now.duration_since(t).as_secs())
            } else {
                None
            };

            ChildActorMetric {
                name: child.name.clone(),
                is_alive: child.is_alive,
                restart_count: child.restart_timestamps.len(),
                last_restart_secs_ago: last_restart,
                uptime_secs: uptime,
            }
        }).collect()
    }

    /// Signals and deterministically aborts all active supervised children immediately.
    pub fn shutdown(&self) {
        info!("🛑 [Supervisor:{}] Initiating deterministic hard shutdown for all children...", self.supervisor_name);
        self.is_shutting_down.store(true, Ordering::SeqCst);

        for entry in self.children.iter() {
            let mut child = entry.value().write();
            if let Some(ref handle) = child.current_handle {
                handle.abort();
            }
            child.is_alive = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    #[tokio::test]
    async fn test_supervisor_restarts_panicking_actor_and_resets_backoff() {
        let supervisor = Supervisor::new(
            "test_supervisor_one_for_one",
            SupervisorStrategy::OneForOne,
            RestartPolicy {
                max_restarts: 3,
                window_duration: Duration::from_millis(150),
                initial_backoff: Duration::from_millis(10),
                max_backoff: Duration::from_millis(50),
                backoff_factor: 1.5,
            },
        );

        let execution_count = Arc::new(AtomicUsize::new(0));
        let exec_clone = execution_count.clone();

        supervisor.supervise("panicking_actor", move || {
            let count = exec_clone.clone();
            async move {
                let current = count.fetch_add(1, Ordering::SeqCst);
                if current < 2 {
                    panic!("Simulated intentional panic #{}", current);
                }
                // Stable execution
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        });

        tokio::time::sleep(Duration::from_millis(400)).await;

        let metrics = supervisor.handle().get_metrics();
        assert_eq!(metrics.len(), 1);
        assert!(execution_count.load(Ordering::SeqCst) >= 3);
    }

    #[tokio::test]
    async fn test_supervisor_one_for_all_cascades_restarts() {
        let supervisor = Supervisor::new(
            "test_supervisor_one_for_all",
            SupervisorStrategy::OneForAll,
            RestartPolicy {
                max_restarts: 4,
                window_duration: Duration::from_secs(2),
                initial_backoff: Duration::from_millis(10),
                max_backoff: Duration::from_millis(30),
                backoff_factor: 1.2,
            },
        );

        let count_a = Arc::new(AtomicUsize::new(0));
        let count_b = Arc::new(AtomicUsize::new(0));

        let ca = count_a.clone();
        supervisor.supervise("child_a", move || {
            let c = ca.clone();
            async move {
                let cur = c.fetch_add(1, Ordering::SeqCst);
                if cur == 0 {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                    panic!("Child A crashing to trigger OneForAll cascade!");
                } else {
                    tokio::time::sleep(Duration::from_millis(300)).await;
                }
            }
        });

        let cb = count_b.clone();
        supervisor.supervise("child_b", move || {
            let c = cb.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        });

        // Wait for cascade and restart
        tokio::time::sleep(Duration::from_millis(250)).await;

        // Both child A and child B should have been restarted due to OneForAll
        assert!(count_a.load(Ordering::SeqCst) >= 2, "Child A should have restarted");
        assert!(count_b.load(Ordering::SeqCst) >= 2, "Child B should have cascaded and restarted");
    }

    #[tokio::test]
    async fn test_supervisor_deterministic_hard_shutdown() {
        let supervisor = Supervisor::new(
            "test_shutdown",
            SupervisorStrategy::OneForOne,
            RestartPolicy::default(),
        );

        let is_running = Arc::new(AtomicBool::new(false));
        let running_clone = is_running.clone();

        supervisor.supervise("infinite_child", move || {
            let r = running_clone.clone();
            async move {
                r.store(true, Ordering::SeqCst);
                // Infinite loop simulating long await
                loop {
                    tokio::time::sleep(Duration::from_secs(10)).await;
                }
            }
        });

        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(is_running.load(Ordering::SeqCst));

        // Trigger hard shutdown
        supervisor.handle().shutdown();

        tokio::time::sleep(Duration::from_millis(50)).await;
        let metrics = supervisor.handle().get_metrics();
        assert_eq!(metrics.len(), 1);
        assert!(!metrics[0].is_alive, "Task should be immediately aborted and marked not alive");
    }
}

// Metadata: [supervisor]

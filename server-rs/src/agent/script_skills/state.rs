/*
@docs ARCHITECTURE:SovereignKernel

### AI Assist Note
**🛡️ Tadpole OS: State**
Core system module providing specialized functionality for the agent swarm.

### 🔍 Debugging & Observability
- **Failure Path**: Unexpected execution drift or type compatibility issues.
- **Telemetry Link**: Traced via active system logging channels.
*/

//! State definitions and point-in-time snapshots for the capability registry.

use dashmap::DashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use super::model::{HookDefinition, SkillDefinition, WorkflowDefinition};
use super::parse::normalize_alias;

/// Represents a point-in-time state of the capability registry.
/// Uses DashMaps internally for concurrent read-mostly workloads and provides
/// atomic monotonic generation tracking.
#[derive(Debug)]
pub struct RegistryState {
    pub skills: DashMap<String, SkillDefinition>,
    pub workflows: DashMap<String, WorkflowDefinition>,
    pub hooks: DashMap<String, HookDefinition>,
    pub generation: AtomicU64,
}

impl Default for RegistryState {
    fn default() -> Self {
        Self {
            skills: DashMap::new(),
            workflows: DashMap::new(),
            hooks: DashMap::new(),
            generation: AtomicU64::new(0),
        }
    }
}

impl Clone for RegistryState {
    fn clone(&self) -> Self {
        Self {
            skills: self.skills.clone(),
            workflows: self.workflows.clone(),
            hooks: self.hooks.clone(),
            generation: AtomicU64::new(self.generation.load(Ordering::SeqCst)),
        }
    }
}

impl RegistryState {
    /// Creates a new state with a designated starting generation.
    pub fn with_generation(generation: u64) -> Self {
        Self {
            skills: DashMap::new(),
            workflows: DashMap::new(),
            hooks: DashMap::new(),
            generation: AtomicU64::new(generation),
        }
    }

    /// Loads the current generation monotonically.
    pub fn current_generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }

    /// Atomically increments and returns the next generation counter.
    pub fn bump_generation(&self) -> u64 {
        self.generation.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Retrieves a skill by name.
    pub fn get_skill(&self, name: &str) -> Option<SkillDefinition> {
        self.skills.get(name).map(|r| r.value().clone())
    }

    /// Retrieves a workflow by name, resolving normalized aliases at lookup-time
    /// rather than polluting internal collections with duplicates.
    pub fn get_workflow(&self, name: &str) -> Option<WorkflowDefinition> {
        if let Some(wf) = self.workflows.get(name) {
            return Some(wf.value().clone());
        }
        let normalized = normalize_alias(name);
        if let Some(wf) = self.workflows.get(&normalized) {
            return Some(wf.value().clone());
        }
        for entry in self.workflows.iter() {
            if normalize_alias(entry.key()) == normalized || normalize_alias(&entry.value().name) == normalized {
                return Some(entry.value().clone());
            }
        }
        None
    }

    /// Retrieves a hook by name.
    pub fn get_hook(&self, name: &str) -> Option<HookDefinition> {
        self.hooks.get(name).map(|r| r.value().clone())
    }
}

/*
@docs ARCHITECTURE:SovereignKernel

### AI Assist Note
**🛡️ Tadpole OS: Registry**
Core system module providing specialized functionality for the agent swarm.

### 🔍 Debugging & Observability
- **Failure Path**: Unexpected execution drift or type compatibility issues.
- **Telemetry Link**: Traced via active system logging channels.
*/

//! Public facade and synchronization lifecycle for ScriptSkillsRegistry.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use parking_lot::RwLock;
use tokio::fs;
use crate::error::AppError;
use super::model::{Capability, HookDefinition, SkillDefinition, WorkflowDefinition};
use super::tier::Tier;
use super::state::RegistryState;
use super::store::{persist_capability, verified_remove};
use super::loader::ReloadCache;
use super::hot_reload;

/// ### 🏗️ Core Architecture: Capability Registry Facade
/// Manages the discovery, validation, storage, and hot-reloading of custom skills,
/// deterministic workflows, and hooks.
pub struct ScriptSkillsRegistry {
    pub base_dir: PathBuf,
    pub skills_dir: PathBuf,
    pub workflows_dir: PathBuf,
    pub hooks_dir: PathBuf,
    pub agent_skills_dir: PathBuf,
    pub agent_workflows_dir: PathBuf,
    pub agent_hooks_dir: PathBuf,
    pub state: RwLock<Arc<RegistryState>>,
    pub op_lock: tokio::sync::Mutex<()>,
    pub cache: Arc<ReloadCache>,
}

impl ScriptSkillsRegistry {
    /// Initializes the registry from the configured workspace root and loads capabilities.
    pub async fn new() -> Result<Self, AppError> {
        let base_dir = std::env::var("WORKSPACE_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                if std::env::current_dir()
                    .unwrap_or_default()
                    .ends_with("server-rs")
                {
                    PathBuf::from("..")
                } else {
                    PathBuf::from(".")
                }
            });
        let skills_dir = base_dir.join("execution");
        let workflows_dir = base_dir.join("directives");
        let hooks_dir = base_dir.join("hooks");
        let agent_skills_dir = base_dir.join("execution").join("agent_generated").join("skills");
        let agent_workflows_dir = base_dir.join("directives").join("agent_generated");
        let agent_hooks_dir = base_dir.join("hooks").join("agent_generated");

        fs::create_dir_all(&skills_dir).await.map_err(AppError::Io)?;
        fs::create_dir_all(&workflows_dir).await.map_err(AppError::Io)?;
        fs::create_dir_all(&hooks_dir).await.map_err(AppError::Io)?;
        fs::create_dir_all(&agent_skills_dir).await.map_err(AppError::Io)?;
        fs::create_dir_all(&agent_workflows_dir).await.map_err(AppError::Io)?;
        fs::create_dir_all(&agent_hooks_dir).await.map_err(AppError::Io)?;

        let registry = Self {
            base_dir,
            skills_dir,
            workflows_dir,
            hooks_dir,
            agent_skills_dir,
            agent_workflows_dir,
            agent_hooks_dir,
            state: RwLock::new(Arc::new(RegistryState::default())),
            op_lock: tokio::sync::Mutex::new(()),
            cache: Arc::new(ReloadCache::new()),
        };

        registry.reload_all().await?;
        Ok(registry)
    }

    /// Constructs a mock registry anchored to an isolated directory for tests.
    pub fn mock(base_dir: PathBuf) -> Self {
        let skills_dir = base_dir.join("execution");
        let workflows_dir = base_dir.join("directives");
        let hooks_dir = base_dir.join("hooks");
        let agent_skills_dir = base_dir.join("execution").join("agent_generated").join("skills");
        let agent_workflows_dir = base_dir.join("directives").join("agent_generated");
        let agent_hooks_dir = base_dir.join("hooks").join("agent_generated");

        std::fs::create_dir_all(&skills_dir).ok();
        std::fs::create_dir_all(&workflows_dir).ok();
        std::fs::create_dir_all(&hooks_dir).ok();
        std::fs::create_dir_all(&agent_skills_dir).ok();
        std::fs::create_dir_all(&agent_workflows_dir).ok();
        std::fs::create_dir_all(&agent_hooks_dir).ok();

        Self {
            base_dir,
            skills_dir,
            workflows_dir,
            hooks_dir,
            agent_skills_dir,
            agent_workflows_dir,
            agent_hooks_dir,
            state: RwLock::new(Arc::new(RegistryState::default())),
            op_lock: tokio::sync::Mutex::new(()),
            cache: Arc::new(ReloadCache::new()),
        }
    }

    /// Returns the current point-in-time snapshot of the registry.
    pub fn snapshot(&self) -> Arc<RegistryState> {
        self.state.read().clone()
    }

    /// Performs an in-place mutation of the registry state without deep-cloning collections.
    /// Monotonically bumps generation.
    fn mutate_state<F>(&self, f: F)
    where
        F: FnOnce(&RegistryState),
    {
        let guard = self.state.read();
        f(&guard);
        guard.bump_generation();
    }

    /// Spawns a background hot-reload loop.
    pub fn spawn_hot_reload_loop(registry: Arc<Self>) -> tokio::task::JoinHandle<()> {
        hot_reload::spawn_hot_reload_loop(registry)
    }

    /// Scans all tiers concurrently with mtime-caching, enforces precedence,
    /// and performs a zero-downtime atomic state swap.
    pub async fn reload_all(&self) -> Result<(), AppError> {
        let built_in_agent_skills_dir = self.base_dir.join(".agent").join("skills");
        let built_in_agent_workflows_dir = self.base_dir.join(".agent").join("workflows");

        // Concurrent directory reads across all tiers without holding op_lock
        let (
            std_skills_res,
            gen_skills_res,
            built_in_skills_res,
            std_wf_res,
            built_in_wf_res,
            gen_wf_res,
            std_hooks_res,
            gen_hooks_res,
        ) = tokio::join!(
            self.cache.load_skills_from_dir(&self.skills_dir, Tier::User),
            self.cache.load_skills_from_dir(&self.agent_skills_dir, Tier::Agent),
            self.cache.load_built_in_skills(&built_in_agent_skills_dir),
            self.cache.load_workflows_from_dir(&self.workflows_dir, Tier::User),
            self.cache.load_workflows_from_dir(&built_in_agent_workflows_dir, Tier::BuiltIn),
            self.cache.load_workflows_from_dir(&self.agent_workflows_dir, Tier::Agent),
            self.cache.load_hooks_from_dir(&self.hooks_dir, Tier::User),
            self.cache.load_hooks_from_dir(&self.agent_hooks_dir, Tier::Agent)
        );

        let std_skills = std_skills_res.unwrap_or_default();
        let gen_skills = gen_skills_res.unwrap_or_default();
        let built_in_skills = built_in_skills_res.unwrap_or_default();
        let std_wf = std_wf_res.unwrap_or_default();
        let built_in_wf = built_in_wf_res.unwrap_or_default();
        let gen_wf = gen_wf_res.unwrap_or_default();
        let std_hooks = std_hooks_res.unwrap_or_default();
        let gen_hooks = gen_hooks_res.unwrap_or_default();

        // Monotonic generation preservation
        let current_gen = self.snapshot().current_generation();
        let next_state = RegistryState::with_generation(current_gen.wrapping_add(1));

        // Precedence: Built-in (.agent/) > Agent-Generated > User
        // 1. Insert User capabilities
        for (k, v) in std_skills { next_state.skills.insert(k, v); }
        for (k, v) in std_wf { next_state.workflows.insert(k, v); }
        for (k, v) in std_hooks { next_state.hooks.insert(k, v); }

        // 2. Insert Agent-Generated capabilities (shadows user)
        for (k, v) in gen_skills {
            if next_state.skills.contains_key(&k) {
                tracing::warn!("⚠️ [ScriptSkills] Capability collision: agent skill '{}' shadows user definition", k);
            }
            next_state.skills.insert(k, v);
        }
        for (k, v) in gen_wf {
            if next_state.workflows.contains_key(&k) {
                tracing::warn!("⚠️ [ScriptSkills] Capability collision: agent workflow '{}' shadows user definition", k);
            }
            next_state.workflows.insert(k, v);
        }
        for (k, v) in gen_hooks {
            if next_state.hooks.contains_key(&k) {
                tracing::warn!("⚠️ [ScriptSkills] Capability collision: agent hook '{}' shadows user definition", k);
            }
            next_state.hooks.insert(k, v);
        }

        // 3. Insert Built-in capabilities (highest precedence, shadows all)
        for (k, v) in built_in_skills {
            if next_state.skills.contains_key(&k) {
                tracing::warn!("⚠️ [ScriptSkills] Capability collision: built-in skill '{}' shadows lower-precedence definition", k);
            }
            next_state.skills.insert(k, v);
        }
        for (k, v) in built_in_wf {
            if next_state.workflows.contains_key(&k) {
                tracing::warn!("⚠️ [ScriptSkills] Capability collision: built-in workflow '{}' shadows lower-precedence definition", k);
            }
            next_state.workflows.insert(k, v);
        }

        let total_skills = next_state.skills.len();
        let total_workflows = next_state.workflows.len();
        let total_hooks = next_state.hooks.len();

        // Atomic swap
        {
            let mut write_guard = self.state.write();
            *write_guard = Arc::new(next_state);
        }

        tracing::info!(
            "[ScriptSkillsRegistry] Hot-reload complete: {} skills, {} workflows, {} hooks loaded.",
            total_skills, total_workflows, total_hooks
        );

        Ok(())
    }

    /// Internal generic save implementation unifying collision checks, atomic write, and state mutation.
    async fn save_capability_internal<T: Capability>(
        &self,
        dir: &Path,
        mut capability: T,
        tier: Tier,
    ) -> Result<(), AppError> {
        let _lock = self.op_lock.lock().await;

        if tier.enforce_oversight() {
            capability.set_oversight(true);
        }
        capability.set_category(tier.category().to_string());

        persist_capability(dir, &capability).await?;
        Ok(())
    }

    /// Internal generic delete implementation with built-in immutability guard.
    async fn delete_capability_internal<T: Capability>(
        &self,
        name: &str,
        user_dir: &Path,
        agent_dir: &Path,
        built_in_dir: &Path,
    ) -> Result<(), AppError> {
        let _lock = self.op_lock.lock().await;

        // Guard against deleting built-in capabilities
        let canonical_built_in = super::parse::collision_safe_skill_filename(name, T::EXT);
        let built_in_path = built_in_dir.join(&canonical_built_in);
        let built_in_raw = built_in_dir.join(format!("{name}.{}", T::EXT));
        let built_in_dir_file = built_in_dir.join(name).join("SKILL.md");

        if fs::try_exists(&built_in_path).await.unwrap_or(false)
            || fs::try_exists(&built_in_raw).await.unwrap_or(false)
            || fs::try_exists(&built_in_dir_file).await.unwrap_or(false)
        {
            return Err(AppError::Conflict("Built-in capability is immutable and cannot be deleted".to_string()));
        }

        // Delete from user and agent directories with verified remove
        verified_remove::<T>(user_dir, name).await?;
        verified_remove::<T>(agent_dir, name).await?;
        Ok(())
    }

    // --- Public Facade Methods ---

    pub async fn save_skill(&self, skill: SkillDefinition) -> Result<(), AppError> {
        let name = skill.name.clone();
        self.save_capability_internal(&self.skills_dir, skill.clone(), Tier::User).await?;
        self.mutate_state(|s| {
            s.skills.insert(name, skill);
        });
        Ok(())
    }

    pub async fn save_agent_skill(&self, skill: SkillDefinition) -> Result<(), AppError> {
        let name = skill.name.clone();
        self.save_capability_internal(&self.agent_skills_dir, skill.clone(), Tier::Agent).await?;
        self.mutate_state(|s| {
            s.skills.insert(name, skill);
        });
        Ok(())
    }

    pub async fn save_workflow(&self, workflow: WorkflowDefinition) -> Result<(), AppError> {
        let name = workflow.name.clone();
        self.save_capability_internal(&self.workflows_dir, workflow.clone(), Tier::User).await?;
        self.mutate_state(|s| {
            s.workflows.insert(name, workflow);
        });
        Ok(())
    }

    pub async fn save_agent_workflow(&self, workflow: WorkflowDefinition) -> Result<(), AppError> {
        let name = workflow.name.clone();
        self.save_capability_internal(&self.agent_workflows_dir, workflow.clone(), Tier::Agent).await?;
        self.mutate_state(|s| {
            s.workflows.insert(name, workflow);
        });
        Ok(())
    }

    pub async fn save_hook(&self, hook: HookDefinition) -> Result<(), AppError> {
        let name = hook.name.clone();
        self.save_capability_internal(&self.hooks_dir, hook.clone(), Tier::User).await?;
        self.mutate_state(|s| {
            s.hooks.insert(name, hook);
        });
        Ok(())
    }

    pub async fn save_agent_hook(&self, hook: HookDefinition) -> Result<(), AppError> {
        let name = hook.name.clone();
        self.save_capability_internal(&self.agent_hooks_dir, hook.clone(), Tier::Agent).await?;
        self.mutate_state(|s| {
            s.hooks.insert(name, hook);
        });
        Ok(())
    }

    pub async fn delete_skill(&self, name: &str) -> Result<(), AppError> {
        // Check snapshot category: built_in skills cannot be deleted
        if let Some(skill) = self.snapshot().get_skill(name) {
            if skill.category == "built_in" {
                return Err(AppError::Conflict("Built-in capability is immutable and cannot be deleted".to_string()));
            }
        }

        let built_in_dir = self.base_dir.join(".agent").join("skills");
        self.delete_capability_internal::<SkillDefinition>(
            name,
            &self.skills_dir,
            &self.agent_skills_dir,
            &built_in_dir,
        ).await?;

        self.mutate_state(|s| {
            s.skills.remove(name);
        });
        Ok(())
    }

    pub async fn delete_workflow(&self, name: &str) -> Result<(), AppError> {
        let canonical_name = self.snapshot().get_workflow(name)
            .map(|w| w.name)
            .unwrap_or_else(|| name.to_string());

        if let Some(wf) = self.snapshot().get_workflow(&canonical_name) {
            if wf.category == "built_in" {
                return Err(AppError::Conflict("Built-in capability is immutable and cannot be deleted".to_string()));
            }
        }

        let built_in_dir = self.base_dir.join(".agent").join("workflows");
        self.delete_capability_internal::<WorkflowDefinition>(
            &canonical_name,
            &self.workflows_dir,
            &self.agent_workflows_dir,
            &built_in_dir,
        ).await?;

        self.mutate_state(|s| {
            s.workflows.remove(&canonical_name);
            s.workflows.remove(name);
        });
        Ok(())
    }

    pub async fn delete_hook(&self, name: &str) -> Result<(), AppError> {
        if let Some(hook) = self.snapshot().get_hook(name) {
            if hook.category == "built_in" {
                return Err(AppError::Conflict("Built-in capability is immutable and cannot be deleted".to_string()));
            }
        }

        let built_in_dir = self.base_dir.join(".agent").join("hooks");
        self.delete_capability_internal::<HookDefinition>(
            name,
            &self.hooks_dir,
            &self.agent_hooks_dir,
            &built_in_dir,
        ).await?;

        self.mutate_state(|s| {
            s.hooks.remove(name);
        });
        Ok(())
    }

    pub async fn register_capability(
        &self,
        cap_type: &str,
        data: serde_json::Value,
        category: &str,
    ) -> Result<String, AppError> {
        let is_agent = category == "ai";
        match cap_type {
            "skill" => {
                let mut skill: SkillDefinition = serde_json::from_value(data).map_err(|e| AppError::BadRequest(e.to_string()))?;
                let name = skill.name.clone();
                if is_agent {
                    self.save_agent_skill(skill).await?;
                } else {
                    skill.category = category.to_string();
                    self.save_skill(skill).await?;
                }
                Ok(name)
            }
            "workflow" => {
                let mut workflow: WorkflowDefinition = serde_json::from_value(data).map_err(|e| AppError::BadRequest(e.to_string()))?;
                let name = workflow.name.clone();
                if is_agent {
                    self.save_agent_workflow(workflow).await?;
                } else {
                    workflow.category = category.to_string();
                    self.save_workflow(workflow).await?;
                }
                Ok(name)
            }
            "hook" => {
                let mut hook: HookDefinition = serde_json::from_value(data).map_err(|e| AppError::BadRequest(e.to_string()))?;
                let name = hook.name.clone();
                if is_agent {
                    self.save_agent_hook(hook).await?;
                } else {
                    hook.category = category.to_string();
                    self.save_hook(hook).await?;
                }
                Ok(name)
            }
            _ => Err(AppError::BadRequest(format!("Unknown capability type: {}", cap_type))),
        }
    }
}

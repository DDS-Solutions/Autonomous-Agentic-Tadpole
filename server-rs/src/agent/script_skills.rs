//! @docs ARCHITECTURE:Agent
//!
//! ### AI Assist Note
//! **Dynamic Capabilities**: Orchestrates the discovery and execution of
//! **Custom Skills** (JSON) and **Deterministic Workflows** (Markdown).
//! Features a dual-loader system for standard and agent-specific
//! directories. Supports **Frontmatter Extraction** (YAML) and
//! **Schema Validation** for autonomously discovered toolsets.
//!
//! ### 🛡️ Nexus Synthesis Hardening
//! This implementation follows the **Zero-Downtime Reload** pattern via atomic
//! state snapshots. I/O operations are parallelized for performance, and
//! persistence is made atomic via temp-file rotation.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Invalid YAML frontmatter in `SKILL.md`, duplicate
//!   skill names in the `DashMap`, or `WORKSPACE_ROOT` resolution failure.
//! - **Trace Scope**: `server-rs::agent::script_skills`

use serde::{Deserialize, Serialize};
use serde_json::json;
use dashmap::DashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs;
use parking_lot::RwLock;
use crate::error::AppError;

/// ### 🏗️ Core Architecture: Registry Snapshot
/// Represents a point-in-time state of the capability registry.
/// Uses DashMaps internally to allow for optimistic partial updates while
/// supporting atomic full-state swaps.
#[derive(Debug, Default)]
pub struct RegistryState {
    pub skills: DashMap<String, SkillDefinition>,
    pub workflows: DashMap<String, WorkflowDefinition>,
    pub hooks: DashMap<String, HookDefinition>,
}

/// Represents a dynamic skill loaded from `data/skills/*.json`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillDefinition {
    pub id: Option<String>,
    pub name: String,
    pub description: String,
    pub execution_command: String,
    pub schema: serde_json::Value,
    #[serde(default = "default_oversight")]
    pub oversight_required: bool,
    pub doc_url: Option<String>,
    pub tags: Option<Vec<String>>,
    pub full_instructions: Option<String>,
    pub negative_constraints: Option<Vec<String>>,
    pub verification_script: Option<String>,
    #[serde(default = "default_category")]
    pub category: String,
}

fn default_category() -> String {
    "user".to_string()
}

fn default_oversight() -> bool {
    true
}

/// Represents a dynamic workflow loaded from `data/workflows/*.md`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowDefinition {
    pub id: Option<String>,
    pub name: String,
    pub content: String,
    pub doc_url: Option<String>,
    pub tags: Option<Vec<String>>,
    #[serde(default = "default_category")]
    pub category: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookDefinition {
    pub name: String,
    pub description: String,
    pub hook_type: String, // e.g., "pre_validation", "post_analysis"
    pub content: String,
    pub active: bool,
    #[serde(default = "default_category")]
    pub category: String,
}

/// The Skills registry holding in-memory maps of skills and workflows.
pub struct ScriptSkillsRegistry {
    skills_dir: PathBuf,
    workflows_dir: PathBuf,
    hooks_dir: PathBuf,
    agent_skills_dir: PathBuf,
    agent_workflows_dir: PathBuf,
    agent_hooks_dir: PathBuf,
    /// The atomic state container.
    state: RwLock<Arc<RegistryState>>,
}

impl ScriptSkillsRegistry {
    /// ### 🏗️ Core Architecture: Dynamic Capability Registry
    /// Initializes the in-memory registry with a zero-downtime snapshot system.
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
        let agent_root = skills_dir.join("agent_generated");

        let agent_skills_dir = agent_root.join("skills");
        let agent_workflows_dir = agent_root.join("workflows");
        let agent_hooks_dir = agent_root.join("hooks");

        // Ensure directories exist
        fs::create_dir_all(&skills_dir).await.map_err(AppError::Io)?;
        fs::create_dir_all(&workflows_dir).await.map_err(AppError::Io)?;
        fs::create_dir_all(&hooks_dir).await.map_err(AppError::Io)?;
        fs::create_dir_all(&agent_skills_dir).await.map_err(AppError::Io)?;
        fs::create_dir_all(&agent_workflows_dir).await.map_err(AppError::Io)?;
        fs::create_dir_all(&agent_hooks_dir).await.map_err(AppError::Io)?;

        let registry = Self {
            skills_dir,
            workflows_dir,
            hooks_dir,
            agent_skills_dir,
            agent_workflows_dir,
            agent_hooks_dir,
            state: RwLock::new(Arc::new(RegistryState::default())),
        };

        registry.reload_all().await?;
        Ok(registry)
    }

    /// Create a mock registry for testing with isolated directories.
    pub fn mock(base_dir: PathBuf) -> Self {
        let skills_dir = base_dir.join("execution");
        let workflows_dir = base_dir.join("directives");
        let hooks_dir = base_dir.join("hooks");
        let agent_root = skills_dir.join("agent_generated");
        let agent_skills_dir = agent_root.join("skills");
        let agent_workflows_dir = agent_root.join("workflows");
        let agent_hooks_dir = agent_root.join("hooks");

        // SEC: Create mock directories synchronously for test isolation
        std::fs::create_dir_all(&skills_dir).ok();
        std::fs::create_dir_all(&workflows_dir).ok();
        std::fs::create_dir_all(&hooks_dir).ok();
        std::fs::create_dir_all(&agent_skills_dir).ok();
        std::fs::create_dir_all(&agent_workflows_dir).ok();
        std::fs::create_dir_all(&agent_hooks_dir).ok();

        Self {
            skills_dir,
            workflows_dir,
            hooks_dir,
            agent_skills_dir,
            agent_workflows_dir,
            agent_hooks_dir,
            state: RwLock::new(Arc::new(RegistryState::default())),
        }
    }

    /// Returns the current point-in-time snapshot of the registry.
    pub fn snapshot(&self) -> Arc<RegistryState> {
        self.state.read().clone()
    }

    /// ### 📡 Synchronization: reload_all
    /// Scans all directories concurrently and atomically swaps the registry state.
    pub async fn reload_all(&self) -> Result<(), AppError> {
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

        let built_in_agent_skills_dir = base_dir.join(".agent").join("skills");
        let built_in_agent_workflows_dir = base_dir.join(".agent").join("workflows");

        let next_state = RegistryState::default();

        // --- Parallel Load Execution ---
        // We use tokio::join! to perform directory scans in parallel, 
        // significantly reducing startup/reload latency.
        let (
            std_skills, 
            gen_skills, 
            built_in_skills, 
            std_wf, 
            built_in_wf, 
            gen_wf, 
            std_hooks, 
            gen_hooks
        ) = tokio::join!(
            Self::load_skills_from_dir(&self.skills_dir, "user"),
            Self::load_skills_from_dir(&self.agent_skills_dir, "ai"),
            Self::load_built_in_skills(&built_in_agent_skills_dir),
            Self::load_workflows_from_dir(&self.workflows_dir, "user"),
            Self::load_workflows_from_dir(&built_in_agent_workflows_dir, "ai"),
            Self::load_workflows_from_dir(&self.agent_workflows_dir, "ai"),
            Self::load_hooks_from_dir(&self.hooks_dir, "user"),
            Self::load_hooks_from_dir(&self.agent_hooks_dir, "ai")
        );

        // Merge results into the next state
        for (k, v) in std_skills { next_state.skills.insert(k, v); }
        for (k, v) in gen_skills { next_state.skills.insert(k, v); }
        for (k, v) in built_in_skills { next_state.skills.insert(k, v); }
        for (k, v) in std_wf { next_state.workflows.insert(k, v); }
        for (k, v) in built_in_wf { next_state.workflows.insert(k, v); }
        for (k, v) in gen_wf { next_state.workflows.insert(k, v); }
        for (k, v) in std_hooks { next_state.hooks.insert(k, v); }
        for (k, v) in gen_hooks { next_state.hooks.insert(k, v); }

        // ATOMIC SWAP: No downtime for readers
        let mut write_guard = self.state.write();
        *write_guard = Arc::new(next_state);

        Ok(())
    }

    async fn load_skills_from_dir(dir: &Path, category: &str) -> Vec<(String, SkillDefinition)> {
        let mut results = Vec::new();
        if let Ok(mut entries) = fs::read_dir(dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("json") {
                    if let Ok(content) = read_file_bounded(&path, 5_000_000).await {
                        if let Ok(mut skill) = serde_json::from_str::<SkillDefinition>(&content) {
                            skill.category = category.to_string();
                            results.push((skill.name.clone(), skill));
                        }
                    }
                }
            }
        }
        results
    }

    async fn load_built_in_skills(dir: &Path) -> Vec<(String, SkillDefinition)> {
        let mut results = Vec::new();
        if let Ok(mut entries) = fs::read_dir(dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let path = entry.path();
                if path.is_dir() {
                    let skill_md = path.join("SKILL.md");
                    if skill_md.exists() {
                        if let Ok(content) = read_file_bounded(&skill_md, 1_000_000).await {
                            if let Some(skill) = parse_skill_md(&content) {
                                results.push((skill.name.clone(), skill));
                            }
                        }
                    }
                }
            }
        }
        results
    }

    async fn load_workflows_from_dir(dir: &Path, category: &str) -> Vec<(String, WorkflowDefinition)> {
        let mut results = Vec::new();
        if let Ok(mut entries) = fs::read_dir(dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("md") {
                    if let Ok(content) = read_file_bounded(&path, 2_000_000).await {
                        let name = match path.file_stem() {
                            Some(s) => s.to_string_lossy().to_string(),
                            None => continue,
                        };
                        results.push((name.clone(), WorkflowDefinition {
                            id: None,
                            name,
                            content,
                            doc_url: None,
                            tags: None,
                            category: category.to_string(),
                        }));
                    }
                }
            }
        }
        results
    }

    async fn load_hooks_from_dir(dir: &Path, category: &str) -> Vec<(String, HookDefinition)> {
        let mut results = Vec::new();
        if let Ok(mut entries) = fs::read_dir(dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("json") {
                    if let Ok(content) = read_file_bounded(&path, 500_000).await {
                        if let Ok(mut hook) = serde_json::from_str::<HookDefinition>(&content) {
                            hook.category = category.to_string();
                            results.push((hook.name.clone(), hook));
                        }
                    }
                }
            }
        }
        results
    }

    /// ### 🛡️ Security: Atomic Write Pattern
    /// Persists a skill by writing to a temporary file and performing a 
    /// rename, ensuring disk integrity even on power failure or crash.
    pub async fn save_skill(&self, skill: SkillDefinition) -> Result<(), AppError> {
        let safe_name = crate::utils::security::sanitize_id(&skill.name);
        let filename = format!("{}.json", safe_name);
        let path = crate::utils::security::validate_path(&self.skills_dir, &filename).map_err(|e| AppError::InternalServerError(e.to_string()))?;

        let content = serde_json::to_string_pretty(&skill).map_err(|e| AppError::InternalServerError(e.to_string()))?;
        self.atomic_write(&path, content.as_bytes()).await?;

        // Update local state without waiting for full reload (Optimistic UI)
        self.snapshot().skills.insert(skill.name.clone(), skill);
        Ok(())
    }

    pub async fn save_agent_skill(&self, mut skill: SkillDefinition) -> Result<(), AppError> {
        let safe_name = crate::utils::security::sanitize_id(&skill.name);
        let filename = format!("{}.json", safe_name);
        let path = crate::utils::security::validate_path(&self.agent_skills_dir, &filename).map_err(|e| AppError::InternalServerError(e.to_string()))?;

        skill.category = "ai".to_string();
        let content = serde_json::to_string_pretty(&skill).map_err(|e| AppError::InternalServerError(e.to_string()))?;
        self.atomic_write(&path, content.as_bytes()).await?;

        self.snapshot().skills.insert(skill.name.clone(), skill);
        Ok(())
    }

    pub async fn save_agent_workflow(&self, mut workflow: WorkflowDefinition) -> Result<(), AppError> {
        let safe_name = crate::utils::security::sanitize_id(&workflow.name);
        let filename = format!("{}.md", safe_name);
        let path = crate::utils::security::validate_path(&self.agent_workflows_dir, &filename).map_err(|e| AppError::InternalServerError(e.to_string()))?;

        workflow.category = "ai".to_string();
        self.atomic_write(&path, workflow.content.as_bytes()).await?;

        self.snapshot().workflows.insert(workflow.name.clone(), workflow);
        Ok(())
    }

    pub async fn save_agent_hook(&self, mut hook: HookDefinition) -> Result<(), AppError> {
        let safe_name = crate::utils::security::sanitize_id(&hook.name);
        let filename = format!("{}.json", safe_name);
        let path = crate::utils::security::validate_path(&self.agent_hooks_dir, &filename).map_err(|e| AppError::InternalServerError(e.to_string()))?;

        hook.category = "ai".to_string();
        let content = serde_json::to_string_pretty(&hook).map_err(|e| AppError::InternalServerError(e.to_string()))?;
        self.atomic_write(&path, content.as_bytes()).await?;

        self.snapshot().hooks.insert(hook.name.clone(), hook);
        Ok(())
    }

    pub async fn delete_skill(&self, name: &str) -> Result<(), AppError> {
        let safe_name = crate::utils::security::sanitize_id(name);
        let filename = format!("{}.json", safe_name);
        let path = crate::utils::security::validate_path(&self.skills_dir, &filename).map_err(|e| AppError::InternalServerError(e.to_string()))?;

        if path.exists() {
            fs::remove_file(path).await.map_err(AppError::Io)?;
        }
        self.snapshot().skills.remove(name);
        Ok(())
    }

    pub async fn save_workflow(&self, workflow: WorkflowDefinition) -> Result<(), AppError> {
        let safe_name = crate::utils::security::sanitize_id(&workflow.name);
        let filename = format!("{}.md", safe_name);
        let path = crate::utils::security::validate_path(&self.workflows_dir, &filename).map_err(|e| AppError::InternalServerError(e.to_string()))?;

        self.atomic_write(&path, workflow.content.as_bytes()).await?;

        self.snapshot().workflows.insert(workflow.name.clone(), workflow);
        Ok(())
    }

    pub async fn delete_workflow(&self, name: &str) -> Result<(), AppError> {
        let safe_name = crate::utils::security::sanitize_id(name);
        let filename = format!("{}.md", safe_name);
        let path = crate::utils::security::validate_path(&self.workflows_dir, &filename).map_err(|e| AppError::InternalServerError(e.to_string()))?;

        if path.exists() {
            fs::remove_file(path).await.map_err(AppError::Io)?;
        }
        self.snapshot().workflows.remove(name);
        Ok(())
    }

    pub async fn save_hook(&self, hook: HookDefinition) -> Result<(), AppError> {
        let safe_name = crate::utils::security::sanitize_id(&hook.name);
        let filename = format!("{}.json", safe_name);
        let path = crate::utils::security::validate_path(&self.hooks_dir, &filename).map_err(|e| AppError::InternalServerError(e.to_string()))?;
        let content = serde_json::to_string_pretty(&hook).map_err(|e| AppError::InternalServerError(e.to_string()))?;
        self.atomic_write(&path, content.as_bytes()).await?;
        self.snapshot().hooks.insert(hook.name.clone(), hook);
        Ok(())
    }

    pub async fn delete_hook(&self, name: &str) -> Result<(), AppError> {
        let safe_name = crate::utils::security::sanitize_id(name);
        let filename = format!("{}.json", safe_name);
        let path = crate::utils::security::validate_path(&self.hooks_dir, &filename).map_err(|e| AppError::InternalServerError(e.to_string()))?;
        if path.exists() {
            fs::remove_file(path).await.map_err(AppError::Io)?;
        }
        self.snapshot().hooks.remove(name);
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

    async fn atomic_write(&self, path: &Path, content: &[u8]) -> Result<(), AppError> {
        let tmp_path = path.with_extension("tmp");
        fs::write(&tmp_path, content).await.map_err(AppError::Io)?;
        fs::rename(&tmp_path, path).await.map_err(AppError::Io)?;
        Ok(())
    }
}

/// Helper to read file with size bounds to prevent resource exhaustion.
async fn read_file_bounded(path: &Path, max_bytes: u64) -> Result<String, AppError> {
    let metadata = fs::metadata(path).await.map_err(AppError::Io)?;
    if metadata.len() > max_bytes {
        return Err(AppError::BadRequest(format!("File too large: {} bytes (max: {})", metadata.len(), max_bytes)));
    }
    fs::read_to_string(path).await.map_err(AppError::Io)
}

/// ### 🧪 Logic: Semantic Skill Extraction (parse_skill_md)
pub fn parse_skill_md(content: &str) -> Option<SkillDefinition> {
    if !content.starts_with("---") {
        return None;
    }

    let parts: Vec<&str> = content.split("---").collect();
    if parts.len() < 3 {
        return None;
    }

    let yaml_str = parts[1];
    let body = parts[2..].join("---");

    let metadata: serde_json::Value = serde_yaml::from_str(yaml_str).ok()?;
    let name = metadata
        .get("name")
        .and_then(|v| v.as_str())
        .or_else(|| metadata.get("title").and_then(|v| v.as_str()))
        .or(None)?;
        
    let description = metadata
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    Some(SkillDefinition {
        id: None,
        name: name.to_string(),
        description,
        execution_command: metadata
            .get("command")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        schema: metadata
            .get("schema")
            .cloned()
            .unwrap_or(json!({ "type": "object", "properties": {} })),
        oversight_required: metadata
            .get("oversight")
            .and_then(|v| v.as_bool())
            .unwrap_or(true),
        doc_url: metadata
            .get("doc_url")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        tags: metadata.get("tags").and_then(|v| v.as_array()).map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        }),
        full_instructions: Some(body.trim().to_string()),
        negative_constraints: None,
        verification_script: None,
        category: "ai".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_skill_md_basic() {
        let content = r#"---
name: test_skill
description: A test skill
command: python test.py
oversight: false
tags: ["test", "verify"]
---
This is the body content."#;

        let skill = parse_skill_md(content).expect("Should parse valid markdown");

        assert_eq!(skill.name, "test_skill");
        assert_eq!(skill.description, "A test skill");
        assert_eq!(skill.execution_command, "python test.py");
        assert!(!skill.oversight_required);
        assert_eq!(
            skill.tags.unwrap(),
            vec!["test".to_string(), "verify".to_string()]
        );
        assert_eq!(
            skill.full_instructions.unwrap(),
            "This is the body content."
        );
    }
}

// Metadata: [script_skills]

// Metadata: [script_skills]

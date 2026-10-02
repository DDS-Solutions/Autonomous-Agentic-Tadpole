/*
@docs ARCHITECTURE:SovereignKernel

### AI Assist Note
**🛡️ Tadpole OS: Loader**
Core system module providing specialized functionality for the agent swarm.

### 🔍 Debugging & Observability
- **Failure Path**: Unexpected execution drift or type compatibility issues.
- **Telemetry Link**: Traced via active system logging channels.
*/

//! Directory traversal, bounded I/O, mtime-caching, and tier-aware capability loaders.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use parking_lot::RwLock;
use tokio::fs;
use crate::error::AppError;
use super::model::{
    Capability, HookDefinition, SkillDefinition, WorkflowDefinition,
    MAX_HOOK_BYTES, MAX_SCRIPT_DOC_BYTES, MAX_SKILL_BYTES, MAX_WORKFLOW_BYTES,
};
use super::tier::Tier;
use super::parse::{
    extract_script_docstring, parse_skill_md, parse_workflow_content,
};

/// High-performance metadata cache for zero-downtime hot reloads.
/// Avoids re-reading and re-parsing multi-megabyte JSON and Markdown files
/// on every 10-second tick when file contents are unchanged.
#[derive(Debug, Default)]
pub struct ReloadCache {
    skills: RwLock<HashMap<PathBuf, (SystemTime, u64, SkillDefinition)>>,
    workflows: RwLock<HashMap<PathBuf, (SystemTime, u64, WorkflowDefinition)>>,
    hooks: RwLock<HashMap<PathBuf, (SystemTime, u64, HookDefinition)>>,
}

/// Helper to read file with size bounds and symlink verification to prevent
/// resource exhaustion and TOCTOU directory escapes.
pub async fn read_file_bounded(path: &Path, max_bytes: u64) -> Result<String, AppError> {
    if let Ok(meta) = fs::symlink_metadata(path).await {
        if meta.file_type().is_symlink() {
            return Err(AppError::Forbidden(format!("Symlinks are forbidden: {:?}", path)));
        }
    }
    let file = fs::File::open(path).await.map_err(AppError::Io)?;
    let mut reader = tokio::io::AsyncReadExt::take(file, max_bytes + 1);
    let mut buffer = Vec::new();
    tokio::io::AsyncReadExt::read_to_end(&mut reader, &mut buffer).await.map_err(AppError::Io)?;

    if buffer.len() as u64 > max_bytes {
        return Err(AppError::BadRequest(format!(
            "File {:?} exceeds maximum allowed size of {} bytes",
            path, max_bytes
        )));
    }

    String::from_utf8(buffer)
        .map_err(|e| AppError::BadRequest(format!("File {:?} is not valid UTF-8: {}", path, e)))
}

impl ReloadCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Loads skills from a directory with mtime caching and tier-enforced oversight.
    pub async fn load_skills_from_dir(
        &self,
        dir: &Path,
        tier: Tier,
    ) -> Result<Vec<(String, SkillDefinition)>, AppError> {
        let mut results = Vec::new();
        if !fs::try_exists(dir).await.unwrap_or(false) {
            return Ok(results);
        }

        let mut entries = match fs::read_dir(dir).await {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!("⚠️ [ScriptSkills] Failed to read directory {:?}: {}", dir, e);
                return Ok(results);
            }
        };

        let mut entry_list = Vec::new();
        while let Ok(Some(entry)) = entries.next_entry().await {
            if let Ok(ft) = entry.file_type().await {
                if ft.is_symlink() {
                    tracing::warn!("⚠️ [ScriptSkills] Skipping symlink at {:?}", entry.path());
                    continue;
                }
            }
            entry_list.push(entry);
        }
        entry_list.sort_by_key(|e| e.path());

        let mut pending_scripts = Vec::new();

        for entry in entry_list {
            let path = entry.path();
            let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();

            if ext == "json" {
                let meta = match fs::metadata(&path).await {
                    Ok(m) => m,
                    Err(_) => continue,
                };
                let mtime = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                let size = meta.len();

                // Check cache first
                let cached = {
                    let cache = self.skills.read();
                    cache.get(&path).and_then(|(c_mtime, c_size, item)| {
                        if *c_mtime == mtime && *c_size == size {
                            Some(item.clone())
                        } else {
                            None
                        }
                    })
                };

                let mut skill = if let Some(cached_skill) = cached {
                    cached_skill
                } else {
                    match read_file_bounded(&path, MAX_SKILL_BYTES).await {
                        Ok(content) => match serde_json::from_str::<SkillDefinition>(&content) {
                            Ok(parsed) => {
                                self.skills.write().insert(path.clone(), (mtime, size, parsed.clone()));
                                parsed
                            }
                            Err(e) => {
                                tracing::warn!("⚠️ [ScriptSkills] Failed to parse skill JSON at {:?}: {}", path, e);
                                continue;
                            }
                        },
                        Err(e) => {
                            tracing::warn!("⚠️ [ScriptSkills] Failed to read skill file at {:?}: {}", path, e);
                            continue;
                        }
                    }
                };

                // Security gate: validate commands
                if let Err(e) = crate::utils::security::validate_shell_command(&skill.execution_command) {
                    tracing::warn!("🚫 [ScriptSkills] Rejecting skill '{}' ({:?}) — invalid execution command: {}", skill.name, path, e);
                    continue;
                }
                if let Some(ref verify_cmd) = skill.verification_script {
                    if let Err(e) = crate::utils::security::validate_shell_command(verify_cmd) {
                        tracing::warn!("🚫 [ScriptSkills] Rejecting skill '{}' ({:?}) — invalid verification command: {}", skill.name, path, e);
                        continue;
                    }
                }

                // Tier enforcement: Agent tier MUST have oversight enforced unconditionally
                if tier.enforce_oversight() {
                    skill.oversight_required = true;
                }
                skill.category = tier.category().to_string();
                results.push((skill.name.clone(), skill));
            } else if ext == "py" || ext == "sh" || ext == "ps1" {
                let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
                if !stem.is_empty() && !stem.starts_with("__") {
                    if !pending_scripts.iter().any(|(s, _, _)| s == &stem) {
                        pending_scripts.push((stem, ext.to_string(), path));
                    }
                }
            }
        }

        // Auto-synthesize skill definitions for scripts without explicit .json manifests
        let is_agent_dir = tier == Tier::Agent;
        for (stem, ext, path) in pending_scripts {
            if results.iter().any(|(n, _)| n == &stem) {
                continue;
            }

            let mut description = format!("Deterministic execution script '{}'", stem);
            if let Ok(content) = read_file_bounded(&path, MAX_SCRIPT_DOC_BYTES).await {
                if let Some(doc) = extract_script_docstring(&content) {
                    description = doc;
                }
            }

            let rel_path = if is_agent_dir {
                format!("execution/agent_generated/skills/{}.{}", stem, ext)
            } else {
                format!("execution/{}.{}", stem, ext)
            };

            let execution_command = match ext.as_str() {
                "sh" => format!("bash {}", rel_path),
                "ps1" => format!("powershell {}", rel_path),
                _ => format!("python {}", rel_path),
            };

            if let Err(e) = crate::utils::security::validate_shell_command(&execution_command) {
                tracing::warn!("🚫 [ScriptSkills] Rejecting synthesized skill '{}' ({:?}) — command validation failed: {}", stem, path, e);
                continue;
            }

            let skill = SkillDefinition {
                id: None,
                name: stem.clone(),
                description,
                execution_command,
                schema: serde_json::json!({
                    "type": "object",
                    "properties": {}
                }),
                oversight_required: true,
                doc_url: None,
                tags: Some(vec!["execution".to_string(), "deterministic".to_string()]),
                full_instructions: None,
                negative_constraints: None,
                verification_script: None,
                category: tier.category().to_string(),
            };

            results.push((stem, skill));
        }

        Ok(results)
    }

    /// Loads built-in skills from `.agent/skills/` with recursive `SKILL.md` detection.
    pub async fn load_built_in_skills(&self, agent_skills_dir: &Path) -> Result<Vec<(String, SkillDefinition)>, AppError> {
        let mut results = Vec::new();
        if !fs::try_exists(agent_skills_dir).await.unwrap_or(false) {
            return Ok(results);
        }

        let mut entries = match fs::read_dir(agent_skills_dir).await {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!("⚠️ [ScriptSkills] Failed to read built-in skills dir {:?}: {}", agent_skills_dir, e);
                return Ok(results);
            }
        };

        let mut candidate_paths = Vec::new();
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if let Ok(ft) = entry.file_type().await {
                if ft.is_symlink() {
                    continue;
                }
                if ft.is_dir() {
                    let sub_skill = path.join("SKILL.md");
                    if fs::try_exists(&sub_skill).await.unwrap_or(false) {
                        candidate_paths.push(sub_skill);
                    }
                } else if path.file_name().and_then(|s| s.to_str()) == Some("SKILL.md") {
                    candidate_paths.push(path);
                }
            }
        }
        candidate_paths.sort();

        for path in candidate_paths {
            let meta = match fs::metadata(&path).await {
                Ok(m) => m,
                Err(_) => continue,
            };
            let mtime = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            let size = meta.len();

            let cached = {
                let cache = self.skills.read();
                cache.get(&path).and_then(|(c_mtime, c_size, item)| {
                    if *c_mtime == mtime && *c_size == size {
                        Some(item.clone())
                    } else {
                        None
                    }
                })
            };

            let mut skill = if let Some(s) = cached {
                s
            } else if let Ok(content) = read_file_bounded(&path, MAX_SKILL_BYTES).await {
                if let Some(parsed) = parse_skill_md(&content) {
                    self.skills.write().insert(path.clone(), (mtime, size, parsed.clone()));
                    parsed
                } else {
                    continue;
                }
            } else {
                continue;
            };

            if let Err(e) = crate::utils::security::validate_shell_command(&skill.execution_command) {
                tracing::warn!("🚫 [ScriptSkills] Rejecting built-in skill '{}' — invalid command: {}", skill.name, e);
                continue;
            }

            skill.category = "built_in".to_string();
            results.push((skill.name.clone(), skill));
        }

        Ok(results)
    }

    /// Loads workflows from a directory. Stores ONLY the canonical workflow name
    /// to prevent alias duplication and bloated counts.
    pub async fn load_workflows_from_dir(
        &self,
        dir: &Path,
        tier: Tier,
    ) -> Result<Vec<(String, WorkflowDefinition)>, AppError> {
        let mut results = Vec::new();
        if !fs::try_exists(dir).await.unwrap_or(false) {
            return Ok(results);
        }

        let mut entries = match fs::read_dir(dir).await {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!("⚠️ [ScriptSkills] Failed to read workflow dir {:?}: {}", dir, e);
                return Ok(results);
            }
        };

        let mut entry_list = Vec::new();
        while let Ok(Some(entry)) = entries.next_entry().await {
            if let Ok(ft) = entry.file_type().await {
                if ft.is_symlink() {
                    continue;
                }
            }
            entry_list.push(entry);
        }
        entry_list.sort_by_key(|e| e.path());

        for entry in entry_list {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("md") {
                let meta = match fs::metadata(&path).await {
                    Ok(m) => m,
                    Err(_) => continue,
                };
                let mtime = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                let size = meta.len();

                let cached = {
                    let cache = self.workflows.read();
                    cache.get(&path).and_then(|(c_mtime, c_size, item)| {
                        if *c_mtime == mtime && *c_size == size {
                            Some(item.clone())
                        } else {
                            None
                        }
                    })
                };

                let wf_def = if let Some(wf) = cached {
                    wf
                } else if let Ok(raw_content) = read_file_bounded(&path, MAX_WORKFLOW_BYTES).await {
                    let file_stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                    if file_stem.is_empty() {
                        continue;
                    }
                    let (name_opt, body) = parse_workflow_content(&raw_content);
                    let name = name_opt.unwrap_or(file_stem);
                    let wf = WorkflowDefinition {
                        id: None,
                        name,
                        content: body,
                        doc_url: None,
                        tags: None,
                        category: tier.category().to_string(),
                    };
                    self.workflows.write().insert(path.clone(), (mtime, size, wf.clone()));
                    wf
                } else {
                    continue;
                };

                results.push((wf_def.name.clone(), wf_def));
            }
        }

        Ok(results)
    }

    /// Loads hooks from a directory with mtime caching and shell validation.
    pub async fn load_hooks_from_dir(
        &self,
        dir: &Path,
        tier: Tier,
    ) -> Result<Vec<(String, HookDefinition)>, AppError> {
        let mut results = Vec::new();
        if !fs::try_exists(dir).await.unwrap_or(false) {
            return Ok(results);
        }

        let mut entries = match fs::read_dir(dir).await {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!("⚠️ [ScriptSkills] Failed to read hooks dir {:?}: {}", dir, e);
                return Ok(results);
            }
        };

        let mut entry_list = Vec::new();
        while let Ok(Some(entry)) = entries.next_entry().await {
            if let Ok(ft) = entry.file_type().await {
                if ft.is_symlink() {
                    continue;
                }
            }
            entry_list.push(entry);
        }
        entry_list.sort_by_key(|e| e.path());

        for entry in entry_list {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                let meta = match fs::metadata(&path).await {
                    Ok(m) => m,
                    Err(_) => continue,
                };
                let mtime = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                let size = meta.len();

                let cached = {
                    let cache = self.hooks.read();
                    cache.get(&path).and_then(|(c_mtime, c_size, item)| {
                        if *c_mtime == mtime && *c_size == size {
                            Some(item.clone())
                        } else {
                            None
                        }
                    })
                };

                let mut hook = if let Some(h) = cached {
                    h
                } else if let Ok(content) = read_file_bounded(&path, MAX_HOOK_BYTES).await {
                    match serde_json::from_str::<HookDefinition>(&content) {
                        Ok(parsed) => {
                            self.hooks.write().insert(path.clone(), (mtime, size, parsed.clone()));
                            parsed
                        }
                        Err(e) => {
                            tracing::warn!("⚠️ [ScriptSkills] Failed to parse hook JSON at {:?}: {}", path, e);
                            continue;
                        }
                    }
                } else {
                    continue;
                };

                if let Err(e) = hook.validate() {
                    tracing::warn!("🚫 [ScriptSkills] Rejecting hook '{}' — validation failed: {}", hook.name, e);
                    continue;
                }

                hook.category = tier.category().to_string();
                results.push((hook.name.clone(), hook));
            }
        }

        Ok(results)
    }
}

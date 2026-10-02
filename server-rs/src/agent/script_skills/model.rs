/*
@docs ARCHITECTURE:SovereignKernel

### AI Assist Note
**🛡️ Tadpole OS: Model**
Core system module providing specialized functionality for the agent swarm.

### 🔍 Debugging & Observability
- **Failure Path**: Unexpected execution drift or type compatibility issues.
- **Telemetry Link**: Traced via active system logging channels.
*/

//! Capability models and traits for ScriptSkillsRegistry.

use serde::{Deserialize, Serialize};
use crate::error::AppError;

pub const MAX_SKILL_BYTES: u64 = 5_000_000;
pub const MAX_WORKFLOW_BYTES: u64 = 2_000_000;
pub const MAX_HOOK_BYTES: u64 = 500_000;
pub const MAX_SCRIPT_DOC_BYTES: u64 = 4096;
pub const MAX_CAPABILITY_NAME_LEN: usize = 128;

pub fn default_category() -> String {
    "user".to_string()
}

pub fn default_oversight() -> bool {
    true
}

pub fn default_active() -> bool {
    true
}

/// Represents a dynamic skill loaded from `data/skills/*.json` or scripts
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

/// Represents a hook definition loaded from `data/hooks/*.json`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookDefinition {
    pub name: String,
    pub description: String,
    pub hook_type: String, // e.g., "pre_validation", "post_analysis"
    pub content: String,
    #[serde(default = "default_active")]
    pub active: bool,
    #[serde(default = "default_category")]
    pub category: String,
}

/// Common trait unifying serialization, validation, and metadata for registry capabilities.
pub trait Capability: Serialize + serde::de::DeserializeOwned + Clone + Send + Sync + 'static {
    const EXT: &'static str;
    const MAX_BYTES: u64;

    fn name(&self) -> &str;
    fn category(&self) -> &str;
    fn set_category(&mut self, category: String);
    fn set_oversight(&mut self, required: bool);
    fn validate(&self) -> Result<(), AppError>;
    fn serialize_content(&self) -> Result<Vec<u8>, AppError>;
    fn deserialize_content(content: &[u8], path_stem: &str) -> Result<Self, AppError>;
    fn extract_name(content: &[u8], path_stem: &str) -> Option<String>;
}

impl Capability for SkillDefinition {
    const EXT: &'static str = "json";
    const MAX_BYTES: u64 = MAX_SKILL_BYTES;

    fn name(&self) -> &str {
        &self.name
    }

    fn category(&self) -> &str {
        &self.category
    }

    fn set_category(&mut self, category: String) {
        self.category = category;
    }

    fn set_oversight(&mut self, required: bool) {
        self.oversight_required = required;
    }

    fn validate(&self) -> Result<(), AppError> {
        let trimmed = self.name.trim();
        if trimmed.is_empty() {
            return Err(AppError::BadRequest("Skill name cannot be empty".to_string()));
        }
        if self.name.len() > MAX_CAPABILITY_NAME_LEN {
            return Err(AppError::BadRequest(format!("Skill name exceeds {} characters", MAX_CAPABILITY_NAME_LEN)));
        }
        crate::utils::security::validate_shell_command(&self.execution_command)
            .map_err(|e| AppError::BadRequest(format!("Invalid execution command: {}", e)))?;
        if let Some(ref verify_cmd) = self.verification_script {
            crate::utils::security::validate_shell_command(verify_cmd)
                .map_err(|e| AppError::BadRequest(format!("Invalid verification command: {}", e)))?;
        }
        Ok(())
    }

    fn serialize_content(&self) -> Result<Vec<u8>, AppError> {
        serde_json::to_vec_pretty(self)
            .map_err(|e| AppError::InternalServerError(format!("Failed to serialize skill: {}", e)))
    }

    fn deserialize_content(content: &[u8], _path_stem: &str) -> Result<Self, AppError> {
        serde_json::from_slice(content)
            .map_err(|e| AppError::BadRequest(format!("Failed to parse skill JSON: {}", e)))
    }

    fn extract_name(content: &[u8], _path_stem: &str) -> Option<String> {
        serde_json::from_slice::<serde_json::Value>(content).ok()
            .and_then(|val| val.get("name").and_then(|n| n.as_str()).map(|s| s.to_string()))
    }
}

impl Capability for WorkflowDefinition {
    const EXT: &'static str = "md";
    const MAX_BYTES: u64 = MAX_WORKFLOW_BYTES;

    fn name(&self) -> &str {
        &self.name
    }

    fn category(&self) -> &str {
        &self.category
    }

    fn set_category(&mut self, category: String) {
        self.category = category;
    }

    fn set_oversight(&mut self, _required: bool) {}

    fn validate(&self) -> Result<(), AppError> {
        let trimmed = self.name.trim();
        if trimmed.is_empty() {
            return Err(AppError::BadRequest("Workflow name cannot be empty".to_string()));
        }
        if self.name.len() > MAX_CAPABILITY_NAME_LEN {
            return Err(AppError::BadRequest(format!("Workflow name exceeds {} characters", MAX_CAPABILITY_NAME_LEN)));
        }
        if self.content.len() > Self::MAX_BYTES as usize {
            return Err(AppError::BadRequest(format!("Workflow content exceeds {} bytes", Self::MAX_BYTES)));
        }
        Ok(())
    }

    fn serialize_content(&self) -> Result<Vec<u8>, AppError> {
        let content_str = match std::str::from_utf8(self.content.as_bytes()) {
            Ok(s) => s,
            Err(_) => return Err(AppError::BadRequest("Workflow content is not valid UTF-8".to_string())),
        };
        let (extracted_name, _) = super::parse::parse_workflow_content(content_str);
        let to_write = if extracted_name.is_none() {
            format!("---\nname: \"{}\"\n---\n{}", self.name.replace('"', "\\\""), self.content)
        } else {
            self.content.clone()
        };
        Ok(to_write.into_bytes())
    }

    fn deserialize_content(content: &[u8], path_stem: &str) -> Result<Self, AppError> {
        let raw = std::str::from_utf8(content)
            .map_err(|e| AppError::BadRequest(format!("Invalid UTF-8 in workflow: {}", e)))?;
        let (name_opt, body) = super::parse::parse_workflow_content(raw);
        let name = name_opt.unwrap_or_else(|| path_stem.to_string());
        Ok(WorkflowDefinition {
            id: None,
            name,
            content: body,
            doc_url: None,
            tags: None,
            category: "user".to_string(),
        })
    }

    fn extract_name(content: &[u8], path_stem: &str) -> Option<String> {
        let raw = std::str::from_utf8(content).ok()?;
        let (name_opt, _) = super::parse::parse_workflow_content(raw);
        name_opt.or_else(|| if path_stem.is_empty() { None } else { Some(path_stem.to_string()) })
    }
}

impl Capability for HookDefinition {
    const EXT: &'static str = "json";
    const MAX_BYTES: u64 = MAX_HOOK_BYTES;

    fn name(&self) -> &str {
        &self.name
    }

    fn category(&self) -> &str {
        &self.category
    }

    fn set_category(&mut self, category: String) {
        self.category = category;
    }

    fn set_oversight(&mut self, _required: bool) {}

    fn validate(&self) -> Result<(), AppError> {
        let trimmed = self.name.trim();
        if trimmed.is_empty() {
            return Err(AppError::BadRequest("Hook name cannot be empty".to_string()));
        }
        if self.name.len() > MAX_CAPABILITY_NAME_LEN {
            return Err(AppError::BadRequest(format!("Hook name exceeds {} characters", MAX_CAPABILITY_NAME_LEN)));
        }
        if self.hook_type.trim().is_empty() {
            return Err(AppError::BadRequest("Hook type cannot be empty".to_string()));
        }
        if self.content.len() > Self::MAX_BYTES as usize {
            return Err(AppError::BadRequest(format!("Hook content exceeds {} bytes", Self::MAX_BYTES)));
        }
        Ok(())
    }

    fn serialize_content(&self) -> Result<Vec<u8>, AppError> {
        serde_json::to_vec_pretty(self)
            .map_err(|e| AppError::InternalServerError(format!("Failed to serialize hook: {}", e)))
    }

    fn deserialize_content(content: &[u8], _path_stem: &str) -> Result<Self, AppError> {
        serde_json::from_slice(content)
            .map_err(|e| AppError::BadRequest(format!("Failed to parse hook JSON: {}", e)))
    }

    fn extract_name(content: &[u8], _path_stem: &str) -> Option<String> {
        serde_json::from_slice::<serde_json::Value>(content).ok()
            .and_then(|val| val.get("name").and_then(|n| n.as_str()).map(|s| s.to_string()))
    }
}

//! @docs ARCHITECTURE:Agent
//!
//! ### AI Assist Note
//! **Dynamic Capabilities**: Orchestrates the discovery and execution of
//! **Custom Skills** (JSON) and **Deterministic Workflows** (Markdown).
//! Features a multi-tiered architecture (User < Agent < BuiltIn) supporting
//! **Frontmatter Extraction** (YAML), **Schema Validation**, and **Sovereign Oversight**.
//!
//! ### 🛡️ Nexus Synthesis Hardening
//! This implementation follows the **Zero-Downtime Reload** pattern via atomic
//! state snapshots and metadata-based mtime caching. I/O operations are validated
//! against symlinks, persistence is made atomic via temp-file rotation, and
//! mutations are strictly serialized without deep-cloning collections.
//!
//! ### 🔍 Debugging & Observability
//! - **Telemetry Link**: Search `[script_skills]` in tracing logs.
//! - **Failure Path**: Invalid YAML frontmatter in `SKILL.md`, duplicate
//!   skill names, symlink traversal attempts, or `WORKSPACE_ROOT` resolution failure.
//! - **Trace Scope**: `server-rs::agent::script_skills`

pub mod model;
pub mod tier;
pub mod parse;
pub mod state;
pub mod store;
pub mod loader;
pub mod registry;
pub mod hot_reload;
#[cfg(test)]
pub mod tests;

// Re-exports for complete public interface backward-compatibility
pub use model::{
    Capability, HookDefinition, SkillDefinition, WorkflowDefinition,
    MAX_CAPABILITY_NAME_LEN, MAX_HOOK_BYTES, MAX_SCRIPT_DOC_BYTES,
    MAX_SKILL_BYTES, MAX_WORKFLOW_BYTES,
};
pub use tier::Tier;
pub use parse::{
    collision_safe_skill_filename, extract_script_docstring, extract_workflow_name,
    fnv1a64, normalize_alias, parse_skill_md, parse_workflow_content,
    split_frontmatter, validate_capability_name,
};
pub use state::RegistryState;
pub use store::{atomic_write, cleanup_stale_temp_files, persist_capability, verified_remove};
pub use loader::{read_file_bounded, ReloadCache};
pub use registry::ScriptSkillsRegistry;
pub use hot_reload::spawn_hot_reload_loop;

// Metadata: [script_skills]

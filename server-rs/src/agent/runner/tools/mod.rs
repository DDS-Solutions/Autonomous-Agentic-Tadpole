//! @docs ARCHITECTURE:Runner
//!
//! ### AI Assist Note
//! **Tool Dispatcher Facade**: Entry module for the Zero-Trust tool execution subsystem,
//! organizing security pipeline orchestration (`pipeline`), dynamic script skills
//! (`dynamic_skills`), sandboxed shell & script builder (`shell`), working memory and
//! blackboard facets (`blackboard_tools`), and pure helper utilities.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Runtime logic error, security gate rejection, or unknown tool lookup.
//! - **Telemetry Link**: Search `[mod]` or `ToolExecution` in tracing logs.

pub mod blackboard_tools;
pub mod cache;
pub mod capability;
pub mod dispatcher;
pub mod dynamic_skills;
pub mod error;
pub mod manifest;
pub mod pipeline;
pub mod registry;
pub mod security;
pub mod shell;
pub mod trait_tool;

#[cfg(test)]
mod tests;

pub use cache::SharedToolCache;
pub use capability::{CapabilityToken, ZeroTrustGuard};
pub use crate::agent::types::ToolContext;
pub use trait_tool::Tool;

/// Normalizes raw tool names passed by LLMs by stripping path prefixes and script extensions.
pub fn normalize_tool_name(raw: &str) -> String {
    static PREFIXES: &[&str] = &[
        "./",
        "execution/",
        "execution\\",
        "agent_generated/skills/",
        "agent_generated\\skills\\",
        "skills/",
        "skills\\",
        "directives/",
        "directives\\",
    ];
    static SUFFIXES: &[&str] = &[".py", ".sh", ".ps1", ".bat", ".exe"];

    let mut s = raw.trim();
    for prefix in PREFIXES {
        if let Some(rest) = s.strip_prefix(prefix) {
            s = rest;
        }
    }
    for suffix in SUFFIXES {
        if let Some(rest) = s.strip_suffix(suffix) {
            s = rest;
        }
    }
    s.to_string()
}

/// Extracts the primary target file path from heterogeneous tool call JSON arguments.
pub fn extract_target_path(args: &serde_json::Value) -> Option<&str> {
    args.get("path")
        .or_else(|| args.get("file_path"))
        .or_else(|| args.get("filename"))
        .or_else(|| args.get("file_name"))
        .or_else(|| args.get("file"))
        .and_then(|v| v.as_str())
}

/// Recursively merges a patch JSON object into a target JSON value.
pub fn recursive_merge_json(target: &mut serde_json::Value, patch: &serde_json::Value) {
    match (target, patch) {
        (serde_json::Value::Object(target_obj), serde_json::Value::Object(patch_obj)) => {
            for (key, val) in patch_obj {
                if let Some(existing) = target_obj.get_mut(key) {
                    recursive_merge_json(existing, val);
                } else {
                    target_obj.insert(key.clone(), val.clone());
                }
            }
        }
        (target_val, patch_val) => {
            *target_val = patch_val.clone();
        }
    }
}

// Metadata: [mod]

/*
@docs ARCHITECTURE:SovereignKernel

### AI Assist Note
**🛡️ Tadpole OS: Parse**
Core system module providing specialized functionality for the agent swarm.

### 🔍 Debugging & Observability
- **Failure Path**: Unexpected execution drift or type compatibility issues.
- **Telemetry Link**: Traced via active system logging channels.
*/

//! Parsing utilities for capability frontmatter, script docstrings, and filenames.

use serde_json::json;
use crate::error::AppError;
use super::model::{SkillDefinition, MAX_CAPABILITY_NAME_LEN};

/// Computes a 64-bit FNV-1a hash of a string slice.
pub fn fnv1a64(s: &str) -> u64 {
    s.bytes().fold(0xcbf29ce484222325u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    })
}

/// Maps a capability name to a safe filename using a uniform hash-suffixed encoding scheme.
/// This prevents deterministic collisions between unescaped and formatted names.
pub fn collision_safe_skill_filename(name: &str, extension: &str) -> String {
    let safe_name = crate::utils::security::sanitize_id(name);
    let base = if safe_name.is_empty() { "capability" } else { safe_name.as_str() };
    let hash = fnv1a64(name);
    format!("{base}-{hash:016x}.{extension}")
}

/// Normalizes a capability name for lookup-time alias resolution.
pub fn normalize_alias(name: &str) -> String {
    name.trim().to_lowercase().replace([' ', '-'], "_")
}

/// Validates that a capability name is non-empty, within bounds, and contains no null bytes.
pub fn validate_capability_name(name: &str) -> Result<(), AppError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(AppError::BadRequest("Capability name cannot be empty".to_string()));
    }
    if name.len() > MAX_CAPABILITY_NAME_LEN {
        return Err(AppError::BadRequest(format!(
            "Capability name exceeds {} characters",
            MAX_CAPABILITY_NAME_LEN
        )));
    }
    if name.contains('\0') {
        return Err(AppError::BadRequest("Capability name cannot contain null bytes".to_string()));
    }
    Ok(())
}

/// Splits YAML frontmatter from markdown body.
/// Returns `Some((yaml_slice, body_slice))` if frontmatter markers are present.
pub fn split_frontmatter(content: &str) -> Option<(&str, &str)> {
    let content = content.strip_prefix('\u{feff}').unwrap_or(content);
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return None;
    }
    let rest = trimmed.strip_prefix("---")?;
    // Closing fence must be on its own line
    let end = rest.find("\n---")?;
    let yaml_str = &rest[..end];
    let after_closing = &rest[end + 4..];
    let body = after_closing.trim_start_matches('\r').trim_start_matches('\n');
    Some((yaml_str, body))
}

/// Parses a workflow markdown string, extracting the optional frontmatter name and the body content.
pub fn parse_workflow_content(raw: &str) -> (Option<String>, String) {
    if let Some((yaml_str, body)) = split_frontmatter(raw) {
        if let Ok(val) = serde_yaml::from_str::<serde_json::Value>(yaml_str) {
            let name = val.get("name").and_then(|v| v.as_str()).map(|s| s.trim().to_string());
            return (name, body.to_string());
        }
    }
    (None, raw.to_string())
}

/// Extracts a workflow's canonical name from YAML frontmatter if present, falling back to file stem.
pub fn extract_workflow_name(content: &str, file_stem: &str) -> String {
    let (name_opt, _) = parse_workflow_content(content);
    name_opt.unwrap_or_else(|| file_stem.to_string())
}

/// Semantic skill extraction from markdown with YAML frontmatter.
pub fn parse_skill_md(content: &str) -> Option<SkillDefinition> {
    let (yaml_str, body) = split_frontmatter(content)?;

    let metadata: serde_json::Value = serde_yaml::from_str(yaml_str).ok()?;
    let name = metadata
        .get("name")
        .and_then(|v| v.as_str())
        .or_else(|| metadata.get("title").and_then(|v| v.as_str()))?;
    let trimmed_name = name.trim();
    if trimmed_name.is_empty() {
        return None;
    }

    let command = metadata
        .get("command")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if command.trim().is_empty() {
        return None;
    }

    let description = metadata
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let verification_script = metadata
        .get("verification_script")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let negative_constraints = metadata.get("negative_constraints").and_then(|v| v.as_array()).map(|arr| {
        arr.iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect()
    });

    Some(SkillDefinition {
        id: None,
        name: trimmed_name.to_string(),
        description,
        execution_command: command,
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
        negative_constraints,
        verification_script,
        category: "built_in".to_string(),
    })
}

/// Extracts the top-level docstring or initial comment summary from a script file.
pub fn extract_script_docstring(content: &str) -> Option<String> {
    let clean_content = content.strip_prefix('\u{feff}').unwrap_or(content);
    let mut lines = clean_content.lines().peekable();

    // Skip shebang if present
    if let Some(first) = lines.peek() {
        if first.starts_with("#!") {
            lines.next();
        }
    }

    // Skip initial empty lines
    while let Some(line) = lines.peek() {
        if line.trim().is_empty() {
            lines.next();
        } else {
            break;
        }
    }

    // Check for Python triple-quote docstrings
    let remaining: String = lines.clone().collect::<Vec<_>>().join("\n");
    let trimmed = remaining.trim_start();
    if trimmed.starts_with("\"\"\"") || trimmed.starts_with("'''") {
        let quote = &trimmed[0..3];
        if let Some(end_idx) = trimmed[3..].find(quote) {
            let doc = trimmed[3..3 + end_idx].trim();
            if !doc.is_empty() {
                let first_para = doc.split("\n\n").next().unwrap_or(doc);
                return Some(first_para.lines().map(|l| l.trim()).collect::<Vec<_>>().join(" "));
            }
        }
    }

    // Fall back to comment blocks (# or //)
    let mut comment_lines = Vec::new();
    for line in lines {
        let line_trimmed = line.trim();
        if line_trimmed.starts_with('#') {
            let comment = line_trimmed.trim_start_matches('#').trim();
            if !comment.is_empty() && !comment.starts_with("---") {
                comment_lines.push(comment.to_string());
            }
        } else if line_trimmed.starts_with("//") {
            let comment = line_trimmed.trim_start_matches('/').trim();
            if !comment.is_empty() {
                comment_lines.push(comment.to_string());
            }
        } else if line_trimmed.is_empty() && comment_lines.is_empty() {
            continue;
        } else {
            break;
        }
    }

    if !comment_lines.is_empty() {
        Some(comment_lines.join(" "))
    } else {
        None
    }
}

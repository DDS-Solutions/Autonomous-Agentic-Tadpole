//! @docs ARCHITECTURE:Core
//!
//! ### AI Assist Note
//! **path**: Core technical resource for the Tadpole OS infrastructure.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Unhandled errors, lock contention, or connection staling.
//! - **Telemetry Link**: Search `[path]` in tracing logs.
//! - **Trace Scope**: `server-rs::intelligence::graph::path`
//! 
//! Path utilities: Unix-style conversion, log sanitization, and obfuscation.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Salt length validation failure, invalid path structure.
//! - **Telemetry Link**: Search `[graph]` in tracing logs.

use std::path::Path;
use sha2::{Digest, Sha256};
use crate::intelligence::graph::error::GraphError;

/// Converts a `Path` to a forward-slash string representation.
pub(crate) fn to_unix_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Returns a sanitized, workspace-relative representation of `path` for log output.
/// If `path` is outside `root`, returns `<redacted>/<filename>` to prevent leaking
/// absolute filesystem paths into structured logs.
pub(crate) fn sanitize_log_path(path: &Path, root: &Path) -> String {
    if let Ok(rel) = path.strip_prefix(root) {
        rel.to_string_lossy().replace('\\', "/")
    } else {
        let filename = path
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or("unknown_file");
        format!("<redacted>/{}", filename)
    }
}

/// Obfuscates physical file path structures deterministically while preserving
/// UX force-graph clustering and file basenames.
///
/// # Errors
/// Returns [`GraphError::KeyNormalization`] if `salt` is shorter than 4 bytes.
/// Returns [`GraphError::PathOutOfBounds`] if `path_str` has no valid filename or parent.
pub fn obfuscate_path(path_str: &str, salt: &str) -> Result<String, GraphError> {
    if salt.len() < 4 {
        return Err(GraphError::KeyNormalization(format!(
            "Salt is too short: got {} bytes, minimum is 4 bytes",
            salt.len()
        )));
    }
    let path = Path::new(path_str);
    let file_name = path
        .file_name()
        .and_then(|f| f.to_str())
        .ok_or_else(|| GraphError::PathOutOfBounds(format!("Invalid file path: {}", path_str)))?;
    let parent = path
        .parent()
        .ok_or_else(|| GraphError::PathOutOfBounds(format!("Path has no parent structure: {}", path_str)))?;
    let parent_str = parent.to_string_lossy();

    if parent_str.is_empty() {
        Ok(file_name.to_string())
    } else {
        let mut hasher = Sha256::new();
        hasher.update(salt.as_bytes());
        hasher.update(parent_str.as_bytes());
        let result = hasher.finalize();
        let hash_val = hex::encode(result);
        let obf_prefix = hash_val.get(..16).unwrap_or(&hash_val);
        Ok(format!("{}/{}", obf_prefix, file_name))
    }
}

// Metadata: [path]

//! @docs ARCHITECTURE:Core
//!
//! ### AI Assist Note
//! **key**: Core technical resource for the Tadpole OS infrastructure.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Unhandled errors, lock contention, or connection staling.
//! - **Telemetry Link**: Search `[key]` in tracing logs.
//! - **Trace Scope**: `server-rs::intelligence::graph::key`

//! Key normalization for the symbol index.
//!
//! The symbol index (`HashMap<String, NodeIndex>`) uses composite keys of the
//! form `"<rel_path>\0<symbol_name>"`. This module defines the normalisation
//! contract and its default implementation.

/// Trait defining normalisation behavior for symbol index keys.
pub trait KeyNormalizer: Send + Sync {
    fn normalize_key(&self, path: &str, name: &str) -> String;
}

/// Default normalization: converts backslashes to forward slashes and
/// replaces null bytes with `_` to prevent DoS key collisions.
pub struct DefaultKeyNormalizer;

impl KeyNormalizer for DefaultKeyNormalizer {
    fn normalize_key(&self, path: &str, name: &str) -> String {
        let clean_path = path.replace('\\', "/").replace('\0', "_");
        let clean_name = name.replace('\0', "_");
        format!("{clean_path}\0{clean_name}")
    }
}

/// Convenience function: returns a normalized composite index key.
pub fn index_key(path: &str, name: &str) -> String {
    DefaultKeyNormalizer.normalize_key(path, name)
}

// Metadata: [key]

//! @docs ARCHITECTURE:Core
//!
//! ### AI Assist Note
//! **config**: Core technical resource for the Tadpole OS infrastructure.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Unhandled errors, lock contention, or connection staling.
//! - **Telemetry Link**: Search `[config]` in tracing logs.
//! - **Trace Scope**: `server-rs::intelligence::graph::config`
//! 
//! Build-time configuration for the `CodeSymbolGraph`.
//!
//! `GraphConfig` controls which symbols and path components are excluded
//! from dead-code anomaly detection, preventing false positives for
//! well-known framework lifecycle hooks and generated/test code.

/// Configuration for anomaly detection filtering in the `CodeSymbolGraph`.
#[derive(Debug, Clone)]
pub struct GraphConfig {
    /// Symbol names that are globally excluded from anomaly detection
    /// (e.g., well-known proxy traps, lifecycle hooks, framework internals).
    pub excluded_symbols: Vec<String>,
    /// Path components that mark an entire subtree as excluded
    /// (e.g., `"server-rs"`, `"tests"`, `"generated"`).
    pub excluded_paths: Vec<String>,
    /// File extension or suffix patterns excluded from anomaly scans.
    pub excluded_file_patterns: Vec<String>,
    /// Path segments that indicate framework/entrypoint directories.
    pub excluded_path_segments: Vec<String>,
    /// Case-insensitive keywords for symbols that should not be flagged as dead code.
    pub ignored_symbol_keywords: Vec<String>,
}

impl Default for GraphConfig {
    fn default() -> Self {
        Self {
            excluded_symbols: vec![
                "get".to_string(), "set".to_string(), "has".to_string(),
                "deleteProperty".to_string(), "ownKeys".to_string(),
                "getOwnPropertyDescriptor".to_string(), "defineProperty".to_string(),
                "preventExtensions".to_string(), "isExtensible".to_string(),
                "getPrototypeOf".to_string(), "setPrototypeOf".to_string(),
                "apply".to_string(), "construct".to_string(),
                "constructor".to_string(), "toString".to_string(),
                "valueOf".to_string(), "toJSON".to_string(),
                "render".to_string(), "componentDidMount".to_string(),
                "componentDidUpdate".to_string(), "componentWillUnmount".to_string(),
                "shouldComponentUpdate".to_string(), "getDerivedStateFromProps".to_string(),
                "getDerivedStateFromError".to_string(), "componentDidCatch".to_string(),
                "Workspace_Status".to_string()
            ],
            excluded_paths: vec![
                "server-rs".to_string(),
                "src-tauri".to_string(),
                "wasm-codec".to_string(),
                "scratch".to_string(),
                "generated".to_string(),
                "contracts".to_string(),
                "test".to_string(),
                "tests".to_string(),
                "__tests__".to_string(),
            ],
            excluded_file_patterns: vec![
                ".d.ts".to_string(),
                "vite.config.ts".to_string(),
                "playwright.config.ts".to_string(),
                ".test.ts".to_string(),
                ".test.tsx".to_string(),
                ".spec.ts".to_string(),
                ".spec.tsx".to_string(),
                "App.tsx".to_string(),
                "main.tsx".to_string(),
            ],
            excluded_path_segments: vec![
                "pages/".to_string(),
                "components/ui/".to_string(),
            ],
            ignored_symbol_keywords: vec![
                "main".to_string(),
                "app".to_string(),
                "test".to_string(),
                "route".to_string(),
                "handler".to_string(),
                "register".to_string(),
                "force_".to_string(),
                "invalidate_".to_string(),
                "persist_".to_string(),
                "breaker".to_string(),
            ],
        }
    }
}

impl GraphConfig {
    /// Determines whether a given file path should be excluded from anomaly scanning.
    pub fn is_path_excluded(&self, real_path: &str) -> bool {
        for pattern in &self.excluded_file_patterns {
            if real_path.ends_with(pattern) {
                return true;
            }
        }
        for segment in &self.excluded_path_segments {
            if real_path.contains(segment) {
                return true;
            }
        }
        let path_obj = std::path::Path::new(real_path);
        path_obj.components().any(|c| {
            let name = c.as_os_str().to_string_lossy();
            self.excluded_paths.iter().any(|p| name == *p)
        })
    }

    /// Determines whether a symbol should be excluded from anomaly detection.
    pub fn is_symbol_excluded(&self, name: &str, kind: &str) -> bool {
        if kind == "module" || name == "__module__" {
            return true;
        }
        if self.excluded_symbols.iter().any(|s| s == name) {
            return true;
        }
        let name_lower = name.to_lowercase();
        self.ignored_symbol_keywords.iter().any(|kw| {
            name_lower == *kw || name_lower.contains(kw)
        })
    }
}

// Metadata: [config]

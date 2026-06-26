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
        }
    }
}

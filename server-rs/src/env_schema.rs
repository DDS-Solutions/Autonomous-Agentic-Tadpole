//! @docs ARCHITECTURE:EnvironmentSecurity
//!
//! ### AI Assist Note
//! **Startup Environment Validation (Gatekeeper)**: Orchestrates the
//! reading and verification of the `.env.schema` file to ensure
//! systemic requirements are met before engine boot. Features
//! **Schema-based Variable Verification**: supports decorators like
//! `@required`, `@sensitive`, and `@type` to manage data
//! sovereignty and security. Implements **Strict Fatal Halt**: in
//! production, the engine will `anyhow::bail!` if required variables
//! (e.g., `NEURAL_TOKEN`) are missing, preventing accidental
//! unauthenticated outages. AI agents should use the "Validation
//! Report" banner in logs to confirm environment readiness (ENV-01).
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Missing `.env.schema` file at the root,
//!   malformed schema decorators causing parse errors, or
//!   unauthorized access to sensitive variables in debug logs.
//! - **Telemetry Link**: Search `[env_schema]` in tracing logs.
//!   logs for the startup "Validation Report" status.
//! - **Trace Scope**: `server-rs::env_schema`

use std::path::Path;

/// A single entry parsed from the `.env.schema` file.
#[derive(Debug, Clone)]
pub struct EnvSchemaEntry {
    /// The environment variable name (e.g. `NEURAL_TOKEN`).
    pub name: String,
    /// Whether the engine fails to start if this variable is missing.
    pub required: bool,
    /// Whether the value should be redacted in logs and audits.
    pub sensitive: bool,
    /// Technical description appearing in tooltips/logs.
    pub description: String,
    /// Optional default value if not provided by the environment.
    pub default: Option<String>,
    /// Primitive type for validation (e.g. `int`, `url`, `string`).
    pub var_type: Option<String>,
}

/// Parsed schema result.
#[derive(Debug, Clone)]
pub struct EnvSchema {
    pub entries: Vec<EnvSchemaEntry>,
}

/// Validation result for a single variable.
#[derive(Debug)]
pub struct ValidationResult {
    pub name: String,
    pub is_set: bool,
    pub required: bool,
    pub sensitive: bool,
    #[allow(dead_code)]
    pub description: String,
}

impl EnvSchema {
    /// Parse schema from a string.
    pub fn parse_str(content: &str) -> Self {
        let mut entries = Vec::new();

        let mut current_decorators: Vec<String> = Vec::new();
        let mut current_comments: Vec<String> = Vec::new();
        let mut default_sensitive = false;

        for line in content.lines() {
            let trimmed = line.trim();

            // Parse header decorators
            if trimmed.starts_with("# @defaultSensitive=true") {
                default_sensitive = true;
                continue;
            }
            if trimmed.starts_with("# ---") || trimmed.starts_with("# ==") {
                // Divider — reset block comments
                current_decorators.clear();
                current_comments.clear();
                continue;
            }

            // Comment lines
            if trimmed.starts_with('#') {
                let comment_text = trimmed.trim_start_matches('#').trim();
                if comment_text.starts_with('@') {
                    current_decorators.push(comment_text.to_string());
                } else if !comment_text.is_empty() {
                    current_comments.push(comment_text.to_string());
                }
                continue;
            }

            // Empty lines reset the comment accumulator
            if trimmed.is_empty() {
                current_decorators.clear();
                current_comments.clear();
                continue;
            }

            // Variable assignment line: NAME=value
            if let Some(eq_pos) = trimmed.find('=') {
                let var_name = trimmed[..eq_pos].trim();

                // Parse decorators
                let mut required = false;
                let mut sensitive = default_sensitive;
                let mut var_type = None;
                let mut default = None;

                for dec in &current_decorators {
                    for token in dec.split_whitespace() {
                        match token {
                            "@required" => required = true,
                            "@sensitive" => sensitive = true,
                            _ if token.starts_with("@type=") => {
                                var_type = Some(token.trim_start_matches("@type=").to_string());
                            }
                            _ if token.starts_with("@default=") => {
                                default = Some(token.trim_start_matches("@default=").to_string());
                            }
                            _ if token.starts_with("@sensitive=false") => {
                                sensitive = false;
                            }
                            _ => {}
                        }
                    }
                }

                let description = current_comments.join(" ");

                entries.push(EnvSchemaEntry {
                    name: var_name.to_string(),
                    required,
                    sensitive,
                    description,
                    default,
                    var_type,
                });

                current_decorators.clear();
                current_comments.clear();
            }
        }

        Self { entries }
    }

    /// Parse the `.env.schema` file using a lightweight line-based parser.
    /// Checks if a sensitive token meets minimum length (>= 32 chars) and is not a known placeholder string.
    pub fn is_valid_token_value(val: &str) -> bool {
        let trimmed = val.trim();
        if trimmed.len() < 32 {
            return false;
        }
        let lower = trimmed.to_lowercase();
        if lower.contains("your-")
            || lower.contains("secret-token")
            || lower.contains("changeme")
            || lower.contains("placeholder")
            || lower.contains("replace-me")
        {
            return false;
        }
        true
    }

    /// Embedded compile-time fallback schema ensures standalone release binaries
    /// can always validate the schema even without the source tree on disk.
    pub const EMBEDDED_SCHEMA: &'static str = include_str!("../../.env.schema");

    /// Load and parse a `.env.schema` file from disk.
    /// Understands `@required`, `@sensitive`, `@type=...`, `@default=...` decorators.
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        Ok(Self::parse_str(&content))
    }

    /// Load from disk if resolved, otherwise fallback seamlessly to the embedded schema.
    pub fn load_or_embedded(path: &Path) -> Self {
        if let Some(resolved) = resolve_schema_path(Some(path)) {
            if let Ok(content) = std::fs::read_to_string(&resolved) {
                return Self::parse_str(&content);
            }
        }
        Self::parse_str(Self::EMBEDDED_SCHEMA)
    }

    /// Validate all schema entries against the current environment.
    /// Returns a list of validation results.
    pub fn validate(&self) -> Vec<ValidationResult> {
        self.entries
            .iter()
            .map(|entry| {
                let is_set = std::env::var(&entry.name)
                    .map(|v| {
                        let trimmed = v.trim();
                        if trimmed.is_empty() {
                            return false;
                        }
                        if entry.name == "NEURAL_TOKEN"
                            || entry.name == "NEURAL_ENGINE_ACCESS_TOKEN"
                            || entry.name == "AUDIT_PRIVATE_KEY"
                        {
                            if !Self::is_valid_token_value(trimmed) {
                                return false;
                            }
                        }
                        true
                    })
                    .unwrap_or(false);

                ValidationResult {
                    name: entry.name.clone(),
                    is_set,
                    required: entry.required,
                    sensitive: entry.sensitive,
                    description: entry.description.clone(),
                }
            })
            .collect()
    }

    /// Returns entries as a safe metadata list (for the schema API).
    /// Never includes actual secret values.
    pub fn to_safe_metadata(&self) -> Vec<serde_json::Value> {
        self.entries
            .iter()
            .map(|entry| {
                let is_set = std::env::var(&entry.name)
                    .map(|v| !v.trim().is_empty())
                    .unwrap_or(false);

                serde_json::json!({
                    "name": entry.name,
                    "required": entry.required,
                    "sensitive": entry.sensitive,
                    "description": entry.description,
                    "type": entry.var_type,
                    "hasDefault": entry.default.is_some(),
                    "isSet": is_set,
                })
            })
            .collect()
    }
}

/// Resolves the `.env.schema` path across WORKSPACE_ROOT, CWD, and parent directories.
pub fn resolve_schema_path(explicit_path: Option<&Path>) -> Option<std::path::PathBuf> {
    if let Some(p) = explicit_path {
        if p.exists() {
            return Some(p.to_path_buf());
        }
    }

    if let Ok(ws) = std::env::var("WORKSPACE_ROOT") {
        let p = Path::new(&ws).join(".env.schema");
        if p.exists() {
            return Some(p);
        }
    }

    let local = Path::new(".env.schema");
    if local.exists() {
        return Some(local.to_path_buf());
    }

    let parent = Path::new("../.env.schema");
    if parent.exists() {
        return Some(parent.to_path_buf());
    }

    None
}

/// Run startup validation. Logs a clear banner and returns any fatal errors.
pub fn validate_and_report(schema_path: &Path) -> anyhow::Result<()> {
    let resolved = resolve_schema_path(Some(schema_path));
    let (schema, _source_desc) = match resolved {
        Some(p) => (EnvSchema::load(&p)?, format!("{:?}", p)),
        None => {
            tracing::info!(
                "ℹ️  [EnvSchema] No on-disk .env.schema found at {:?}; utilizing compiled-in embedded schema.",
                schema_path
            );
            (EnvSchema::parse_str(EnvSchema::EMBEDDED_SCHEMA), "compiled-in embedded schema".to_string())
        }
    };

    let results = schema.validate();

    tracing::info!("╔══════════════════════════════════════════════════════╗");
    tracing::info!("║           🔐 Environment Validation Report          ║");
    tracing::info!("╠══════════════════════════════════════════════════════╣");

    let mut missing_required = Vec::new();

    for result in &results {
        let status = if result.is_set {
            "✅"
        } else if result.required {
            "🚨"
        } else {
            "⚠️ "
        };
        let sensitivity = if result.sensitive { "🔒" } else { "  " };
        let set_label = if result.is_set { "set" } else { "missing" };

        tracing::info!(
            "║ {} {} {:<30} ({})",
            status,
            sensitivity,
            result.name,
            set_label
        );

        if result.required && !result.is_set {
            missing_required.push(result.name.clone());
        }
    }

    tracing::info!("╚══════════════════════════════════════════════════════╝");

    if !missing_required.is_empty() {
        let names = missing_required.join(", ");
        if cfg!(debug_assertions) {
            tracing::warn!(
                "⚠️  [EnvSchema] Missing required variables (dev mode — continuing): {}",
                names
            );
        } else {
            // NEURAL_TOKEN is now strictly required. No default injection.

            anyhow::bail!(
                "🚨 FATAL: Missing required environment variables: {}. Set them in .env or your secret manager.",
                names
            );
        }
    }

    tracing::info!(
        "🔐 [EnvSchema] Validation complete — {} variables checked.",
        results.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_env_schema_parse_all_decorators() {
        let schema_raw = r#"
# Global configuration
# @defaultSensitive=true

# Port for the server
# @type=int @default=3000
SERVER_PORT=3000

# Primary authentication token
# @required @sensitive @type=string
NEURAL_TOKEN=

# Telemetry bridge URL
# @type=url @sensitive=false
TELEMETRY_ENDPOINT=https://telemetry.tadpole.internal
"#;

        let schema = EnvSchema::parse_str(schema_raw);
        assert_eq!(schema.entries.len(), 3);

        // Entry 1: SERVER_PORT
        let port_entry = &schema.entries[0];
        assert_eq!(port_entry.name, "SERVER_PORT");
        assert!(!port_entry.required);
        assert!(port_entry.sensitive); // inherited from @defaultSensitive=true
        assert_eq!(port_entry.var_type.as_deref(), Some("int"));
        assert_eq!(port_entry.default.as_deref(), Some("3000"));
        assert_eq!(port_entry.description, "Port for the server");

        // Entry 2: NEURAL_TOKEN
        let token_entry = &schema.entries[1];
        assert_eq!(token_entry.name, "NEURAL_TOKEN");
        assert!(token_entry.required);
        assert!(token_entry.sensitive);
        assert_eq!(token_entry.var_type.as_deref(), Some("string"));

        // Entry 3: TELEMETRY_ENDPOINT
        let telem_entry = &schema.entries[2];
        assert_eq!(telem_entry.name, "TELEMETRY_ENDPOINT");
        assert!(!telem_entry.required);
        assert!(!telem_entry.sensitive); // overridden by @sensitive=false
        assert_eq!(telem_entry.var_type.as_deref(), Some("url"));
    }

    #[test]
    fn test_env_schema_validation_detects_missing_required() {
        let schema_raw = r#"
# @required
TEST_UNSET_CRITICAL_VAR_123=
"#;
        let schema = EnvSchema::parse_str(schema_raw);
        let results = schema.validate();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "TEST_UNSET_CRITICAL_VAR_123");
        assert!(results[0].required);
        assert!(!results[0].is_set);
    }

    #[test]
    fn test_env_schema_validation_recognizes_set_variables() {
        std::env::set_var("TEST_SET_SCHEMA_VAR_456", "active_value");
        let schema_raw = r#"
# @required
TEST_SET_SCHEMA_VAR_456=
"#;
        let schema = EnvSchema::parse_str(schema_raw);
        let results = schema.validate();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "TEST_SET_SCHEMA_VAR_456");
        assert!(results[0].is_set);
        std::env::remove_var("TEST_SET_SCHEMA_VAR_456");
    }

    #[test]
    fn test_env_schema_token_validation() {
        assert!(!EnvSchema::is_valid_token_value("short"));
        assert!(!EnvSchema::is_valid_token_value("your-secret-token-here-1234567890"));
        assert!(!EnvSchema::is_valid_token_value("placeholder-token-0123456789012345"));
        assert!(EnvSchema::is_valid_token_value("a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6"));
    }

    #[test]
    fn test_env_schema_whitespace_trimming() {
        let schema_raw = "  NEURAL_TOKEN   =  ";
        let schema = EnvSchema::parse_str(schema_raw);
        assert_eq!(schema.entries.len(), 1);
        assert_eq!(schema.entries[0].name, "NEURAL_TOKEN");
    }

    #[test]
    fn test_env_schema_embedded_fallback() {
        let nonexistent = Path::new("nonexistent_schema_file.schema");
        let schema = EnvSchema::load_or_embedded(nonexistent);
        assert!(!schema.entries.is_empty(), "Embedded schema should contain parsed entries");
        assert!(
            schema.entries.iter().any(|e| e.name == "NEURAL_TOKEN"),
            "Embedded schema must include NEURAL_TOKEN"
        );
    }
}

// Metadata: [env_schema]

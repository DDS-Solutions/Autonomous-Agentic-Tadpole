//! @docs ARCHITECTURE:SecretRedaction
//!
//! ### AI Assist Note
//! **Neural Shield Protection (Secret Redactor)**: Orchestrates the
//! runtime log redaction and data isolation for the Tadpole OS engine.
//! Features **Automated Secret Masking**: proactively monitors system
//! broadcasts, error messages, and mission logs for sensitive
//! environment variables (e.g., `GOOGLE_API_KEY`). Implements a
//! **Minimum Secret Length (MIN_SECRET_LEN=8)**: prevents
//! over-redaction of common technical terms (e.g., "sk", "id"). AI
//! agents must register new sensitive keys in the `from_env`
//! constructor to guarantee they are scrubbed from all downstream
//! trace and telemetry streams (SEC-04).
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Short secrets bypassing the shield due to length
//!   thresholds, missing environment variables in the redactor
//!   registry, or performance overhead during high-concurrency log
//!   processing.
//! - **Telemetry Link**: Search `[secret_redactor]` in tracing logs.
//!   `[REDACTED]` in `tracing` logs for validation of successful
//!   scrubbing.
//! - **Trace Scope**: `server-rs::secret_redactor`

use once_cell::sync::Lazy;
use regex::{Regex, RegexSet};
use std::sync::Arc;

/// Minimum length for a secret to be registered. Very short strings
/// (like "sk") would cause excessive false-positive redactions.
const MIN_SECRET_LEN: usize = 8;

/// Index of the JSON key pattern in PATTERNS (special handling to preserve key name).
const JSON_KEY_PATTERN_IDX: usize = 2;

/// Static patterns for common secret formats (Neural Shield).
static PATTERNS: Lazy<(RegexSet, Vec<Regex>)> = Lazy::new(|| {
    let patterns = vec![
        // 0. Bearer tokens in headers or strings
        r"(?i)bearer\s+[a-zA-Z0-9\-\._~+/]+=*",
        // 1. Authorization headers
        r"(?i)authorization:\s*[^\s,]+",
        // 2. JSON keys: "apiKey": "...", "token": "...", etc.
        r#"(?i)("?(?:api_key|secret|password|token|key|credential)"?\s*[:=]\s*)(["'])(?:\\.|[^"'])*(["'])"#,
        // 3. OpenAI
        r"(?i)sk-[a-zA-Z0-9]{20,}",
        // 4. Google Gemini
        r"(?i)AIza[0-9A-Za-z-_]{30,}",
        // 5. GitHub Classic PAT
        r"(?i)ghp_[a-zA-Z0-9]{30,}",
        // 6. GitHub Fine-Grained PAT
        r"(?i)github_pat_[a-zA-Z0-9_]{22,}",
        // 7. Anthropic (aligned with standard prefix and modern length)
        r"(?i)sk-ant-[a-zA-Z0-9\-_]{20,}",
        // 8. Groq
        r"(?i)gsk_[a-zA-Z0-9]{50,}",
        // 9. AWS Keys
        r"(?i)AKIA[0-9A-Z]{16}",
        // 10. Database URLs (PII in connection strings)
        r"(?i)(?:postgres|postgresql|mongodb|mysql|redis)://[a-zA-Z0-9\-_]+:[a-zA-Z0-9\-_]+@[a-zA-Z0-9\-_.]+",
    ];

    let set = match RegexSet::new(&patterns) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("❌ [SecretRedactor] CRITICAL: Failed to initialize Neural Shield RegexSet. Redaction may be incomplete. Error: {}", e);
            RegexSet::empty()
        }
    };

    let regexes: Vec<Regex> = patterns
        .iter()
        .map(|p| match Regex::new(p) {
            Ok(re) => re,
            Err(e) => {
                tracing::error!(
                    "❌ [SecretRedactor] Failed to compile pattern '{}': {}",
                    p,
                    e
                );
                Regex::new(r"[^\s\S]").unwrap() // Dummy regex that never matches
            }
        })
        .collect();

    (set, regexes)
});

/// Thread-safe secret redactor that holds known sensitive values.
#[derive(Debug, Clone)]
pub struct SecretRedactor {
    /// The actual secret values to scan for (never logged).
    secrets: Arc<Vec<String>>,
    /// Flag indicating if this is a no-op redactor.
    is_noop: bool,
}

impl SecretRedactor {
    /// Build a redactor from the current environment.
    /// Only registers non-empty values from known sensitive env vars.
    pub fn from_env() -> Self {
        let sensitive_vars = [
            "NEURAL_TOKEN",
            "NEURAL_TOKEN_OLD",
            "NEURAL_TOKEN_NEW",
            "NEURAL_ENGINE_ACCESS_TOKEN",
            "AUDIT_PRIVATE_KEY",
            "GOOGLE_API_KEY",
            "GROQ_API_KEY",
            "OPENAI_API_KEY",
            "ANTHROPIC_API_KEY",
            "REPLICATE_API_KEY",
            "INCEPTION_API_KEY",
            "DEEPSEEK_API_KEY",
            "DISCORD_WEBHOOK",
        ];

        let secrets: Vec<String> = sensitive_vars
            .iter()
            .filter_map(|var| std::env::var(var).ok())
            .filter(|val| val.len() >= MIN_SECRET_LEN)
            .collect();

        tracing::info!(
            "🔐 [SecretRedactor] Initialized with {} registered secret(s) and Neural Shield patterns.",
            secrets.len()
        );

        Self {
            secrets: Arc::new(secrets),
            is_noop: false,
        }
    }

    /// Returns a new copy of the input where any known secret is replaced
    /// with `[REDACTED]`. Performs a simple substring scan AND sequential pattern matching.
    pub fn scrub(&self, text: &str) -> String {
        if self.is_noop {
            return text.to_string();
        }
        let mut result = text.to_string();

        // 1. Scrub registered environment secrets
        for secret in self.secrets.iter() {
            if result.contains(secret.as_str()) {
                result = result.replace(secret.as_str(), "[REDACTED]");
            }
        }

        // 2. Scrub via Neural Shield (Regex) — sequential evaluation against mutating string
        let (_set, regexes) = &*PATTERNS;
        for (idx, re) in regexes.iter().enumerate() {
            if idx == JSON_KEY_PATTERN_IDX {
                if let std::borrow::Cow::Owned(redacted) = re.replace_all(&result, r#"$1$2[REDACTED]$3"#) {
                    result = redacted;
                }
            } else if let std::borrow::Cow::Owned(redacted) = re.replace_all(&result, "[REDACTED]") {
                result = redacted;
            }
        }

        result
    }

    /// Backwards compatibility for the old name.
    pub fn redact(&self, text: &str) -> String {
        self.scrub(text)
    }

    /// Returns true if the redactor has any secrets or patterns registered.
    /// Checks both runtime env-var secrets AND the always-compiled Neural Shield patterns.
    #[allow(dead_code)]
    pub fn is_active(&self) -> bool {
        if self.is_noop {
            return false;
        }
        !self.secrets.is_empty() || !PATTERNS.0.is_empty()
    }

    /// Checks if a string contains any of the registered secrets or patterns.
    /// Used for proactive safety scanning before execution or logging.
    pub fn is_sensitive(&self, text: &str) -> bool {
        for secret in self.secrets.iter() {
            if text.contains(secret.as_str()) {
                return true;
            }
        }
        PATTERNS.0.is_match(text)
    }

    /// Creates a redactor with only Neural Shield static patterns (no env secrets).
    pub fn empty() -> Self {
        Self {
            secrets: Arc::new(Vec::new()),
            is_noop: false,
        }
    }

    /// Creates a no-op redactor for testing.
    pub fn noop() -> Self {
        Self {
            secrets: Arc::new(Vec::new()),
            is_noop: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_redact_known_secret() {
        let redactor = SecretRedactor {
            secrets: Arc::new(vec!["sk-abcdef123456789".to_string()]),
            is_noop: false,
        };
        let input = "Error: Invalid API Key: sk-abcdef123456789";
        let output = redactor.redact(input);
        assert_eq!(output, "Error: Invalid API Key: [REDACTED]");
        assert!(!output.contains("sk-abcdef"));
    }

    #[test]
    fn test_no_false_positive() {
        let redactor = SecretRedactor {
            secrets: Arc::new(vec!["my-secret-key-12345".to_string()]),
            is_noop: false,
        };
        let input = "Normal log message with no secrets";
        assert_eq!(redactor.redact(input), input);
    }

    #[test]
    fn test_multiple_secrets() {
        let redactor = SecretRedactor {
            secrets: Arc::new(vec![
                "secret-one-12345678".to_string(),
                "secret-two-87654321".to_string(),
            ]),
            is_noop: false,
        };
        let input = "Key1: secret-one-12345678, Key2: secret-two-87654321";
        let output = redactor.redact(input);
        assert_eq!(output, "Key1: [REDACTED], Key2: [REDACTED]");
    }

    #[test]
    fn test_noop_redactor() {
        let redactor = SecretRedactor::noop();
        assert!(!redactor.is_active());
        let input = "anything goes";
        assert_eq!(redactor.redact(input), input);
    }

    #[test]
    fn test_github_pat_and_anthropic_scrubbing() {
        let redactor = SecretRedactor::empty();
        // Fine-grained GitHub PAT
        let gh_input = "Exported token: github_pat_11ABCD0123456789abcdef_ghijklmnopqrstuvwxyz0123456789 in logs";
        let gh_output = redactor.scrub(gh_input);
        assert!(!gh_output.contains("github_pat_"));
        assert!(gh_output.contains("[REDACTED]"));

        // Anthropic key
        let ant_input = "anthropic key: sk-ant-api03-abcdef1234567890abcdef123456";
        let ant_output = redactor.scrub(ant_input);
        assert!(!ant_output.contains("sk-ant-"));
        assert!(ant_output.contains("[REDACTED]"));

        // AWS key
        let aws_input = "aws: AKIAIOSFODNN7EXAMPLE";
        let aws_output = redactor.scrub(aws_input);
        assert!(!aws_output.contains("AKIAIOSFODNN7EXAMPLE"));
        assert!(aws_output.contains("[REDACTED]"));
    }

    #[test]
    fn test_json_key_and_sequential_scrubbing() {
        let redactor = SecretRedactor::empty();
        let json_input = r#"{"api_key": "sk-123456789012345678901234", "other": "fine"}"#;
        let json_output = redactor.scrub(json_input);
        assert!(json_output.contains(r#""api_key": "[REDACTED]""#));
        assert!(!json_output.contains("sk-123456789012345678901234"));
    }
}

// Metadata: [secret_redactor]

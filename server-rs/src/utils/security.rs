//! Security Foundation & Hardening Utilities
//!
//! Provides the core primitives for path validation, sensitive-ID
//! sanitization, and regex-based secret redaction (PII/Key protection).
//! Implements **SEC-03** and **SEC-04** zero-trust models.

use crate::error::AppError;
use std::path::{Path, PathBuf};

/// A non-forgeable wrapper around a validated path.
/// Can only be created through successful validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SafePath(PathBuf);

impl SafePath {
    /// Internal: Creates a SafePath from a trusted source.
    pub(crate) fn from_trusted(p: PathBuf) -> Self {
        Self(p)
    }

    /// Public: Validates a path relative to a base directory.
    #[allow(dead_code)]
    pub fn validate(base: &Path, user_path: &str) -> Result<Self, AppError> {
        validate_path(base, user_path)
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }

    #[allow(dead_code)]
    pub fn to_path_buf(&self) -> PathBuf {
        self.0.clone()
    }

    /// Sanitizes a path string for use in breadcrumbs to prevent traversal injection.
    #[allow(dead_code)]
    pub fn sanitize_breadcrumb(path: &str) -> String {
        path.chars()
            .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-' || *c == '.' || *c == '/' || *c == '\\')
            .collect()
    }
}

impl AsRef<Path> for SafePath {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl std::ops::Deref for SafePath {
    type Target = Path;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Sanitizes and validates a path to prevent directory traversal.
pub fn validate_path(base: &Path, user_path: &str) -> Result<SafePath, AppError> {
    let base_raw = if base.is_absolute() {
        base.to_path_buf()
    } else {
        std::env::current_dir()?.join(base)
    };

    fn normalize(p: &Path) -> PathBuf {
        let mut components = Vec::new();
        for component in p.components() {
            match component {
                std::path::Component::Prefix(prefix) => components.push(std::path::Component::Prefix(prefix)),
                std::path::Component::RootDir => components.push(std::path::Component::RootDir),
                std::path::Component::CurDir => {}
                std::path::Component::ParentDir => {
                    if let Some(std::path::Component::Normal(_)) = components.last() {
                        components.pop();
                    }
                }
                std::path::Component::Normal(c) => components.push(std::path::Component::Normal(c)),
            }
        }
        components.iter().collect()
    }

    let base_abs = normalize(&base_raw);
    let joined = base_abs.join(user_path);
    let result = normalize(&joined);

    if !result.starts_with(&base_abs) {
        return Err(AppError::Forbidden("Path traversal detected: outside authorized base".to_string()));
    }

    Ok(SafePath(result))
}

/// Strictly sanitizes a string to be used as a filename or ID.
pub fn sanitize_id(id: &str) -> String {
    id.chars()
        .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
        .collect()
}

/// Redacts sensitive information from strings.
pub fn redact_secrets(input: &str) -> String {
    use once_cell::sync::Lazy;
    use regex::{Regex, RegexSet};

    static PATTERNS: Lazy<(RegexSet, Vec<Regex>)> = Lazy::new(|| {
        let patterns = vec![
            r"(?i)bearer\s+[a-zA-Z0-9\-\._~+/]+=*",
            r"(?i)authorization:\s*[^\s,]+",
            r#"(?i)("?(?:api_key|secret|password|token|key|credential)"?\s*[:=]\s*)(["'])(?:\\.|[^"'])*(["'])"#,
            r"(?i)sk-[a-zA-Z0-9]{20,}",
            r"(?i)AIza[0-9A-Za-z-_]{30,}",
            r"(?i)ghp_[a-zA-Z0-9]{30,}",
            r"(?i)AKIA[0-9A-Z]{16}",
        ];
        let set = RegexSet::new(&patterns).expect("Security patterns must be valid regex.");
        let regexes = patterns.iter().map(|p| Regex::new(p).unwrap()).collect();
        (set, regexes)
    });

    let mut output = input.to_string();
    let (set, regexes) = &*PATTERNS;

    if set.is_match(&output) {
        for (idx, re) in regexes.iter().enumerate() {
            if set.matches(&output).matched(idx) {
                if idx == 2 {
                    output = re.replace_all(&output, r#"$1$2[REDACTED]$3"#).to_string();
                } else {
                    output = re.replace_all(&output, "[REDACTED]").to_string();
                }
            }
        }
    }
    output
}

/// Validates a shell command against a ZERO-TRUST whitelist using tokenized arguments.
/// 
/// ### 🛡️ SEC-03: Tokenized Validation
/// Accepts a binary and an array of arguments to prevent shell meta-character 
/// injection. Validates each token individually against a whitelist.
pub fn validate_tokenized_command(bin: &str, args: &[String]) -> Result<(), AppError> {
    let lower_bin = bin.to_lowercase();

    // 1. Whitelist of Allowed Base Binaries
    let allowed_binaries = [
        "ls", "cd", "pwd", "cat", "echo", "grep", "find", 
        "cargo", "npm", "git", "python", "node", "rustc",
        "mkdir", "cp", "mv", "touch", "test", "powershell", "sh"
    ];

    if !allowed_binaries.contains(&lower_bin.as_str()) {
        return Err(AppError::Forbidden(format!("Binary '{}' is not in the authorized whitelist", bin)));
    }

    // 2. Scan arguments for dangerous patterns
    let dangerous_patterns = ["$(", "`", "${", "|", ">", "<", ";", "&"];
    let restricted_paths = ["/etc", "/root", "/var", "/bin", "/usr", "C:\\Windows"];

    for arg in args {
        let lower_arg = arg.to_lowercase();
        
        // Block meta-characters even in arguments (defense in depth)
        for pattern in dangerous_patterns {
            if lower_arg.contains(pattern) {
                return Err(AppError::Forbidden(format!("Dangerous meta-character '{}' detected in argument: {}", pattern, arg)));
            }
        }

        // Block access to system paths
        for path in restricted_paths {
            if lower_arg.contains(path) {
                return Err(AppError::Forbidden(format!("Attempt to access restricted system path: {}", arg)));
            }
        }

        // Block specific dangerous flags
        if lower_arg == "--erase" || lower_arg == "--delete" || lower_arg == "-rf" {
            return Err(AppError::Forbidden(format!("Dangerous flag detected: {}", arg)));
        }
    }

    Ok(())
}

/// [LEGACY] Validates a raw shell command string against a ZERO-TRUST whitelist.
/// Prefer `validate_tokenized_command` where possible.
#[allow(dead_code)]
pub fn validate_shell_command(command: &str) -> Result<(), AppError> {
    let lower = command.to_lowercase();

    // 1. Block Command Substitution & Expansion (Critical Vulnerability)
    if lower.contains("$(") || lower.contains("`") || lower.contains("${") {
        return Err(AppError::Forbidden("Command substitution or variable expansion detected".to_string()));
    }

    // 2. Block Piping and Redirection (Except to /dev/null)
    if lower.contains("|") || (lower.contains(">") && !lower.contains("/dev/null")) || lower.contains("<") || lower.contains(";") || lower.contains("&") {
        return Err(AppError::Forbidden("Piping, redirection, or multiple commands prohibited".to_string()));
    }

    // 3. Whitelist of Allowed Base Commands
    let allowed_commands = [
        "ls", "cd", "pwd", "cat", "echo", "grep", "find", 
        "cargo", "npm", "git", "python", "node", "rustc",
        "mkdir", "cp", "mv", "touch", "test"
    ];

    let first_word = lower.split_whitespace().next().unwrap_or("");
    if !allowed_commands.contains(&first_word) {
        return Err(AppError::Forbidden(format!("Command '{}' is not in the authorized whitelist", first_word)));
    }

    // 4. Blacklist specific dangerous flags for allowed commands
    let dangerous_flags = ["--erase", "--delete", "-rf", "/etc", "/root", "/var", "/bin", "/usr"];
    for flag in dangerous_flags {
        if lower.contains(flag) {
            return Err(AppError::Forbidden(format!("Dangerous flag or path detected: '{}'", flag)));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_path_traversal() {
        let base = Path::new("/tmp/base");
        assert!(validate_path(base, "../outside").is_err());
        assert!(validate_path(base, "user1/../../outside").is_err());
    }

    #[test]
    fn test_validate_shell_zero_trust() {
        // Authorized
        assert!(validate_shell_command("ls -la").is_ok());
        assert!(validate_shell_command("cargo build --release").is_ok());
        assert!(validate_shell_command("npm test").is_ok());

        // Unauthorized Command
        assert!(validate_shell_command("rm -rf .").is_err());
        assert!(validate_shell_command("curl http://evil.com").is_err());

        // Injection Attempts
        assert!(validate_shell_command("ls; rm -rf /").is_err());
        assert!(validate_shell_command("echo $(cat /etc/passwd)").is_err());
        assert!(validate_shell_command("ls `rm -rf /`").is_err());
        assert!(validate_shell_command("cat /etc/passwd > out.txt").is_err());
        
        // Dangerous Flags/Paths
        assert!(validate_shell_command("ls /etc/shadow").is_err());
    }
}

// Metadata: [security]

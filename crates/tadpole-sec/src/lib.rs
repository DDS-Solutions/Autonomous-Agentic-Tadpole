//! @docs ARCHITECTURE:Core
//!
//! ### AI Context Alignment
//! - **Subsystem**: Sovereign Tooling / High-Performance Secret & Token Scanner
//! - **Primary Entrypoints**: `SecretScanner`, `scan_content`, `scan_directory`
//!
//! ### ⚠️ Invariants & Non-Negotiables
//! - `[Security]` Precompiled `RegexSet` matching `.gitleaks.toml` patterns.
//! - `[Performance]` Sub-second multi-file scanner without Python GIL overhead.
//!
//! ### 🔍 Debugging & Observability
//! - **Telemetry Link**: Search `[tadpole_sec]` in audit logs.

pub const TELEMETRY_TAG: &str = "[tadpole_sec]";

use regex::RegexSet;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SecretFinding {
    pub file_path: String,
    pub rule_name: String,
    pub line_number: usize,
    pub masked_token: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ScanReport {
    pub files_scanned: usize,
    pub findings: Vec<SecretFinding>,
    pub clean: bool,
}

pub struct SecretScanner {
    regex_set: RegexSet,
    rule_names: Vec<String>,
}

impl Default for SecretScanner {
    fn default() -> Self {
        Self::new()
    }
}

const GITLEAKS_TOML_SSOT: &str = include_str!("../../../.gitleaks.toml");

fn parse_gitleaks_rules(toml_src: &str) -> (Vec<String>, Vec<String>) {
    let mut patterns = Vec::new();
    let mut names = Vec::new();

    for block in toml_src.split("[[rules]]").skip(1) {
        let mut rule_id: Option<String> = None;
        let mut description: Option<String> = None;
        let mut regex: Option<String> = None;

        for raw_line in block.lines() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(val) = line.strip_prefix("id =").map(str::trim) {
                rule_id = Some(unquote_toml_str(val));
            } else if let Some(val) = line.strip_prefix("description =").map(str::trim) {
                description = Some(unquote_toml_str(val));
            } else if let Some(val) = line.strip_prefix("regex =").map(str::trim) {
                regex = Some(unquote_toml_str(val));
            }
        }

        if let Some(pattern) = regex {
            patterns.push(pattern);
            names.push(description.or(rule_id).unwrap_or_else(|| "Unknown Rule".to_string()));
        }
    }

    (patterns, names)
}

fn unquote_toml_str(raw: &str) -> String {
    let trimmed = raw.trim();
    if let Some(inner) = trimmed.strip_prefix("'''").and_then(|s| s.strip_suffix("'''")) {
        return inner.to_string();
    }
    if let Some(inner) = trimmed.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
        return inner.to_string();
    }
    if let Some(inner) = trimmed.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')) {
        return inner.to_string();
    }
    trimmed.to_string()
}

impl SecretScanner {
    /// Builds a scanner directly from the workspace `.gitleaks.toml` Single Source of Truth.
    pub fn new() -> Self {
        Self::from_gitleaks_toml(GITLEAKS_TOML_SSOT).expect("Valid .gitleaks.toml rules")
    }

    /// Loads scanner rules dynamically from an external `.gitleaks.toml` file path at runtime.
    pub fn from_gitleaks_file(path: &Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        Self::from_gitleaks_toml(&content)
    }

    /// Parses `[[rules]]` blocks from `.gitleaks.toml` content into a precompiled `RegexSet`.
    pub fn from_gitleaks_toml(toml_src: &str) -> anyhow::Result<Self> {
        let (mut patterns, mut rule_names) = parse_gitleaks_rules(toml_src);
        patterns.push(r"-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----".to_string());
        rule_names.push("Private Key Header".to_string());

        let regex_set = RegexSet::new(&patterns)?;
        Ok(Self {
            regex_set,
            rule_names,
        })
    }

    /// Scans a text buffer line by line, returning all detected secrets with masked output.
    pub fn scan_content(&self, content: &str, file_path: &str) -> Vec<SecretFinding> {
        let mut findings = Vec::new();

        for (idx, line) in content.lines().enumerate() {
            let matches = self.regex_set.matches(line);
            if matches.matched_any() {
                for rule_idx in matches.into_iter() {
                    let rule_name = &self.rule_names[rule_idx];
                    findings.push(SecretFinding {
                        file_path: file_path.to_string(),
                        rule_name: rule_name.clone(),
                        line_number: idx + 1,
                        masked_token: mask_line(line),
                    });
                }
            }
        }

        findings
    }

    /// Recursively scans a directory tree while ignoring build artifacts and dependencies.
    pub fn scan_directory(&self, root: &Path, ignore_dirs: &[&str]) -> ScanReport {
        let mut report = ScanReport {
            files_scanned: 0,
            findings: Vec::new(),
            clean: true,
        };

        self.walk_dir(root, ignore_dirs, &mut report);
        report.clean = report.findings.is_empty();
        report
    }

    fn walk_dir(&self, dir: &Path, ignore_dirs: &[&str], report: &mut ScanReport) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();

                if path.is_dir() {
                    if ignore_dirs.contains(&file_name) || file_name.starts_with('.') {
                        continue;
                    }
                    self.walk_dir(&path, ignore_dirs, report);
                } else if path.is_file() {
                    // Skip files > 2MB or common binary/archive extensions
                    if let Ok(meta) = path.metadata() {
                        if meta.len() > 2 * 1024 * 1024 {
                            continue;
                        }
                    }
                    if is_binary_extension(file_name) {
                        continue;
                    }

                    if let Ok(content) = std::fs::read_to_string(&path) {
                        report.files_scanned += 1;
                        let mut file_findings = self.scan_content(&content, &path.to_string_lossy());
                        report.findings.append(&mut file_findings);
                    }
                }
            }
        }
    }
}

fn mask_line(line: &str) -> String {
    let trimmed = line.trim();
    if trimmed.len() <= 12 {
        "***REDACTED***".to_string()
    } else {
        format!("{}...[REDACTED]", &trimmed[..8])
    }
}

fn is_binary_extension(name: &str) -> bool {
    name.ends_with(".png")
        || name.ends_with(".jpg")
        || name.ends_with(".exe")
        || name.ends_with(".dll")
        || name.ends_with(".so")
        || name.ends_with(".wasm")
        || name.ends_with(".tar")
        || name.ends_with(".gz")
        || name.ends_with(".zip")
        || name.ends_with(".lock")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scan_clean_content() {
        let scanner = SecretScanner::new();
        let content = "const port = 8080;\nconst host = '127.0.0.1';";
        let findings = scanner.scan_content(content, "src/config.ts");
        assert!(findings.is_empty());
    }

    #[test]
    fn test_scan_detects_real_patterns() {
        let scanner = SecretScanner::new();
        // Construct synthetic tokens matching .gitleaks.toml rules
        let gemini_key = format!("AIza{}", "12345678901234567890123456789012345");
        let content = format!("const API_KEY = '{}'; // TODO remove before prod", gemini_key);
        let findings = scanner.scan_content(&content, "src/secrets.ts");

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule_name, "Google Gemini API Key");
        assert!(findings[0].masked_token.contains("[REDACTED]"));
    }
}

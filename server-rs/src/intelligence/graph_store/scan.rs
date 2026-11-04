//! @docs ARCHITECTURE:CodeBaseIntelligence
//!
//! ### AI Assist Note
//! **Graph Source Scanner**: Walks the workspace and extracts supported symbols.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: File read, metadata, or symbol extraction failures.
//! - **Telemetry Link**: Search [graph_store] in tracing logs.

use super::*;
use crate::utils::parser::{Symbol, SymbolExtractor, SymbolRange};
use once_cell::sync::Lazy;
use regex::Regex;
use std::path::Path;
use walkdir::WalkDir;

static PY_FUNC_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\s*(?:async\s+)?def\s+([A-Za-z_][A-Za-z0-9_]*)").unwrap());
static PY_CLASS_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\s*class\s+([A-Za-z_][A-Za-z0-9_]*)").unwrap());
static SQL_CLASS_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)^\s*CREATE\s+TABLE\s+(?:IF\s+NOT\s+EXISTS\s+)?([A-Za-z_][A-Za-z0-9_]*)").unwrap());
static JS_FUNC_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\s*(?:export\s+)?(?:async\s+)?function\s+([A-Za-z_][A-Za-z0-9_]*)").unwrap());
static JS_CLASS_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\s*(?:export\s+)?class\s+([A-Za-z_][A-Za-z0-9_]*)").unwrap());
static JS_VAR_FUNC_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\s*(?:export\s+)?(?:const|let|var)\s+([A-Za-z_][A-Za-z0-9_]*)\s*=").unwrap());
static SH_FUNC_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\s*(?:function\s+)?([A-Za-z_][A-Za-z0-9_-]*)\s*(?:\(\))?\s*\{").unwrap());
static IMPORT_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r#"(?i)(?:import|from|require|use)\s+["']?([A-Za-z0-9_./:-]+)"#).unwrap());
static CALL_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\b([A-Za-z_][A-Za-z0-9_]*)\s*\(").unwrap());

static RS_IMPORT_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\s*use\s+([A-Za-z0-9_:]+)").unwrap());
static OTHER_IMPORT_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r#"^\s*import\s+(?:.+?\s+from\s+)?["']?([A-Za-z0-9_@./-]+)"#).unwrap());

pub(super) fn scan_files(root: &Path, salt: &str) -> Result<Vec<FileRecord>, AppError> {
    let mut extractor = SymbolExtractor::new();
    let mut records = Vec::new();
    for entry in WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            if name.starts_with("target") {
                return false;
            }
            !matches!(
                name.as_ref(),
                "node_modules" | ".git" | "dist" | "scratch" | "3rdparty" | ".tmp" | "tmp" | "workspaces" | ".agent"
            )
        })
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
    {
        let path = entry.path();
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !matches!(
            ext,
            "rs" | "ts" | "tsx" | "py" | "sql" | "js" | "cjs" | "mjs" | "ps1" | "sh"
        ) {
            continue;
        }
        if std::fs::metadata(path).map(|m| m.len()).unwrap_or(0) > 2 * 1024 * 1024 {
            continue;
        }
        let content = match std::fs::read_to_string(path) {
            Ok(content) => content,
            Err(_) => continue,
        };
        let absolute_path = path
            .canonicalize()
            .unwrap_or_else(|_| path.to_path_buf())
            .to_string_lossy()
            .to_string();
        let relative_path = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string()
            .replace('\\', "/");
        let language = language_for_ext(ext).to_string();
        let is_test = is_test_path(&relative_path);
        let (symbols, refs, imports) = if matches!(ext, "rs" | "ts" | "tsx") {
            let symbols = extractor
                .extract_symbols(path, &content)
                .into_iter()
                .map(symbol_to_record)
                .collect();
            (
                symbols,
                extractor.extract_references(path, &content),
                extract_imports(ext, &content),
            )
        } else {
            lightweight_extract(ext, &content)
        };
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        hasher.update(salt.as_bytes());
        let hash = hasher.finalize();
        let file_hash = hex::encode(hash);

        records.push(FileRecord {
            absolute_path,
            relative_path,
            name,
            language,
            is_test,
            symbols,
            refs,
            imports,
            file_hash,
        });
    }
    records.sort_by(|a, b| a.absolute_path.cmp(&b.absolute_path));
    Ok(records)
}

fn symbol_to_record(sym: Symbol) -> SymbolRecord {
    SymbolRecord {
        name: sym.name,
        kind: sym.kind,
        line_start: (sym.range.start_line as i64).saturating_add(1),
        line_end: (sym.range.end_line as i64).saturating_add(1),
        signature: sym.signature,
        parent_name: None,
        params: None,
        return_type: None,
        modifiers: None,
    }
}

pub(super) fn lightweight_extract(
    ext: &str,
    content: &str,
) -> (Vec<SymbolRecord>, Vec<Reference>, Vec<String>) {
    let mut symbols = Vec::new();
    let mut refs = Vec::new();
    let mut imports = Vec::new();
    let patterns = match ext {
        "py" => vec![
            (&*PY_FUNC_RE, "func"),
            (&*PY_CLASS_RE, "class"),
        ],
        "sql" => vec![
            (&*SQL_CLASS_RE, "class"),
        ],
        "js" | "cjs" | "mjs" => vec![
            (&*JS_FUNC_RE, "func"),
            (&*JS_CLASS_RE, "class"),
            (&*JS_VAR_FUNC_RE, "func"),
        ],
        "ps1" | "sh" => vec![
            (&*SH_FUNC_RE, "func"),
        ],
        _ => Vec::new(),
    };
    for (line_idx, line) in content.lines().enumerate() {
        for (re, kind) in &patterns {
            if let Some(cap) = re.captures(line) {
                let name = cap.get(1).map(|m| m.as_str()).unwrap_or("").to_string();
                if !name.is_empty() {
                    symbols.push(SymbolRecord {
                        name,
                        kind: kind.to_string(),
                        line_start: (line_idx as i64).saturating_add(1),
                        line_end: (line_idx as i64).saturating_add(1),
                        signature: line.trim().to_string(),
                        parent_name: None,
                        params: None,
                        return_type: None,
                        modifiers: None,
                    });
                }
            }
        }
        for cap in IMPORT_RE.captures_iter(line) {
            if let Some(name) = cap.get(1) {
                imports.push(name.as_str().to_string());
            }
        }
        for cap in CALL_RE.captures_iter(line) {
            if let Some(name) = cap.get(1) {
                refs.push(Reference {
                    name: name.as_str().to_string(),
                    range: SymbolRange {
                        start_byte: 0,
                        end_byte: 0,
                        start_line: line_idx,
                        end_line: line_idx,
                    },
                });
            }
        }
    }
    (symbols, refs, imports)
}

fn extract_imports(ext: &str, content: &str) -> Vec<String> {
    let re = if ext == "rs" {
        &*RS_IMPORT_RE
    } else {
        &*OTHER_IMPORT_RE
    };
    content
        .lines()
        .filter_map(|line| {
            re.captures(line)
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().to_string())
        })
        .collect()
}

fn language_for_ext(ext: &str) -> &'static str {
    match ext {
        "rs" => "rust",
        "ts" => "typescript",
        "tsx" => "tsx",
        "py" => "python",
        "sql" => "sql",
        "ps1" => "powershell",
        "sh" => "bash",
        _ => "javascript",
    }
}

fn is_test_path(path: &str) -> bool {
    let lower = path.to_lowercase();
    lower.contains(".test.")
        || lower.contains("_test.")
        || lower.contains("_tests.")
        || lower.contains("/tests/")
        || lower.ends_with("tests.rs")
}

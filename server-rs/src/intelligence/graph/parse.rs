//! @docs ARCHITECTURE:Core
//!
//! ### AI Assist Note
//! **parse**: Core technical resource for the Tadpole OS infrastructure.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Unhandled errors, lock contention, or connection staling.
//! - **Telemetry Link**: Search `[parse]` in tracing logs.
//! - **Trace Scope**: `server-rs::intelligence::graph::parse`
//! 
//! Parallel file parsing service for the `CodeSymbolGraph`.
//!
//! Uses `rayon` for parallel I/O + AST extraction while applying TOCTOU
//! symlink boundary checks before each file read.

use std::path::{Path, PathBuf};
use rayon::prelude::*;

use crate::utils::parser::SymbolExtractor;
use crate::intelligence::graph::{
    error::GraphError,
    path::{to_unix_path, sanitize_log_path},
};

/// Service trait to parse a batch of files into symbols and references.
pub trait CodeParser: Send + Sync {
    fn parse_files(
        &self,
        files: &[PathBuf],
        root: &Path,
    ) -> Result<Vec<(PathBuf, String, Option<(Vec<crate::utils::parser::Symbol>, Vec<crate::utils::parser::Reference>, std::time::SystemTime, u64)>)>, GraphError>;
}

/// Default implementation: parallel parse with `rayon`, enforcing workspace
/// boundary checks (TOCTOU mitigation) before each file read.
pub struct CodeParsingService;

impl CodeParser for CodeParsingService {
    fn parse_files(
        &self,
        files: &[PathBuf],
        root: &Path,
    ) -> Result<Vec<(PathBuf, String, Option<(Vec<crate::utils::parser::Symbol>, Vec<crate::utils::parser::Reference>, std::time::SystemTime, u64)>)>, GraphError> {
        let canonical_root = root.canonicalize().map_err(|e| {
            GraphError::WorkspaceRootNotFound(format!(
                "Failed to canonicalize root {}: {}",
                root.display(),
                e
            ))
        })?;

        let updates: Result<Vec<_>, GraphError> = files
            .par_iter()
            .map_init(SymbolExtractor::new, |extractor, path| -> Result<_, GraphError> {
                // 🛡️ Time-check boundary verification before reading (Mitigates symlink TOCTOU races)
                let canonical_path = path.canonicalize().map_err(GraphError::Io)?;

                if !canonical_path.starts_with(&canonical_root) {
                    return Err(GraphError::PathOutOfBounds(format!(
                        "Security Violation: Path {} points outside workspace root {}",
                        sanitize_log_path(&canonical_path, root),
                        sanitize_log_path(&canonical_root, root)
                    )));
                }

                let rel_path = match path.strip_prefix(root) {
                    Ok(rel) => to_unix_path(rel),
                    Err(_) => {
                        return Err(GraphError::PathOutOfBounds(format!(
                            "Path integrity lost: {} is not inside root {}",
                            path.display(),
                            root.display()
                        )));
                    }
                };

                match std::fs::read_to_string(path) {
                    Ok(content) => {
                        let symbols = extractor.extract_symbols(path, &content);
                        let refs = extractor.extract_references(path, &content);
                        if let Ok(m) = std::fs::metadata(path) {
                            if let (Ok(mtime), size) = (m.modified(), m.len()) {
                                return Ok((
                                    path.clone(),
                                    rel_path,
                                    Some((symbols, refs, mtime, size)),
                                ));
                            }
                        }
                        Ok((path.clone(), rel_path, None))
                    }
                    Err(e) => {
                        // Return the I/O error to fail the build synchronously
                        Err(GraphError::Io(e))
                    }
                }
            })
            .collect();
        updates
    }
}

// Metadata: [parse]

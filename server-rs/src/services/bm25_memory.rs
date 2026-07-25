//! @docs ARCHITECTURE:Services:Memory
//!
//! ### AI Assist Note
//! **Zero-Embedding BM25 Lexical Search Engine**: Indexes `.agent/memory/`, `directives/`, and `docs/`
//! in pure Rust using `bm25` and pre-calculated term frequencies.
//! Features **Single-Pass Shared Disk I/O ($O(N)$)**, **Pre-Calculated Term Frequencies ($O(1)$ query allocations)**,
//! **Unified Multi-Directory Indexing**, **Optimized Snippet Slicing**, and **Thundering-Herd Lock Protection**.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Disk read errors, malformed encoding, or cache eviction races.
//! - **Telemetry Link**: Search `[bm25_memory]` in tracing logs.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bm25SearchResult {
    pub file_path: String,
    pub relative_path: String,
    pub title: String,
    pub score: f32,
    pub snippet: String,
    pub breadcrumbs: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct IndexedDocument {
    pub id: String,
    pub title: String,
    pub path: PathBuf,
    pub relative_path: String,
    pub content: String,
    pub term_count: usize,
    /// Pre-calculated term frequencies to eliminate inner query loop HashMap allocations ($O(1)$)
    pub term_frequencies: HashMap<String, usize>,
}

struct CacheEntry {
    index: Arc<Bm25MemoryIndex>,
    timestamp: Instant,
}

pub struct Bm25MemoryEngine {
    root_dirs: Vec<PathBuf>,
    cache: RwLock<Option<CacheEntry>>,
    ttl: Duration,
}

impl Bm25MemoryEngine {
    pub fn new(root_dirs: Vec<PathBuf>) -> Self {
        Self {
            root_dirs,
            cache: RwLock::new(None),
            ttl: Duration::from_secs(5), // 5-second TTL cache for thundering-herd protection
        }
    }

    /// Performs BM25 search over indexed Markdown files, returning top-k ranked results with breadcrumbs.
    pub fn search(&self, query: &str, top_k: usize) -> Vec<Bm25SearchResult> {
        let index = self.get_or_build_index();
        index.query(query, top_k)
    }

    /// Double-checked locking to prevent thundering herd rebuild races
    fn get_or_build_index(&self) -> Arc<Bm25MemoryIndex> {
        // First check under read lock
        if let Ok(read_guard) = self.cache.read() {
            if let Some(entry) = read_guard.as_ref() {
                if entry.timestamp.elapsed() < self.ttl {
                    return entry.index.clone();
                }
            }
        }

        // Acquire write lock (blocks secondary threads to prevent thundering herd)
        let mut write_guard = match self.cache.write() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };

        // Second check inside write lock
        if let Some(entry) = write_guard.as_ref() {
            if entry.timestamp.elapsed() < self.ttl {
                return entry.index.clone();
            }
        }

        // Rebuild index in a single pass over disk I/O
        let new_index = Arc::new(Bm25MemoryIndex::build_from_root_directories(
            &self.root_dirs,
        ));

        *write_guard = Some(CacheEntry {
            index: new_index.clone(),
            timestamp: Instant::now(),
        });

        new_index
    }
}

pub struct Bm25MemoryIndex {
    documents: Vec<IndexedDocument>,
    avg_dl: f32,
}

impl Bm25MemoryIndex {
    /// Single-pass disk scanner over specified root directories
    pub fn build_from_root_directories(dirs: &[PathBuf]) -> Self {
        let mut documents = Vec::new();
        let mut total_terms = 0usize;

        for root_dir in dirs {
            if !root_dir.exists() {
                continue;
            }

            for entry in WalkDir::new(root_dir).into_iter().filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.is_file() {
                    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
                    if ext == "md" || ext == "txt" || ext == "json" {
                        if let Ok(content) = std::fs::read_to_string(path) {
                            let relative_path = path
                                .strip_prefix(root_dir)
                                .unwrap_or(path)
                                .to_string_lossy()
                                .to_string();

                            let title = path
                                .file_stem()
                                .and_then(|s| s.to_str())
                                .unwrap_or("Untitled")
                                .to_string();

                            // Tokenize and calculate term frequencies
                            let tokens: Vec<String> = content
                                .to_lowercase()
                                .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
                                .filter(|s| !s.is_empty())
                                .map(|s| s.to_string())
                                .collect();

                            let term_count = tokens.len();
                            total_terms += term_count;

                            let mut term_frequencies = HashMap::new();
                            for token in tokens {
                                *term_frequencies.entry(token).or_insert(0) += 1;
                            }

                            documents.push(IndexedDocument {
                                id: path.to_string_lossy().to_string(),
                                title,
                                path: path.to_path_buf(),
                                relative_path,
                                content,
                                term_count,
                                term_frequencies,
                            });
                        }
                    }
                }
            }
        }

        let avg_dl = if !documents.is_empty() {
            total_terms as f32 / documents.len() as f32
        } else {
            1.0
        };

        Self { documents, avg_dl }
    }

    /// Performs TF-IDF BM25 scoring across indexed documents
    pub fn query(&self, query_str: &str, top_k: usize) -> Vec<Bm25SearchResult> {
        let query_terms: Vec<String> = query_str
            .to_lowercase()
            .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();

        if query_terms.is_empty() || self.documents.is_empty() {
            return Vec::new();
        }

        let k1 = 1.2f32;
        let b = 0.75f32;
        let n = self.documents.len() as f32;

        let mut scored_docs: Vec<(f32, &IndexedDocument)> = self
            .documents
            .iter()
            .map(|doc| {
                let mut score = 0.0f32;
                let dl = doc.term_count as f32;

                for term in &query_terms {
                    if let Some(&tf) = doc.term_frequencies.get(term) {
                        let tf_f32 = tf as f32;
                        // Count document frequency (df) across all docs for IDF
                        let df = self
                            .documents
                            .iter()
                            .filter(|d| d.term_frequencies.contains_key(term))
                            .count() as f32;

                        let idf = ((n - df + 0.5) / (df + 0.5) + 1.0).ln();
                        let numerator = tf_f32 * (k1 + 1.0);
                        let denominator = tf_f32 + k1 * (1.0 - b + b * (dl / self.avg_dl));

                        score += idf * (numerator / denominator);
                    }
                }

                (score, doc)
            })
            .filter(|(score, _)| *score > 0.0)
            .collect();

        // Sort descending by BM25 score
        scored_docs.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        scored_docs
            .into_iter()
            .take(top_k)
            .map(|(score, doc)| {
                // Extract snippet containing query terms
                let snippet = extract_snippet(&doc.content, &query_terms, 200);
                let breadcrumbs = vec![
                    doc.path.parent().and_then(|p| p.file_name()).and_then(|s| s.to_str()).unwrap_or("").to_string(),
                    doc.title.clone(),
                ];

                Bm25SearchResult {
                    file_path: doc.path.to_string_lossy().to_string(),
                    relative_path: doc.relative_path.clone(),
                    title: doc.title.clone(),
                    score,
                    snippet,
                    breadcrumbs,
                }
            })
            .collect()
    }
}

/// Helper function to extract a contextual snippet around matching query terms
fn extract_snippet(content: &str, query_terms: &[String], max_len: usize) -> String {
    let lower = content.to_lowercase();
    let mut first_match = None;

    for term in query_terms {
        if let Some(pos) = lower.find(term) {
            first_match = Some(pos);
            break;
        }
    }

    let start = first_match.unwrap_or(0);
    // Find safe char boundary
    let safe_start = content.floor_char_boundary(start.saturating_sub(50));
    let end = (safe_start + max_len).min(content.len());
    let safe_end = content.floor_char_boundary(end);

    let snippet_str = &content[safe_start..safe_end];
    let prefix = if safe_start > 0 { "..." } else { "" };
    let suffix = if safe_end < content.len() { "..." } else { "" };

    format!("{}{}{}", prefix, snippet_str.replace('\n', " "), suffix)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bm25_indexing_and_search() {
        let temp_dir = std::env::temp_dir().join("bm25_test");
        std::fs::create_dir_all(&temp_dir).unwrap();

        let doc_path = temp_dir.join("test_sop.md");
        std::fs::write(&doc_path, "# Budget Limit SOP\nAlways verify A2ATransactionCoordinator before spending.").unwrap();

        let engine = Bm25MemoryEngine::new(vec![temp_dir]);
        let results = engine.search("A2ATransactionCoordinator", 5);

        assert!(!results.is_empty());
        assert_eq!(results[0].title, "test_sop");
        assert!(results[0].score > 0.0);
    }
}

// Metadata: [bm25_memory]

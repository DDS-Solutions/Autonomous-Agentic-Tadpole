//! @docs ARCHITECTURE:Services:RAG
//!
//! ### AI Assist Note
//! **Reciprocal Rank Fusion (RRF) RAG Engine**: Unifies the Hybrid RAG Triad
//! (LanceDB Vector + BM25 Lexical + TrustGraph Entity/GraphRAG) into a single,
//! highly calibrated ranking. Eliminates prompt context dilution and boosts
//! search precision.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Score tie resolution, empty candidate lists, or identifier collisions.
//! - **Telemetry Link**: Search `[rag_fusion]` in tracing logs.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A retrieval candidate from an individual search engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagCandidate {
    pub id: String,
    pub title: String,
    pub content: String,
    pub relative_path: Option<String>,
    pub source: String,
    pub metadata: Option<serde_json::Value>,
}

/// Configuration weights for the Hybrid RAG Triad.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagEngineWeights {
    pub vector_weight: f32,
    pub bm25_weight: f32,
    pub trustgraph_weight: f32,
    pub k_constant: f32,
}

impl Default for RagEngineWeights {
    fn default() -> Self {
        Self {
            vector_weight: 0.40,
            bm25_weight: 0.35,
            trustgraph_weight: 0.25,
            k_constant: 60.0,
        }
    }
}

/// A unified, deduplicated and score-calibrated search result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FusedSearchResult {
    pub id: String,
    pub title: String,
    pub content: String,
    pub relative_path: Option<String>,
    pub sources: Vec<String>,
    pub rrf_score: f32,
    pub metadata: Option<serde_json::Value>,
}

struct MergedCandidateAccumulator {
    id: String,
    title: String,
    content: String,
    relative_path: Option<String>,
    sources: Vec<String>,
    total_rrf_score: f32,
    metadata: Option<serde_json::Value>,
}

/// Fuses ranked results from Vector, BM25, and TrustGraph engines using Reciprocal Rank Fusion.
///
/// Formula:
/// $$RRF(d) = \sum_{e \in \{Vector, BM25, Graph\}} \frac{w_e}{k + rank_e(d)}$$
pub fn fuse_search_results(
    vector_results: &[RagCandidate],
    bm25_results: &[RagCandidate],
    graph_results: &[RagCandidate],
    weights: &RagEngineWeights,
    top_k: usize,
) -> Vec<FusedSearchResult> {
    let mut accumulator_map: HashMap<String, MergedCandidateAccumulator> = HashMap::new();

    let engines: [(&[RagCandidate], f32, &str); 3] = [
        (vector_results, weights.vector_weight, "vector"),
        (bm25_results, weights.bm25_weight, "bm25"),
        (graph_results, weights.trustgraph_weight, "trustgraph"),
    ];

    for (candidate_list, engine_weight, default_engine_name) in engines {
        for (index, candidate) in candidate_list.iter().enumerate() {
            let rank = (index + 1) as f32; // 1-indexed rank
            let rrf_component = engine_weight / (weights.k_constant + rank);
            let engine_name = if candidate.source.is_empty() {
                default_engine_name.to_string()
            } else {
                candidate.source.clone()
            };

            accumulator_map
                .entry(candidate.id.clone())
                .and_modify(|acc| {
                    acc.total_rrf_score += rrf_component;
                    if !acc.sources.contains(&engine_name) {
                        acc.sources.push(engine_name.clone());
                    }
                    if acc.content.is_empty() && !candidate.content.is_empty() {
                        acc.content = candidate.content.clone();
                    }
                    if acc.title.is_empty() && !candidate.title.is_empty() {
                        acc.title = candidate.title.clone();
                    }
                    if acc.relative_path.is_none() && candidate.relative_path.is_some() {
                        acc.relative_path = candidate.relative_path.clone();
                    }
                })
                .or_insert_with(|| MergedCandidateAccumulator {
                    id: candidate.id.clone(),
                    title: candidate.title.clone(),
                    content: candidate.content.clone(),
                    relative_path: candidate.relative_path.clone(),
                    sources: vec![engine_name],
                    total_rrf_score: rrf_component,
                    metadata: candidate.metadata.clone(),
                });
        }
    }

    let mut fused: Vec<FusedSearchResult> = accumulator_map
        .into_values()
        .map(|acc| FusedSearchResult {
            id: acc.id,
            title: acc.title,
            content: acc.content,
            relative_path: acc.relative_path,
            sources: acc.sources,
            rrf_score: acc.total_rrf_score,
            metadata: acc.metadata,
        })
        .collect();

    // Sort descending by RRF score
    fused.sort_by(|a, b| {
        b.rrf_score
            .partial_cmp(&a.rrf_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    if fused.len() > top_k {
        fused.truncate(top_k);
    }

    fused
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rrf_deduplication_boosts_intersecting_items() {
        let weights = RagEngineWeights::default();

        let item_a = RagCandidate {
            id: "doc_a".to_string(),
            title: "Kernel Architecture".to_string(),
            content: "Rust Axum engine details".to_string(),
            relative_path: Some("docs/ARCHITECTURE.md".to_string()),
            source: "vector".to_string(),
            metadata: None,
        };

        let item_b = RagCandidate {
            id: "doc_b".to_string(),
            title: "Actor Supervision".to_string(),
            content: "OTP supervisor patterns".to_string(),
            relative_path: Some("docs/ACTORS.md".to_string()),
            source: "bm25".to_string(),
            metadata: None,
        };

        let item_a_bm25 = RagCandidate {
            id: "doc_a".to_string(),
            title: "Kernel Architecture".to_string(),
            content: "Rust Axum engine details".to_string(),
            relative_path: Some("docs/ARCHITECTURE.md".to_string()),
            source: "bm25".to_string(),
            metadata: None,
        };

        let vector_list = vec![item_a.clone()];
        // In BM25, item_b is rank 1, item_a is rank 2
        let bm25_list = vec![item_b.clone(), item_a_bm25];
        let graph_list = vec![];

        let results = fuse_search_results(&vector_list, &bm25_list, &graph_list, &weights, 10);

        assert_eq!(results.len(), 2);
        // doc_a appeared in BOTH Vector (#1) and BM25 (#2), so its combined score should beat doc_b (only BM25 #1)
        assert_eq!(results[0].id, "doc_a");
        assert_eq!(results[1].id, "doc_b");
        assert_eq!(results[0].sources.len(), 2);
        assert!(results[0].sources.contains(&"vector".to_string()));
        assert!(results[0].sources.contains(&"bm25".to_string()));
    }

    #[test]
    fn test_rrf_top_k_truncation() {
        let weights = RagEngineWeights::default();
        let mut candidates = Vec::new();
        for i in 1..=10 {
            candidates.push(RagCandidate {
                id: format!("doc_{}", i),
                title: format!("Doc {}", i),
                content: format!("Content {}", i),
                relative_path: None,
                source: "bm25".to_string(),
                metadata: None,
            });
        }

        let results = fuse_search_results(&[], &candidates, &[], &weights, 3);
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].id, "doc_1");
        assert_eq!(results[1].id, "doc_2");
        assert_eq!(results[2].id, "doc_3");
    }
}

// Metadata: [rag_fusion]

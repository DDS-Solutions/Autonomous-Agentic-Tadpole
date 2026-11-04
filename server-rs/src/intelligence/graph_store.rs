//! @docs ARCHITECTURE:CodeBaseIntelligence
//!
//! ### AI Assist Note
//! **Persistent Graph Store**: Public refresh facade and graph snapshot record types.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Database write errors or graph refresh task failures.
//! - **Telemetry Link**: Search [graph_store] in tracing logs.

use crate::error::AppError;
use crate::utils::parser::Reference;
use serde::Serialize;
use std::path::PathBuf;

mod analysis;
mod db;
mod scan;
mod snapshot;

#[cfg(test)]
use scan::lightweight_extract;

#[derive(Debug, Clone, Serialize)]
pub struct GraphDbRefreshSummary {
    pub db_path: PathBuf,
    pub node_count: usize,
    pub edge_count: usize,
    pub risk_count: usize,
    pub community_count: usize,
    pub flow_count: usize,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct CommunityRule {
    pub pattern: String,
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SymbolKind {
    File,
    Class,
    Function,
    Test,
}

impl SymbolKind {
    fn as_str(&self) -> &'static str {
        match self {
            SymbolKind::File => "File",
            SymbolKind::Class => "Class",
            SymbolKind::Function => "Function",
            SymbolKind::Test => "Test",
        }
    }
}

#[derive(Debug, Clone)]
struct FileRecord {
    absolute_path: String,
    relative_path: String,
    name: String,
    language: String,
    is_test: bool,
    symbols: Vec<SymbolRecord>,
    refs: Vec<Reference>,
    imports: Vec<String>,
    file_hash: String,
}

#[derive(Debug, Clone)]
struct SymbolRecord {
    name: String,
    kind: String,
    line_start: i64,
    line_end: i64,
    signature: String,
    parent_name: Option<String>,
    params: Option<String>,
    return_type: Option<String>,
    modifiers: Option<String>,
}

#[derive(Debug, Clone)]
struct NodeRow {
    id: i64,
    kind: String,
    name: String,
    qualified_name: String,
    file_path: String,
    line_start: Option<i64>,
    line_end: Option<i64>,
    language: String,
    parent_name: Option<String>,
    params: Option<String>,
    return_type: Option<String>,
    modifiers: Option<String>,
    is_test: bool,
    file_hash: String,
    extra: String,
    signature: String,
    community_id: Option<i64>,
}

#[derive(Debug, Clone, Eq, Hash, PartialEq)]
struct EdgeRow {
    kind: String,
    source_qualified: String,
    target_qualified: String,
    file_path: String,
    line: i64,
    extra: String,
}

#[derive(Debug, Clone)]
struct RiskRow {
    node_id: i64,
    qualified_name: String,
    risk_score: f64,
    caller_count: i64,
    test_coverage: String,
    security_relevant: bool,
}

#[derive(Debug, Clone)]
struct CommunityRow {
    id: i64,
    name: String,
    cohesion: f64,
    size: i64,
    dominant_language: String,
    description: String,
    risk: String,
}

#[derive(Debug, Clone)]
struct FlowRow {
    id: i64,
    name: String,
    entry_point_id: i64,
    entry_point: String,
    depth: i64,
    node_count: i64,
    node_ids: Vec<i64>,
    critical_path: Vec<String>,
    criticality: f64,
    file_count: i64,
}

#[derive(Debug, Clone)]
struct GraphSnapshot {
    root: PathBuf,
    nodes: Vec<NodeRow>,
    edges: Vec<EdgeRow>,
    risks: Vec<RiskRow>,
    communities: Vec<CommunityRow>,
    flows: Vec<FlowRow>,
}

pub async fn refresh_code_review_graph_db(
    root: PathBuf,
    db_path: PathBuf,
    salt: String,
    git_path: Option<PathBuf>,
) -> Result<GraphDbRefreshSummary, AppError> {
    tracing::info!("🔄 [graph_store] Refreshing persistent graph database...");
    let root = root
        .canonicalize()
        .map_err(|e| AppError::InternalServerError(format!("failed to resolve graph root: {e}")))?;
    let build_root = root.clone();
    let snapshot = tokio::task::spawn_blocking(move || snapshot::build_snapshot(build_root, &salt))
        .await
        .map_err(|e| {
            AppError::InternalServerError(format!("graph DB refresh task panicked: {e}"))
        })??;

    if let Some(parent) = db_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let pool = db::open_graph_pool(&db_path).await?;
    db::ensure_schema(&pool).await?;
    db::write_snapshot(&pool, &snapshot, git_path.as_deref()).await?;
    pool.close().await;

    Ok(GraphDbRefreshSummary {
        db_path,
        node_count: snapshot.nodes.len(),
        edge_count: snapshot.edges.len(),
        risk_count: snapshot.risks.len(),
        community_count: snapshot.communities.len(),
        flow_count: snapshot.flows.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::Row;
    use tempfile::tempdir;

    #[tokio::test]
    async fn finding_regression_graph_links_module_qualified_generic_rust_call() {
        let dir = tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/lib.rs"),
            "mod persistence;\nfn caller() { crate::persistence::save_agent_db::<u32>(); }\n").unwrap();
        std::fs::write(dir.path().join("src/persistence.rs"),
            "pub fn save_agent_db<T>() {}\n").unwrap();
        let database = dir.path().join(".code-review-graph/graph.db");
        refresh_code_review_graph_db(dir.path().to_path_buf(), database.clone(), "qualified-call-regression".to_string(), None)
            .await.unwrap();
        let pool = db::open_graph_pool(&database).await.unwrap();
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM edges edge JOIN nodes caller ON caller.qualified_name = edge.source_qualified JOIN nodes callee ON callee.qualified_name = edge.target_qualified WHERE edge.kind = 'CALLS' AND caller.name = 'caller' AND callee.name = 'save_agent_db'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(count, 1, "cross-module generic call must reach the actual definition");
    }

    #[tokio::test]
    async fn refresh_creates_idempotent_graph_db() {
        let dir = tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(
            dir.path().join("src/lib.rs"),
            "fn helper() {}\nfn main() { helper(); }\n",
        )
        .unwrap();
        let db = dir.path().join(".code-review-graph/graph.db");
        let first = refresh_code_review_graph_db(
            dir.path().to_path_buf(),
            db.clone(),
            "test-salt".to_string(),
            None,
        )
        .await
        .unwrap();
        let second = refresh_code_review_graph_db(
            dir.path().to_path_buf(),
            db.clone(),
            "test-salt".to_string(),
            None,
        )
        .await
        .unwrap();
        assert_eq!(first.node_count, second.node_count);
        assert_eq!(first.edge_count, second.edge_count);
    }

    #[test]
    fn lightweight_python_extraction_indexes_async_functions() {
        let (symbols, _, _) = lightweight_extract("py", "async def execute_tool(args):\n    return await worker(args)\n\ndef helper():\n    pass\n");
        assert_eq!(symbols.iter().map(|symbol| symbol.name.as_str()).collect::<Vec<_>>(), ["execute_tool", "helper"]);
        assert_eq!(symbols[0].line_start, 1);
        assert_eq!(symbols[1].line_start, 4);
    }

    #[tokio::test]
    async fn refresh_removes_deleted_symbols() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("a.rs"), "fn alpha() {}\n").unwrap();
        std::fs::write(dir.path().join("b.rs"), "fn beta() { alpha(); }\n").unwrap();
        let db = dir.path().join(".code-review-graph/graph.db");
        refresh_code_review_graph_db(dir.path().to_path_buf(), db.clone(), "salt".to_string(), None)
            .await
            .unwrap();
        std::fs::remove_file(dir.path().join("b.rs")).unwrap();
        refresh_code_review_graph_db(dir.path().to_path_buf(), db.clone(), "salt".to_string(), None)
            .await
            .unwrap();
        let pool = db::open_graph_pool(&db).await.unwrap();
        let count: i64 = sqlx::query("SELECT COUNT(*) FROM nodes WHERE name = 'beta'")
            .fetch_one(&pool)
            .await
            .unwrap()
            .get(0);
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn refresh_handles_duplicate_symbol_names_in_same_file() {
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("lib.rs"),
            "struct AppError;\nimpl AppError { fn one() {} }\nimpl AppError { fn two() {} }\n",
        )
        .unwrap();
        let db = dir.path().join(".code-review-graph/graph.db");
        refresh_code_review_graph_db(dir.path().to_path_buf(), db.clone(), "salt".to_string(), None)
            .await
            .unwrap();

        let pool = db::open_graph_pool(&db).await.unwrap();
        let count: i64 =
            sqlx::query("SELECT COUNT(DISTINCT qualified_name) FROM nodes WHERE name = 'AppError'")
                .fetch_one(&pool)
                .await
                .unwrap()
                .get(0);
        assert_eq!(count, 3);
    }

    #[tokio::test]
    async fn risk_index_marks_security_and_fts_rebuilds() {
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("auth.rs"),
            "fn validate_token() {}\nfn caller() { validate_token(); }\n",
        )
        .unwrap();
        let db = dir.path().join(".code-review-graph/graph.db");
        refresh_code_review_graph_db(dir.path().to_path_buf(), db.clone(), "salt".to_string(), None)
            .await
            .unwrap();
        let pool = db::open_graph_pool(&db).await.unwrap();
        let security: i64 = sqlx::query(
            "SELECT security_relevant FROM risk_index WHERE qualified_name LIKE '%validate_token%' LIMIT 1",
        )
        .fetch_one(&pool)
        .await
        .unwrap()
        .get(0);
        let fts_count: i64 =
            sqlx::query("SELECT COUNT(*) FROM nodes_fts WHERE nodes_fts MATCH 'validate_token'")
                .fetch_one(&pool)
                .await
                .unwrap()
                .get(0);
        assert_eq!(security, 1);
        assert!(fts_count > 0);
    }
}

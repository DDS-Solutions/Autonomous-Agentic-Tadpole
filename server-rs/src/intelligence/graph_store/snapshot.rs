//! @docs ARCHITECTURE:CodeBaseIntelligence
//!
//! ### AI Assist Note
//! **Graph Snapshot Builder**: Converts scan records into stable graph nodes and edges.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Snapshot assembly or configured community analysis failures.
//! - **Telemetry Link**: Search [graph_store] in tracing logs.

use super::*;
use super::analysis::{
    assign_communities, build_communities, build_flows, build_risks, default_community_rules,
};
use super::scan::scan_files;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

pub(super) fn build_snapshot(root: PathBuf, salt: &str) -> Result<GraphSnapshot, AppError> {
    let files = scan_files(&root, salt)?;

    let mut nodes = Vec::new();
    let mut next_id = 1i64;
    for file in &files {
        nodes.push(NodeRow {
            id: next_id,
            kind: SymbolKind::File.as_str().to_string(),
            name: file.name.clone(),
            qualified_name: file.absolute_path.clone(),
            file_path: file.absolute_path.clone(),
            line_start: Some(1),
            line_end: None,
            language: file.language.clone(),
            parent_name: None,
            params: None,
            return_type: None,
            modifiers: None,
            is_test: file.is_test,
            file_hash: file.file_hash.clone(),
            extra: serde_json::json!({ "relative_path": file.relative_path }).to_string(),
            signature: file.relative_path.clone(),
            community_id: None,
        });
        next_id = next_id.saturating_add(1);
        for sym in &file.symbols {
            let qualified_name = qualified_symbol(&file.absolute_path, sym);
            nodes.push(NodeRow {
                id: next_id,
                kind: normalize_kind(&sym.kind, file.is_test).as_str().to_string(),
                name: sym.name.clone(),
                qualified_name,
                file_path: file.absolute_path.clone(),
                line_start: Some(sym.line_start),
                line_end: Some(sym.line_end),
                language: file.language.clone(),
                parent_name: sym.parent_name.clone(),
                params: sym.params.clone(),
                return_type: sym.return_type.clone(),
                modifiers: sym.modifiers.clone(),
                is_test: file.is_test,
                file_hash: file.file_hash.clone(),
                extra: "{}".to_string(),
                signature: sym.signature.clone(),
                community_id: None,
            });
            next_id = next_id.saturating_add(1);
        }
    }

    let mut by_name: HashMap<Arc<str>, Vec<Arc<str>>> = HashMap::new();
    let mut file_nodes = HashMap::new();
    for node in &nodes {
        let qn_arc: Arc<str> = node.qualified_name.as_str().into();
        let name_arc: Arc<str> = node.name.as_str().into();
        let file_path_arc: Arc<str> = node.file_path.as_str().into();
        if node.kind == "File" {
            file_nodes.insert(file_path_arc, qn_arc);
        } else {
            by_name
                .entry(name_arc)
                .or_default()
                .push(qn_arc);
        }
    }

    let mut edges = HashSet::new();
    for file in &files {
        let file_path_arc: Arc<str> = file.absolute_path.as_str().into();
        let source_file_qn = file_nodes
            .get(&file_path_arc)
            .map(|s| s.to_string())
            .unwrap_or_else(|| file.absolute_path.clone());

        if let Some(file_qn) = file_nodes.get(&file_path_arc) {
            for sym in &file.symbols {
                edges.insert(EdgeRow {
                    kind: "CONTAINS".to_string(),
                    source_qualified: file_qn.to_string(),
                    target_qualified: qualified_symbol(&file.absolute_path, sym),
                    file_path: file.absolute_path.clone(),
                    line: sym.line_start,
                    extra: "{}".to_string(),
                });
            }
        }

        for import in &file.imports {
            for target in match_targets(import, &by_name) {
                edges.insert(EdgeRow {
                    kind: "IMPORTS_FROM".to_string(),
                    source_qualified: source_file_qn.clone(),
                    target_qualified: target.to_string(),
                    file_path: file.absolute_path.clone(),
                    line: 0,
                    extra: serde_json::json!({ "import": import }).to_string(),
                });
            }
        }

        for reference in &file.refs {
            let Some(source_sym) = tightest_symbol(file, reference) else {
                continue;
            };
            let source = qualified_symbol(&file.absolute_path, source_sym);
            for target in match_targets(&reference.name, &by_name) {
                if target.as_ref() == source.as_str() {
                    continue;
                }
                let kind = if file.is_test { "TESTED_BY" } else { "CALLS" };
                edges.insert(EdgeRow {
                    kind: kind.to_string(),
                    source_qualified: source.clone(),
                    target_qualified: target.to_string(),
                    file_path: file.absolute_path.clone(),
                    line: (reference.range.start_line as i64).saturating_add(1),
                    extra: serde_json::json!({ "reference": reference.name }).to_string(),
                });
            }
        }
    }

    let mut edges = edges.into_iter().collect::<Vec<_>>();
    edges.sort_by(|a, b| {
        (
            &a.kind,
            &a.source_qualified,
            &a.target_qualified,
            &a.file_path,
            a.line,
        )
            .cmp(&(
                &b.kind,
                &b.source_qualified,
                &b.target_qualified,
                &b.file_path,
                b.line,
            ))
    });

    let config_path = root.join(".code-review-graph").join("config.json");
    let rules = if config_path.exists() {
        std::fs::read_to_string(&config_path)
            .ok()
            .and_then(|content| serde_json::from_str::<Vec<CommunityRule>>(&content).ok())
            .unwrap_or_else(default_community_rules)
    } else {
        default_community_rules()
    };

    assign_communities(&mut nodes, &rules);
    let communities = build_communities(&nodes, &edges, &rules);
    let risks = build_risks(&nodes, &edges);
    let flows = build_flows(&nodes, &edges, &risks);

    Ok(GraphSnapshot {
        root,
        nodes,
        edges,
        risks,
        communities,
        flows,
    })
}

fn tightest_symbol<'a>(file: &'a FileRecord, reference: &Reference) -> Option<&'a SymbolRecord> {
    file.symbols
        .iter()
        .filter(|sym| {
            (reference.range.start_line as i64).saturating_add(1) >= sym.line_start
                && (reference.range.end_line as i64).saturating_add(1) <= sym.line_end
        })
        .min_by_key(|sym| sym.line_end - sym.line_start)
}

fn match_targets(name: &str, by_name: &HashMap<Arc<str>, Vec<Arc<str>>>) -> Vec<Arc<str>> {
    let direct = name.rsplit([':', '/', '.', '\\']).next().unwrap_or(name);
    by_name.get(direct).cloned().unwrap_or_default()
}

fn normalize_kind(kind: &str, is_test: bool) -> SymbolKind {
    if is_test {
        return SymbolKind::Test;
    }
    match kind {
        "struct" | "enum" | "trait" | "class" | "interface" | "type" | "impl" => SymbolKind::Class,
        _ => SymbolKind::Function,
    }
}

fn qualified(file_path: &str, symbol: &str) -> String {
    format!("{file_path}::{symbol}")
}

fn qualified_symbol(file_path: &str, symbol: &SymbolRecord) -> String {
    format!(
        "{}@{}-{}",
        qualified(file_path, &symbol.name),
        symbol.line_start,
        symbol.line_end
    )
}

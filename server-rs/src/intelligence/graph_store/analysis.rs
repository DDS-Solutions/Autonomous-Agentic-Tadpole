//! @docs ARCHITECTURE:CodeBaseIntelligence
//!
//! ### AI Assist Note
//! **Graph Analysis**: Assigns communities and computes risk and flow summaries.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Heuristic scoring or graph traversal edge cases.
//! - **Telemetry Link**: Search [graph_store] in tracing logs.

use super::*;
use std::collections::{HashMap, HashSet, VecDeque};

pub(super) fn default_community_rules() -> Vec<CommunityRule> {
    vec![
        CommunityRule {
            pattern: "/server-rs/".to_string(),
            id: 1,
            name: "server-rs-core".to_string(),
        },
        CommunityRule {
            pattern: "/src-tauri/".to_string(),
            id: 2,
            name: "tauri-shell".to_string(),
        },
        CommunityRule {
            pattern: "/src/".to_string(),
            id: 3,
            name: "frontend-app".to_string(),
        },
        CommunityRule {
            pattern: "/execution/".to_string(),
            id: 4,
            name: "execution-tools".to_string(),
        },
        CommunityRule {
            pattern: "/.agent/".to_string(),
            id: 5,
            name: "agent-assets".to_string(),
        },
        CommunityRule {
            pattern: "/data/".to_string(),
            id: 6,
            name: "data-migrations".to_string(),
        },
        CommunityRule {
            pattern: "/migrations/".to_string(),
            id: 6,
            name: "data-migrations".to_string(),
        },
    ]
}

pub(super) fn assign_communities(nodes: &mut [NodeRow], rules: &[CommunityRule]) {
    for node in nodes {
        let rel = node.file_path.replace('\\', "/");
        let mut assigned = false;
        for rule in rules {
            if rel.contains(&rule.pattern) {
                node.community_id = Some(rule.id);
                assigned = true;
                break;
            }
        }
        if !assigned {
            node.community_id = Some(7);
        }
    }
}

pub(super) fn build_communities(nodes: &[NodeRow], edges: &[EdgeRow], rules: &[CommunityRule]) -> Vec<CommunityRow> {
    let mut by_id: HashMap<i64, Vec<&NodeRow>> = HashMap::new();
    for node in nodes {
        by_id
            .entry(node.community_id.unwrap_or(7))
            .or_default()
            .push(node);
    }
    let mut rows = Vec::new();
    for (id, group) in by_id {
        let mut langs = HashMap::<String, usize>::new();
        for node in &group {
            *langs.entry(node.language.clone()).or_default() += 1;
        }
        let dominant_language = langs
            .into_iter()
            .max_by_key(|(_, count)| *count)
            .map(|(lang, _)| lang)
            .unwrap_or_default();
        let names = group
            .iter()
            .map(|n| n.qualified_name.as_str())
            .collect::<HashSet<_>>();
        let internal = edges
            .iter()
            .filter(|e| {
                names.contains(e.source_qualified.as_str())
                    && names.contains(e.target_qualified.as_str())
            })
            .count();
        let cohesion = if group.len() <= 1 {
            0.0
        } else {
            (internal as f64 / group.len() as f64).min(1.0)
        };
        let name = rules
            .iter()
            .find(|r| r.id == id)
            .map(|r| r.name.as_str())
            .unwrap_or_else(|| {
                if id == 7 {
                    "workspace-other"
                } else {
                    "unknown-community"
                }
            });
        rows.push(CommunityRow {
            id,
            name: name.to_string(),
            cohesion,
            size: group.len() as i64,
            dominant_language,
            description: format!("{} symbols grouped by workspace area", group.len()),
            risk: "heuristic".to_string(),
        });
    }
    rows.sort_by_key(|row| row.id);
    rows
}

pub(super) fn build_risks(nodes: &[NodeRow], edges: &[EdgeRow]) -> Vec<RiskRow> {
    let tested = edges
        .iter()
        .filter(|e| e.kind == "TESTED_BY")
        .map(|e| e.target_qualified.clone())
        .collect::<HashSet<_>>();
    let mut caller_counts = HashMap::<String, i64>::new();
    for edge in edges {
        if matches!(edge.kind.as_str(), "CALLS" | "REFERENCES" | "IMPORTS_FROM") {
            *caller_counts
                .entry(edge.target_qualified.clone())
                .or_default() += 1;
        }
    }
    nodes
        .iter()
        .filter(|node| node.kind != "File")
        .map(|node| {
            let caller_count = *caller_counts.get(&node.qualified_name).unwrap_or(&0);
            let is_tested = tested.contains(&node.qualified_name) || node.is_test;
            let security_relevant = is_security_relevant(node);
            let mut score = 0.15;
            if !is_tested {
                score += 0.2;
            }
            if security_relevant {
                score += 0.3;
            }
            if caller_count > 0 {
                score += ((caller_count as f64).log10() * 0.15).min(0.15);
            }
            if node.file_path.contains("\\routes\\") || node.file_path.contains("/routes/") {
                score += 0.05;
            }
            if node.kind == "Class" {
                score += 0.02;
            }
            RiskRow {
                node_id: node.id,
                qualified_name: node.qualified_name.clone(),
                risk_score: score.min(0.85),
                caller_count,
                test_coverage: if is_tested { "tested" } else { "untested" }.to_string(),
                security_relevant,
            }
        })
        .collect()
}

pub(super) fn build_flows(nodes: &[NodeRow], edges: &[EdgeRow], risks: &[RiskRow]) -> Vec<FlowRow> {
    let risk_by_qn = risks
        .iter()
        .map(|r| (r.qualified_name.as_str(), r))
        .collect::<HashMap<_, _>>();
    let id_by_qn = nodes
        .iter()
        .map(|n| (n.qualified_name.as_str(), n.id))
        .collect::<HashMap<_, _>>();
    let mut adjacency = HashMap::<&str, Vec<&str>>::new();
    for edge in edges {
        if matches!(edge.kind.as_str(), "CALLS" | "REFERENCES" | "IMPORTS_FROM") {
            adjacency
                .entry(edge.source_qualified.as_str())
                .or_default()
                .push(edge.target_qualified.as_str());
        }
    }
    let mut entries = nodes
        .iter()
        .filter(|node| {
            node.kind != "File"
                && (node.name == "main"
                    || node.name.ends_with("_handler")
                    || node.file_path.contains("\\routes\\")
                    || node.file_path.contains("/routes/")
                    || node.file_path.contains("\\pages\\")
                    || node.file_path.contains("/pages/")
                    || node.file_path.contains("\\services\\")
                    || node.file_path.contains("/services/")
                    || node.file_path.contains("\\stores\\")
                    || node.file_path.contains("/stores/"))
        })
        .take(250)
        .collect::<Vec<_>>();
    entries.sort_by(|a, b| a.qualified_name.cmp(&b.qualified_name));

    let mut flows = Vec::new();
    for (idx, entry) in entries.into_iter().enumerate() {
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        let mut parents = HashMap::new();

        let entry_qn = entry.qualified_name.as_str();
        visited.insert(entry_qn);
        queue.push_back((entry_qn, 0i64));

        while let Some((current, depth)) = queue.pop_front() {
            if depth >= 6 {
                continue;
            }
            for next in adjacency.get(current).into_iter().flatten().take(20) {
                if visited.insert(next) {
                    parents.insert(*next, current);
                    queue.push_back((next, depth + 1));
                }
            }
        }
        let node_ids = visited
            .iter()
            .filter_map(|qn| id_by_qn.get(qn).copied())
            .collect::<Vec<_>>();
        let files = nodes
            .iter()
            .filter(|node| visited.contains(node.qualified_name.as_str()))
            .map(|node| node.file_path.as_str())
            .collect::<HashSet<_>>();
        let security_hits = visited
            .iter()
            .filter(|qn| {
                risk_by_qn
                    .get(**qn)
                    .map(|r| r.security_relevant)
                    .unwrap_or(false)
            })
            .count();
        let criticality = ((node_ids.len() as f64 * 0.015)
            + (files.len() as f64 * 0.02)
            + (security_hits as f64 * 0.1))
            .min(1.0);

        let target_node = visited
            .iter()
            .max_by(|a, b| {
                let r_a = risk_by_qn.get(*a).map(|r| r.risk_score).unwrap_or(0.0);
                let r_b = risk_by_qn.get(*b).map(|r| r.risk_score).unwrap_or(0.0);
                r_a.partial_cmp(&r_b).unwrap_or(std::cmp::Ordering::Equal)
            });

        let mut path = Vec::new();
        if let Some(mut curr) = target_node.copied() {
            path.push(curr.to_string());
            while let Some(parent) = parents.get(curr) {
                path.push(parent.to_string());
                curr = *parent;
            }
            path.reverse();
        }
        let critical_path = path.into_iter().take(25).collect::<Vec<_>>();

        flows.push(FlowRow {
            id: idx as i64 + 1,
            name: entry.name.clone(),
            entry_point_id: entry.id,
            entry_point: entry.qualified_name.clone(),
            depth: 6,
            node_count: node_ids.len() as i64,
            node_ids,
            critical_path,
            criticality,
            file_count: files.len() as i64,
        });
    }
    flows
}

fn is_security_relevant(node: &NodeRow) -> bool {
    const TERMS: &[&str] = &[
        "auth",
        "crypto",
        "token",
        "secret",
        "permission",
        "policy",
        "shell",
        "command",
        "process",
        "route",
        "persist",
        "db",
        "network",
        "provider",
        "execute",
        "tool",
        "quota",
        "acl",
        "key",
    ];
    let haystack = format!(
        "{} {} {}",
        node.name.to_lowercase(),
        node.file_path.to_lowercase(),
        node.signature.to_lowercase()
    );
    TERMS.iter().any(|term| haystack.contains(term))
}

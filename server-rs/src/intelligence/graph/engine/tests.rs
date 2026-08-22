//! @docs ARCHITECTURE:Core
//!
//! ### AI Assist Note
//! **Graph Engine Test Suite**: Rigorous tests for the symbol graph engine,
//! covering DAG traversals, circular dependencies, incremental cache updates,
//! and Red-Team three-path verification.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Unit test failures or timeout.
//! - **Telemetry Link**: Search `[tests]` in test logs.
//! - **Trace Scope**: `server-rs::intelligence::graph::engine::tests`

use super::*;
use crate::intelligence::graph::{
    cache::{CacheManagementService, CacheManager},
    key::index_key,
    parse::{CodeParsingService, CodeParser},
    types::SymbolEdge,
};
use std::fs::File;
use std::io::Write;
use tempfile::tempdir;

#[test]
fn test_empty_blast_radius_nonexistent() {
    let dir = tempdir().unwrap();
    let graph = CodeSymbolGraph::new(dir.path().to_path_buf());
    let affected = graph.calculate_blast_radius("nonexistent", "src/lib.rs");
    assert!(
        affected.is_empty(),
        "Blast radius of nonexistent symbol must be empty"
    );
}

#[test]
fn test_happy_path_symbol_dependency() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("main.rs");

    let mut file = File::create(&file_path).unwrap();
    writeln!(file, "fn helper() {{ }}").unwrap();
    writeln!(file, "fn main() {{ helper(); }}").unwrap();

    let mut graph = CodeSymbolGraph::new(dir.path().to_path_buf());
    let salt = uuid::Uuid::new_v4().to_string();
    graph.build(&salt).unwrap();

    assert!(
        graph.graph.node_count() >= 2,
        "Should index at least 2 symbols"
    );

    let affected = graph.calculate_blast_radius("helper", "main.rs");
    assert!(
        !affected.is_empty(),
        "helper blast radius should not be empty"
    );
    let has_main = affected.iter().any(|node| node.name == "main");
    assert!(has_main, "main should depend on helper");
}

#[test]
fn test_circular_dependency_handling() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("main.rs");

    let mut file = File::create(&file_path).unwrap();
    writeln!(file, "fn alpha() {{ beta(); }}").unwrap();
    writeln!(file, "fn beta() {{ alpha(); }}").unwrap();

    let mut graph = CodeSymbolGraph::new(dir.path().to_path_buf());
    let salt = uuid::Uuid::new_v4().to_string();
    graph.build(&salt).unwrap();

    let affected_alpha = graph.calculate_blast_radius("alpha", "main.rs");
    let affected_beta = graph.calculate_blast_radius("beta", "main.rs");

    assert!(!affected_alpha.is_empty());
    assert!(!affected_beta.is_empty());
}

#[test]
fn test_incremental_ast_caching() {
    let dir = tempdir().unwrap();
    let file_a = dir.path().join("a.rs");
    let file_b = dir.path().join("b.rs");

    let mut f_a = File::create(&file_a).unwrap();
    writeln!(f_a, "fn helper() {{ }}").unwrap();
    drop(f_a);

    let mut f_b = File::create(&file_b).unwrap();
    writeln!(f_b, "fn main() {{ helper(); }}").unwrap();
    drop(f_b);

    let mut graph = CodeSymbolGraph::new(dir.path().to_path_buf());
    let salt = "test_salt".to_string();

    // 1. Initial build
    graph.build(&salt).unwrap();
    assert_eq!(graph.repository.file_metadata.len(), 2);
    assert_eq!(graph.repository.parse_cache.len(), 2);
    assert!(graph.index.contains_key(&index_key("a.rs", "helper")));
    assert!(graph.index.contains_key(&index_key("b.rs", "main")));

    let meta_a_before = *graph.repository.file_metadata.get(&file_a).unwrap();
    let meta_b_before = *graph.repository.file_metadata.get(&file_b).unwrap();

    std::thread::sleep(std::time::Duration::from_millis(10));

    // 2. Modify file_b, keep file_a untouched
    let mut f_b_mod = File::create(&file_b).unwrap();
    writeln!(f_b_mod, "fn main() {{ helper(); // modified comment \n }}").unwrap();
    drop(f_b_mod);

    graph.build(&salt).unwrap();

    let meta_a_after = *graph.repository.file_metadata.get(&file_a).unwrap();
    assert_eq!(meta_a_before, meta_a_after);

    let meta_b_after = *graph.repository.file_metadata.get(&file_b).unwrap();
    assert_ne!(meta_b_before, meta_b_after);

    // 3. Delete file_a and verify cleanup
    std::fs::remove_file(&file_a).unwrap();
    graph.build(&salt).unwrap();

    assert_eq!(graph.repository.file_metadata.len(), 1);
    assert_eq!(graph.repository.parse_cache.len(), 1);
    assert!(!graph.repository.file_metadata.contains_key(&file_a));
    assert!(!graph.repository.parse_cache.contains_key("a.rs"));
    assert!(!graph.index.contains_key(&index_key("a.rs", "helper")));
    assert!(graph.index.contains_key(&index_key("b.rs", "main")));
}

#[test]
fn test_blast_radius_deep_cycle_limit() {
    let dir = tempdir().unwrap();
    let mut graph = CodeSymbolGraph::new(dir.path().to_path_buf());

    let obf_path = "obf/path.rs".to_string();
    graph.obfuscated_to_real_path.insert(obf_path.clone(), "path.rs".to_string());

    let mut indices = Vec::new();
    for i in 1..=55 {
        let name = format!("S_{i}");
        let node = SymbolNode {
            name: name.clone(),
            path: obf_path.clone(),
            kind: "func".to_string(),
            signature: format!("fn S_{i}()"),
            start_line: i,
            end_line: i + 1,
            tokens: 5,
        };
        let idx = graph.graph.add_node(node);
        graph.index.insert(index_key("path.rs", &name), idx);
        indices.push(idx);
    }

    // S_N references S_N-1 (incoming to S_N-1 from S_N)
    for i in 1..55 {
        graph.graph.add_edge(
            indices[i],
            indices[i - 1],
            SymbolEdge { kind: "ref".to_string() },
        );
    }
    // S_1 references S_55
    graph.graph.add_edge(
        indices[0],
        indices[54],
        SymbolEdge { kind: "ref".to_string() },
    );

    let affected = graph.calculate_blast_radius("S_55", "path.rs");
    assert_eq!(affected.len(), 51, "Visited count should respect depth limit of 50 steps");
}

#[test]
fn test_blast_radius_isolated_node() {
    let dir = tempdir().unwrap();
    let mut graph = CodeSymbolGraph::new(dir.path().to_path_buf());

    let obf_path = "obf/path.rs".to_string();
    graph.obfuscated_to_real_path.insert(obf_path.clone(), "path.rs".to_string());

    let node = SymbolNode {
        name: "X".to_string(),
        path: obf_path.clone(),
        kind: "func".to_string(),
        signature: "fn X()".to_string(),
        start_line: 1,
        end_line: 2,
        tokens: 5,
    };
    let idx = graph.graph.add_node(node);
    graph.index.insert(index_key("path.rs", "X"), idx);

    let affected = graph.calculate_blast_radius("X", "path.rs");
    assert_eq!(affected.len(), 1);
    assert_eq!(affected[0].name, "X");
}

#[test]
fn test_full_cycle_with_mixed_changes() {
    let dir = tempdir().unwrap();
    let file_a = dir.path().join("a.rs");
    let file_b = dir.path().join("b.rs");
    let file_c = dir.path().join("c.rs");

    std::fs::write(&file_a, "fn a_func() {}").unwrap();
    std::fs::write(&file_b, "fn b_func() { a_func(); }").unwrap();
    std::fs::write(&file_c, "fn c_func() {}").unwrap();

    let mut graph = CodeSymbolGraph::new(dir.path().to_path_buf());
    let salt = "salt".to_string();

    graph.build(&salt).unwrap();
    assert_eq!(graph.repository.file_metadata.len(), 3);
    assert_eq!(graph.repository.parse_cache.len(), 3);
    assert!(graph.index.contains_key(&index_key("a.rs", "a_func")));
    assert!(graph.index.contains_key(&index_key("b.rs", "b_func")));
    assert!(graph.index.contains_key(&index_key("c.rs", "c_func")));

    let meta_b_before = *graph.repository.file_metadata.get(&file_b).unwrap();
    let meta_c_before = *graph.repository.file_metadata.get(&file_c).unwrap();

    std::thread::sleep(std::time::Duration::from_millis(10));

    std::fs::write(&file_b, "fn b_func() { c_func(); } // modified").unwrap();
    std::fs::remove_file(&file_a).unwrap();
    let file_d = dir.path().join("d.rs");
    std::fs::write(&file_d, "fn d_func() {}").unwrap();

    graph.build(&salt).unwrap();

    assert_eq!(graph.repository.file_metadata.len(), 3);
    assert_eq!(graph.repository.parse_cache.len(), 3);
    assert!(!graph.repository.file_metadata.contains_key(&file_a));
    assert!(!graph.repository.parse_cache.contains_key("a.rs"));
    assert!(graph.repository.file_metadata.contains_key(&file_b));
    assert!(graph.repository.file_metadata.contains_key(&file_c));
    assert!(graph.repository.file_metadata.contains_key(&file_d));

    let meta_b_after = *graph.repository.file_metadata.get(&file_b).unwrap();
    let meta_c_after = *graph.repository.file_metadata.get(&file_c).unwrap();
    assert_ne!(meta_b_before, meta_b_after);
    assert_eq!(meta_c_before, meta_c_after);

    assert!(!graph.index.contains_key(&index_key("a.rs", "a_func")));
    assert!(graph.index.contains_key(&index_key("b.rs", "b_func")));
    assert!(graph.index.contains_key(&index_key("c.rs", "c_func")));
    assert!(graph.index.contains_key(&index_key("d.rs", "d_func")));

    let affected_c = graph.calculate_blast_radius("c_func", "c.rs");
    assert!(affected_c.iter().any(|node| node.name == "b_func"));

    // Verify I/O error propagation for a missing file
    std::fs::remove_file(&file_b).unwrap();
    let files_list = vec![file_c, file_d, file_b.clone()];
    let cache_mgr = CacheManagementService;
    let (to_parse, _to_delete) =
        cache_mgr.check_changes(&files_list, &graph.repository.file_metadata, &graph.root);
    assert!(to_parse.contains(&file_b));

    let parser = CodeParsingService;
    let parse_res = parser.parse_files(&to_parse, &graph.root);
    assert!(parse_res.is_err(), "Unreadable or missing file should result in GraphError");
}

#[test]
fn test_typescript_import_export_handling() {
    let dir = tempdir().unwrap();
    let file_a = dir.path().join("a.tsx");
    let file_b = dir.path().join("b.tsx");

    std::fs::write(&file_a, "export function foo() { return 42; }").unwrap();
    std::fs::write(&file_b, "import { foo } from './a';\nexport function bar() { foo(); }").unwrap();

    let mut graph = CodeSymbolGraph::new(dir.path().to_path_buf());
    let salt = uuid::Uuid::new_v4().to_string();
    graph.build(&salt).unwrap();

    assert!(graph.index.contains_key(&index_key("a.tsx", "foo")));
    assert!(graph.index.contains_key(&index_key("b.tsx", "bar")));

    let affected = graph.calculate_blast_radius("foo", "a.tsx");
    assert!(affected.iter().any(|node| node.name == "bar"));
}

#[test]
fn test_typescript_circular_dependency() {
    let dir = tempdir().unwrap();
    let file_a = dir.path().join("a.tsx");
    let file_b = dir.path().join("b.tsx");

    std::fs::write(&file_a, "import { bar } from './b';\nexport function foo() { bar(); }").unwrap();
    std::fs::write(&file_b, "import { foo } from './a';\nexport function bar() { foo(); }").unwrap();

    let mut graph = CodeSymbolGraph::new(dir.path().to_path_buf());
    let salt = uuid::Uuid::new_v4().to_string();
    graph.build(&salt).unwrap();

    assert!(graph.index.contains_key(&index_key("a.tsx", "foo")));
    assert!(graph.index.contains_key(&index_key("b.tsx", "bar")));

    let affected_foo = graph.calculate_blast_radius("foo", "a.tsx");
    assert!(affected_foo.iter().any(|node| node.name == "bar"));

    let affected_bar = graph.calculate_blast_radius("bar", "b.tsx");
    assert!(affected_bar.iter().any(|node| node.name == "foo"));
}

#[test]
fn test_token_budgeted_context_resolution() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("main.rs");

    let mut file = File::create(&file_path).unwrap();
    writeln!(file, "fn helper() {{ }}").unwrap();
    writeln!(file, "fn main() {{ helper(); }}").unwrap();

    let mut graph = CodeSymbolGraph::new(dir.path().to_path_buf());
    let salt = uuid::Uuid::new_v4().to_string();
    graph.build(&salt).unwrap();

    // Large budget — both symbols should fit
    let resolved_large = graph.resolve_context("helper", "main.rs", 1000, None);
    assert_eq!(resolved_large.len(), 2);

    // Tiny budget — only target should be returned and its signature truncated
    let resolved_small = graph.resolve_context("helper", "main.rs", 2, None);
    assert!(resolved_small[0].signature.ends_with("..."));
}

#[test]
fn test_blast_radius_bounded_limits_and_truncation_flag() {
    let mut graph = CodeSymbolGraph::new(std::path::PathBuf::from("/workspace"));
    
    let root_idx = graph.graph.add_node(SymbolNode {
        name: "CoreHub".into(),
        path: "src/core.rs".into(),
        ..Default::default()
    });
    graph.index.insert(index_key("src/core.rs", "CoreHub"), root_idx);

    // Add 10 caller nodes depending on CoreHub
    for i in 1..=10 {
        let caller_idx = graph.graph.add_node(SymbolNode {
            name: format!("Caller_{}", i),
            path: format!("src/caller_{}.rs", i),
            ..Default::default()
        });
        graph.index.insert(index_key(&format!("src/caller_{}.rs", i), &format!("Caller_{}", i)), caller_idx);
        // caller calls CoreHub (incoming to CoreHub)
        graph.graph.add_edge(caller_idx, root_idx, SymbolEdge { kind: "call".into() });
    }

    // Test with limit 4: must return 4 nodes and truncated = true
    let (affected, is_truncated) = graph.calculate_blast_radius_bounded("CoreHub", "src/core.rs", Some(4));
    assert_eq!(affected.len(), 4, "Must cap at requested limit");
    assert!(is_truncated, "Truncation flag must be true when callers exceed limit");

    // Test with limit 20: must return all 11 nodes (root + 10 callers) and truncated = false
    let (all_affected, not_truncated) = graph.calculate_blast_radius_bounded("CoreHub", "src/core.rs", Some(20));
    assert_eq!(all_affected.len(), 11, "Must return all 11 nodes");
    assert!(!not_truncated, "Truncation flag must be false when limit is not exceeded");
}

// =========================================================================
// RED-TEAM PILLAR IV: THREE-PATH TEST RIGOR SUITE
// =========================================================================

#[test]
fn test_three_path_happy_multihop_blast_radius() {
    let dir = tempdir().expect("Failed to create tempdir");
    let src_dir = dir.path().join("src");
    std::fs::create_dir_all(&src_dir).unwrap();

    let leaf_file = src_dir.join("leaf.rs");
    let mut f1 = File::create(&leaf_file).unwrap();
    writeln!(f1, "pub fn leaf_fn() {{ }}").unwrap();

    let mid_file = src_dir.join("mid.rs");
    let mut f2 = File::create(&mid_file).unwrap();
    writeln!(f2, "pub fn mid_fn() {{ leaf_fn(); }}").unwrap();

    let root_file = src_dir.join("root.rs");
    let mut f3 = File::create(&root_file).unwrap();
    writeln!(f3, "pub fn root_fn() {{ mid_fn(); }}").unwrap();

    let mut graph = CodeSymbolGraph::new(dir.path().to_path_buf());
    let salt = "test-salt".to_string();
    let built = graph.build(&salt).expect("Build failed");
    assert!(built, "Graph build should succeed");

    let (affected, truncated) = graph.calculate_blast_radius_bounded("leaf_fn", "src/leaf.rs", Some(100));
    assert!(!truncated, "Result should not be truncated");

    let names: Vec<&str> = affected.iter().map(|n| n.name.as_str()).collect();
    assert!(names.contains(&"leaf_fn"), "Must contain origin symbol");
    assert!(names.contains(&"mid_fn"), "Must contain direct dependent");
    assert!(names.contains(&"root_fn"), "Must contain transitive dependent");
}

#[test]
fn test_three_path_failure_nonexistent_and_traversal_queries() {
    let dir = tempdir().expect("Failed to create tempdir");
    let graph = CodeSymbolGraph::new(dir.path().to_path_buf());

    // 1. Non-existent symbol query
    let (affected, truncated) = graph.calculate_blast_radius_bounded("ghost_symbol", "src/ghost.rs", Some(10));
    assert!(affected.is_empty(), "Blast radius of non-existent symbol must be empty");
    assert!(!truncated, "Empty blast radius must not be marked truncated");

    // 2. Traversal path resolution query
    let (traversal_res, _) = graph.calculate_blast_radius_bounded("root", "../../outside.rs", None);
    assert!(traversal_res.is_empty(), "Traversal query must return empty results");

    // 3. Resolve context on non-existent symbol
    let ctx = graph.resolve_context("missing", "src/missing.rs", 1000, None);
    assert!(ctx.is_empty(), "Context resolution for non-existent symbol must be empty");
}

#[test]
fn test_three_path_edge_case_cycles_depth_and_unicode() {
    let dir = tempdir().expect("Failed to create tempdir");
    let mut graph = CodeSymbolGraph::new(dir.path().to_path_buf());

    // 1. Construct cyclic dependency: Node A <-> Node B
    let node_a = SymbolNode {
        name: "fn_alpha".to_string(),
        path: "src/cycle.rs".to_string(),
        kind: "function".to_string(),
        signature: "fn fn_alpha(🦀: &str) -> ()".to_string(), // Multi-byte UTF-8 emoji
        tokens: 150,
        start_line: 1,
        end_line: 5,
    };
    let node_b = SymbolNode {
        name: "fn_beta".to_string(),
        path: "src/cycle.rs".to_string(),
        kind: "function".to_string(),
        signature: "fn fn_beta()".to_string(),
        tokens: 50,
        start_line: 6,
        end_line: 10,
    };

    let idx_a = graph.graph.add_node(node_a);
    let idx_b = graph.graph.add_node(node_b);

    // Cyclic edges: A calls B and B calls A
    graph.graph.add_edge(idx_a, idx_b, SymbolEdge { kind: "call".to_string() });
    graph.graph.add_edge(idx_b, idx_a, SymbolEdge { kind: "call".to_string() });

    let key_a = crate::intelligence::graph::key::index_key("src/cycle.rs", "fn_alpha");
    let key_b = crate::intelligence::graph::key::index_key("src/cycle.rs", "fn_beta");
    graph.index.insert(key_a, idx_a);
    graph.index.insert(key_b, idx_b);

    // Test cycle termination (must not infinite loop)
    let (affected, _) = graph.calculate_blast_radius_bounded("fn_alpha", "src/cycle.rs", Some(10));
    assert_eq!(affected.len(), 2, "Cycle must terminate and discover both nodes without looping");

    // Test budget smaller than start node token count + safe Unicode character boundary slicing
    let ctx = graph.resolve_context("fn_alpha", "src/cycle.rs", 2, None);
    assert_eq!(ctx.len(), 1, "Should only contain target node");
    assert!(ctx[0].signature.ends_with("..."), "Signature must be safely truncated");

    // Test max node truncation clamp
    let (clamped, was_truncated) = graph.calculate_blast_radius_bounded("fn_alpha", "src/cycle.rs", Some(1));
    assert_eq!(clamped.len(), 1, "Clamped limit of 1 must be respected");
    assert!(was_truncated, "Truncation flag must be true when node limit reached");
}

// Metadata: [tests]

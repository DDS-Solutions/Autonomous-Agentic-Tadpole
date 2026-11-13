//! @docs ARCHITECTURE:Core
//!
//! ### AI Context Alignment
//! - **Subsystem**: Sovereign Tooling / Invariant & AST Parity Guard
//! - **Primary Entrypoints**: `parse_router_ast`, `verify_scheduler_invariants`
//!
//! ### ⚠️ Invariants & Non-Negotiables
//! - `[Structural]` 100% deterministic AST parsing using `syn` (no regex for code analysis).
//! - `[Security]` Dual-Pass Nexus Protocol enforcement for routes and scheduler handles.
//!
//! ### 🔍 Debugging & Observability
//! - **Telemetry Link**: Search `[tadpole_guard]` in compiler and test logs.

pub const TELEMETRY_TAG: &str = "[tadpole_guard]";

use serde::{Deserialize, Serialize};
use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{Expr, ExprCall, ExprMethodCall, File, Stmt};

/// Extracted route representation from an Axum router AST.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtractedRoute {
    pub path: String,
    pub verb: String,
    pub handler: String,
    pub feature_gate: Option<String>,
}

/// Invariant violation finding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvariantFinding {
    pub rule: String,
    pub message: String,
    pub line: usize,
}

/// Visitor that extracts Axum router `.route("/path", method(handler))` definitions.
struct RouterVisitor {
    routes: Vec<ExtractedRoute>,
}

impl<'ast> Visit<'ast> for RouterVisitor {
    fn visit_expr_method_call(&mut self, node: &'ast ExprMethodCall) {
        if node.method == "route" && node.args.len() >= 2 {
            // First argument is the route path string literal
            if let Some(path_str) = extract_string_literal(&node.args[0]) {
                // Second argument is the method router (e.g. get(handler).post(handler2))
                self.extract_methods_from_expr(&path_str, &node.args[1]);
            }
        }
        syn::visit::visit_expr_method_call(self, node);
    }
}

impl RouterVisitor {
    fn extract_methods_from_expr(&mut self, path: &str, expr: &Expr) {
        match expr {
            Expr::MethodCall(m) => {
                let verb = m.method.to_string().to_uppercase();
                if is_http_verb(&verb) {
                    let handler = m.args.first()
                        .map(extract_expr_name)
                        .unwrap_or_else(|| "unknown".to_string());
                    self.routes.push(ExtractedRoute {
                        path: path.to_string(),
                        verb,
                        handler,
                        feature_gate: None,
                    });
                }
                // Recursively check the receiver of the chained call (e.g. get().post())
                self.extract_methods_from_expr(path, &m.receiver);
            }
            Expr::Call(c) => {
                let verb = extract_expr_name(&c.func).to_uppercase();
                if is_http_verb(&verb) {
                    let handler = c.args.first()
                        .map(extract_expr_name)
                        .unwrap_or_else(|| "unknown".to_string());
                    self.routes.push(ExtractedRoute {
                        path: path.to_string(),
                        verb,
                        handler,
                        feature_gate: None,
                    });
                }
            }
            _ => {}
        }
    }
}

/// Parses Rust source code and extracts all Axum HTTP routes with compiler-grade AST precision.
pub fn parse_router_ast(source_code: &str) -> Result<Vec<ExtractedRoute>, syn::Error> {
    let file: File = syn::parse_str(source_code)?;
    let mut visitor = RouterVisitor { routes: Vec::new() };
    visitor.visit_file(&file);
    // Reverse routes so outer chained calls preserve natural declaration order
    visitor.routes.reverse();
    Ok(visitor.routes)
}

/// Visitor that validates Scheduler Invariants (JoinHandle retention on tokio::spawn).
struct SchedulerInvariantVisitor {
    findings: Vec<InvariantFinding>,
}

impl<'ast> Visit<'ast> for SchedulerInvariantVisitor {
    fn visit_stmt(&mut self, stmt: &'ast Stmt) {
        if let Stmt::Expr(expr, Some(_semi)) = stmt {
            if is_unassigned_tokio_spawn(expr) {
                self.findings.push(InvariantFinding {
                    rule: "INV-SCHEDULER-001".to_string(),
                    message: "Unassigned tokio::spawn dropped without tracking JoinHandle".to_string(),
                    line: expr.span().start().line,
                });
            }
        }
        syn::visit::visit_stmt(self, stmt);
    }
}

/// Verifies that all `tokio::spawn` calls in the code retain tracking handles (Scheduler Invariant).
pub fn verify_scheduler_invariants(source_code: &str) -> Result<Vec<InvariantFinding>, syn::Error> {
    let file: File = syn::parse_str(source_code)?;
    let mut visitor = SchedulerInvariantVisitor { findings: Vec::new() };
    visitor.visit_file(&file);
    Ok(visitor.findings)
}

fn is_http_verb(verb: &str) -> bool {
    matches!(verb, "GET" | "POST" | "PUT" | "DELETE" | "PATCH" | "HEAD" | "OPTIONS")
}

fn extract_string_literal(expr: &Expr) -> Option<String> {
    if let Expr::Lit(syn::ExprLit { lit: syn::Lit::Str(s), .. }) = expr {
        Some(s.value())
    } else {
        None
    }
}

fn extract_expr_name(expr: &Expr) -> String {
    if let Expr::Path(syn::ExprPath { path, .. }) = expr {
        path.segments.last().map(|s| s.ident.to_string()).unwrap_or_default()
    } else {
        "complex_expr".to_string()
    }
}

fn is_unassigned_tokio_spawn(expr: &Expr) -> bool {
    if let Expr::Call(ExprCall { func, .. }) = expr {
        if let Expr::Path(syn::ExprPath { path, .. }) = &**func {
            let path_str = path.segments.iter().map(|s| s.ident.to_string()).collect::<Vec<_>>().join("::");
            return path_str == "tokio::spawn" || path_str == "spawn";
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_chained_axum_routes() {
        let code = r#"
            pub fn router() -> Router {
                Router::new()
                    .route("/v1/agents", get(list_agents).post(create_agent))
                    .route("/v1/agents/:id", delete(delete_agent).put(update_agent))
            }
        "#;
        let routes = parse_router_ast(code).expect("Parsing must succeed");
        assert_eq!(routes.len(), 4);
        assert!(routes.iter().any(|r| r.path == "/v1/agents" && r.verb == "GET" && r.handler == "list_agents"));
        assert!(routes.iter().any(|r| r.path == "/v1/agents" && r.verb == "POST" && r.handler == "create_agent"));
        assert!(routes.iter().any(|r| r.path == "/v1/agents/:id" && r.verb == "DELETE" && r.handler == "delete_agent"));
        assert!(routes.iter().any(|r| r.path == "/v1/agents/:id" && r.verb == "PUT" && r.handler == "update_agent"));
    }

    #[test]
    fn test_detect_unassigned_tokio_spawn() {
        let code_violating = r#"
            pub fn run_task() {
                // Violates scheduler invariant: drops JoinHandle
                tokio::spawn(async move {
                    do_work().await;
                });
            }
        "#;
        let findings = verify_scheduler_invariants(code_violating).expect("Must parse");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule, "INV-SCHEDULER-001");
        assert!(findings[0].line > 0, "Expected non-zero source line number");

        let code_compliant = r#"
            pub fn run_task() -> tokio::task::JoinHandle<()> {
                // Compliant: assigns JoinHandle
                let handle = tokio::spawn(async move {
                    do_work().await;
                });
                handle
            }
        "#;
        let compliant_findings = verify_scheduler_invariants(code_compliant).expect("Must parse");
        assert_eq!(compliant_findings.len(), 0);
    }
}

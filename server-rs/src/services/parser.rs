//! @docs ARCHITECTURE:UI-Services
//! 
//! ### AI Assist Note
//! **! Sovereign Parser Service - High-Fidelity Symbol Extraction**
//! This module implements high-fidelity logic for the Sovereign Reality layer.
//! 
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Runtime logic error, state desynchronization, or resource exhaustion.
//! - **Telemetry Link**: Search `[parser]` in tracing logs.

//! Sovereign Parser Service - High-Fidelity Symbol Extraction
//! 
//! Replaces brittle regex-based parsing with tree-sitter for mission-critical
//! codebase intelligence. Supports Rust and TypeScript/TSX.

use tree_sitter::{Parser, Query, QueryCursor, StreamingIterator};
use std::path::Path;

pub struct SymbolParser {
    rust_language: tree_sitter::Language,
    typescript_language: tree_sitter::Language,
    tsx_language: tree_sitter::Language,
}

#[derive(Debug, Clone)]
pub struct ParsedSymbol {
    pub name: String,
    pub kind: String,
    #[allow(dead_code)]
    pub start_line: usize,
    #[allow(dead_code)]
    pub end_line: usize,
    pub body: String,
}

impl SymbolParser {
    pub fn new() -> Self {
        Self {
            rust_language: tree_sitter_rust::LANGUAGE.into(),
            typescript_language: tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            tsx_language: tree_sitter_typescript::LANGUAGE_TSX.into(),
        }
    }

    pub fn list_symbols(&self, path: &str, content: &str) -> Vec<ParsedSymbol> {
        let extension = Path::new(path)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("");

        let (language, query_str) = match extension {
            "rs" => (
                self.rust_language.clone(),
                r#"
                (function_item name: (identifier) @name) @fn
                (struct_item name: (type_identifier) @name) @struct
                (enum_item name: (type_identifier) @name) @enum
                (trait_item name: (type_identifier) @name) @trait
                (type_item name: (type_identifier) @name) @type
                "#,
            ),
            "ts" => (
                self.typescript_language.clone(),
                r#"
                (function_declaration name: (identifier) @name) @fn
                (class_declaration name: (type_identifier) @name) @class
                (interface_declaration name: (type_identifier) @name) @interface
                (type_alias_declaration name: (type_identifier) @name) @type
                (method_definition name: (property_identifier) @name) @method
                "#,
            ),
            "tsx" => (
                self.tsx_language.clone(),
                r#"
                (function_declaration name: (identifier) @name) @fn
                (class_declaration name: (type_identifier) @name) @class
                (interface_declaration name: (type_identifier) @name) @interface
                (type_alias_declaration name: (type_identifier) @name) @type
                "#,
            ),
            _ => return Vec::new(),
        };

        let mut parser = Parser::new();
        if let Err(e) = parser.set_language(&language) {
            tracing::error!("❌ [Parser] Failed to set language for {}: {}", path, e);
            return Vec::new();
        }

        let tree = match parser.parse(content, None) {
            Some(t) => t,
            None => {
                tracing::error!("❌ [Parser] Failed to parse content for {}", path);
                return Vec::new();
            }
        };

        let query = match Query::new(&language, query_str) {
            Ok(q) => q,
            Err(e) => {
                tracing::error!("❌ [Parser] Failed to create query for {}: {}", path, e);
                return Vec::new();
            }
        };
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, tree.root_node(), content.as_bytes());

        let mut symbols = Vec::new();
        while let Some(m) = matches.next() {
            let mut symbol_name = String::new();
            let mut kind = "unknown".to_string();
            
            // We expect at least one capture (the item itself)
            if m.captures.is_empty() {
                continue;
            }
            
            let mut node = m.captures[0].node;

            for capture in m.captures {
                let capture_name = query.capture_names()[capture.index as usize];
                if capture_name == "name" {
                    if let Some(s) = content.get(capture.node.byte_range()) {
                        symbol_name = s.to_string();
                    }
                } else {
                    kind = capture_name.to_string();
                    node = capture.node;
                }
            }

            let start_line = node.start_position().row + 1;
            let end_line = node.end_position().row + 1;
            let body = content.get(node.byte_range()).unwrap_or("").to_string();

            symbols.push(ParsedSymbol {
                name: symbol_name,
                kind,
                start_line,
                end_line,
                body,
            });
        }

        symbols
    }
}

// Metadata: [parser]

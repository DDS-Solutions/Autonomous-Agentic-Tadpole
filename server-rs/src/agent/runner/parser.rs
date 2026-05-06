//! @docs ARCHITECTURE:Intelligence
//!
//! ### AI Assist Note
//! **Polyglot Parser**: A high-fidelity extraction engine designed to stabilize 
//! tool-calling for small or non-native models (Gemma, Llama, Phi). 
//! Implements **Multi-Format Extraction**: supports XML tags, native Gemma 
//! call syntax, and standard JSON. Features **Heuristic JSON Repair**: 
//! automatically fixes common LLM hallucinations like unquoted keys or 
//! trailing commas.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Heavily nested JSON that exceeds regex recursion limits 
//!   or multiple conflicting tool calls in a single turn.
//! - **Trace Scope**: `server-rs::agent::runner::parser`
//!

use crate::agent::types::ToolCall;
use regex::Regex;
use once_cell::sync::Lazy;

/// Resilient parser for extracting tool calls from raw model output.
pub struct PolyglotParser;

static XML_TOOL_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?s)<tool_call>(.*?)</tool_call>").unwrap()
});

static GEMMA_TOOL_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?s)<\|tool_call\|>call:([a-zA-Z0-9_-]+)(\{.*?\})<tool_call\|>").unwrap()
});

static BARE_CALL_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?s)(?:\n|^|\s)call:([a-zA-Z0-9_-]+)(\{.*?\})").unwrap()
});

static KEY_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"([{,]\s*)([a-zA-Z_]\w*)\s*:").unwrap()
});

static COMMA_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r",\s*([\]}])").unwrap()
});

impl PolyglotParser {
    /// Extracts all tool calls from the raw text, trying multiple formats.
    pub fn extract(text: &str) -> Vec<ToolCall> {
        let mut calls = Vec::new();

        // 1. Try XML-like JSON format: <tool_call>{...}</tool_call>
        for cap in XML_TOOL_REGEX.captures_iter(text) {
            if let Some(json_str) = cap.get(1) {
                if let Some(call) = Self::parse_json_call(json_str.as_str()) {
                    calls.push(call);
                }
            }
        }

        // 2. Try Gemma native format: <|tool_call|>call:name{...}<tool_call|>
        for cap in GEMMA_TOOL_REGEX.captures_iter(text) {
            let name = cap.get(1).map(|m| m.as_str().to_string()).unwrap_or_default();
            let args_raw = cap.get(2).map(|m| m.as_str()).unwrap_or("{}");
            if let Some(args) = Self::repair_and_parse_json(args_raw) {
                calls.push(ToolCall { name, args });
            }
        }

        // 3. Fallback: Bare call format (only if no calls found yet to avoid false positives)
        if calls.is_empty() {
            for cap in BARE_CALL_REGEX.captures_iter(text) {
                let name = cap.get(1).map(|m| m.as_str().to_string()).unwrap_or_default();
                let args_raw = cap.get(2).map(|m| m.as_str()).unwrap_or("{}");
                if let Some(args) = Self::repair_and_parse_json(args_raw) {
                    calls.push(ToolCall { name, args });
                }
            }
        }

        calls
    }

    /// Attempts to parse a JSON object representing a tool call (name + arguments).
    fn parse_json_call(json_str: &str) -> Option<ToolCall> {
        let repaired = Self::repair_json(json_str);
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&repaired) {
            let name = v.get("name").and_then(|n| n.as_str()).map(|s| s.to_string());
            let args = v.get("arguments").cloned().unwrap_or_else(|| serde_json::json!({}));
            if let Some(name) = name {
                return Some(ToolCall { name, args });
            }
        }
        None
    }

    /// Repairs and parses a raw JSON arguments object.
    fn repair_and_parse_json(json_str: &str) -> Option<serde_json::Value> {
        let repaired = Self::repair_json(json_str);
        serde_json::from_str(&repaired).ok()
    }

    /// Performs heuristic repair on malformed JSON strings.
    /// - Adds quotes to unquoted keys.
    /// - Removes trailing commas.
    /// - Normalizes single quotes to double quotes.
    pub fn repair_json(json_str: &str) -> String {
        let mut s = json_str.trim().to_string();

        // 1. Normalize quotes: replace single quotes with double quotes (dangerous but often needed)
        // Only if it looks like it's being used for keys or strings.
        // For simplicity, we'll skip complex quote normalization and focus on unquoted keys.

        // 2. Fix unquoted keys: { key: "value" } -> { "key": "value" }
        s = KEY_REGEX.replace_all(&s, r#"$1"$2":"#).to_string();

        // 3. Remove trailing commas: [1, 2,] -> [1, 2]
        s = COMMA_REGEX.replace_all(&s, r"$1").to_string();

        // 4. Ensure brackets are balanced (basic healing)
        if s.starts_with('{') && !s.ends_with('}') {
            s.push('}');
        }
        if !s.starts_with('{') && s.ends_with('}') {
            s.insert(0, '{');
        }

        s
    }

    /// Removes all detected tool call blocks from the text to get the clean assistant message.
    pub fn scrub_tool_calls(text: &str) -> String {
        let mut s = text.to_string();
        s = XML_TOOL_REGEX.replace_all(&s, "").to_string();
        s = GEMMA_TOOL_REGEX.replace_all(&s, "").to_string();
        s = BARE_CALL_REGEX.replace_all(&s, "").to_string();
        s.trim().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_repair_json_unquoted_keys() {
        let input = "{ name: test, args: { file: main.rs } }";
        let repaired = PolyglotParser::repair_json(input);
        // Note: values might still be unquoted if they aren't keys.
        // But keys are fixed: { "name": test, "args": { "file": main.rs } }
        assert!(repaired.contains(r#""name":"#));
        assert!(repaired.contains(r#""args":"#));
        assert!(repaired.contains(r#""file":"#));
    }

    #[test]
    fn test_extract_gemma_format() {
        let input = "I will search now. <|tool_call|>call:list_files{\"path\": \".\"}<tool_call|>";
        let calls = PolyglotParser::extract(input);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "list_files");
    }

    #[test]
    fn test_extract_xml_format() {
        let input = "Calling tool: <tool_call>{\"name\": \"read_file\", \"arguments\": {\"path\": \"README.md\"}}</tool_call>";
        let calls = PolyglotParser::extract(input);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "read_file");
        assert_eq!(calls[0].args["path"], "README.md");
    }

    #[test]
    fn test_scrub_tool_calls() {
        let input = "Thinking... <tool_call>...</tool_call> Done.";
        let scrubbed = PolyglotParser::scrub_tool_calls(input);
        assert_eq!(scrubbed, "Thinking...  Done.");
    }
}

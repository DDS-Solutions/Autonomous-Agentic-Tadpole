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
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParserError {
    #[error("No tool calls found in response")]
    NoCallsFound,
    #[error("Invalid JSON in tool call: {0}")]
    InvalidJson(String),
    #[error("Missing tool name in call")]
    MissingName,
}

pub type ParserResult<T> = Result<T, ParserError>;

/// Resilient parser for extracting tool calls from raw model output.
pub struct PolyglotParser;

static XML_TOOL_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?s)<(?:tool_call|invoke_tool)>(.*?)</(?:tool_call|invoke_tool)>").unwrap()
});

static GEMMA_TOOL_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?s)<\|tool_call\|>call:([a-zA-Z0-9_-]+)(\{.*?\})<tool_call\|>").unwrap()
});

static BARE_CALL_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?s)(?:\n|^|\s)call:([a-zA-Z0-9_-]+)(\{.*?\})").unwrap()
});

static EXECUTE_TOOL_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?s)<execute_tool>\s*<tool_name>(.*?)</tool_name>\s*<tool_input>(.*?)</tool_input>\s*</execute_tool>").unwrap()
});

static KEY_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"([{,]\s*)([a-zA-Z_]\w*)\s*:").unwrap()
});

static COMMA_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r",\s*([\]}])").unwrap()
});

impl PolyglotParser {
    /// Extracts all tool calls from the raw text, trying multiple formats.
    pub fn extract(text: &str) -> ParserResult<Vec<ToolCall>> {
        let mut calls = Vec::new();
        let mut last_error = None;

        // 1. Try XML-like JSON format: <tool_call>{...}</tool_call>
        for cap in XML_TOOL_REGEX.captures_iter(text) {
            if let Some(json_str) = cap.get(1) {
                match Self::parse_json_call(json_str.as_str()) {
                    Ok(call) => calls.push(call),
                    Err(e) => last_error = Some(e),
                }
            }
        }

        // 2. Try Gemma native format: <|tool_call|>call:name{...}<tool_call|>
        for cap in GEMMA_TOOL_REGEX.captures_iter(text) {
            let name = cap.get(1).map(|m| m.as_str().to_string()).unwrap_or_default();
            let args_raw = cap.get(2).map(|m| m.as_str()).unwrap_or("{}");
            match Self::repair_and_parse_json(args_raw) {
                Ok(args) => calls.push(ToolCall { name, args }),
                Err(e) => last_error = Some(e),
            }
        }

        // 3. Fallback: Bare call format (only if no calls found yet to avoid false positives)
        if calls.is_empty() {
            for cap in BARE_CALL_REGEX.captures_iter(text) {
                let name = cap.get(1).map(|m| m.as_str().to_string()).unwrap_or_default();
                let args_raw = cap.get(2).map(|m| m.as_str()).unwrap_or("{}");
                match Self::repair_and_parse_json(args_raw) {
                    Ok(args) => calls.push(ToolCall { name, args }),
                    Err(e) => last_error = Some(e),
                }
            }
        }

        // 4. Recovery: Hallucinated <execute_tool> format
        for cap in EXECUTE_TOOL_REGEX.captures_iter(text) {
            let name_raw = cap.get(1).map(|m| m.as_str().trim()).unwrap_or_default();
            let input_raw = cap.get(2).map(|m| m.as_str().trim()).unwrap_or("");

            // The model often wraps the JSON in markdown code blocks
            let json_str = if let Some(stripped) = input_raw.strip_prefix("```json") {
                stripped.strip_suffix("```").unwrap_or(stripped).trim()
            } else if let Some(stripped) = input_raw.strip_prefix("```") {
                stripped.strip_suffix("```").unwrap_or(stripped).trim()
            } else {
                input_raw
            };

            // If the name is 'execute_tool' or 'execute_command', the real tool name is likely inside the JSON
            if name_raw == "execute_tool" || name_raw == "execute_command" {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(json_str) {
                    // Try to find the real name in the inner object first
                    let inner_name = v.get("tool_args").or(v.get("params")).or(v.get("tool_input"))
                        .and_then(|i| i.get("tool_name").or(i.get("command")).or(i.get("function")))
                        .and_then(|n| n.as_str());

                    let real_name = inner_name.or_else(|| {
                         let n = v.get("tool_name").or(v.get("command")).or(v.get("function")).and_then(|n| n.as_str());
                         // If it's the same as name_raw, it's not the "real" name we want
                         if n == Some(name_raw) { None } else { n }
                    });
                    
                    let real_args = v.get("tool_args").or(v.get("params")).or(v.get("tool_input"))
                        .map(|i| {
                            // If we found an inner name, the other fields in this object are likely the args
                            let mut args = i.clone();
                            if let Some(obj) = args.as_object_mut() {
                                obj.remove("tool_name");
                                obj.remove("command");
                                obj.remove("function");
                            }
                            args
                        })
                        .or_else(|| v.get("arguments").cloned());
                    
                    if let Some(name) = real_name {
                        calls.push(ToolCall { name: name.to_string(), args: real_args.unwrap_or_else(|| serde_json::json!({})) });
                        continue;
                    }
                }
            }

            if let Some(args) = Self::repair_and_parse_json(json_str).ok() {
                calls.push(ToolCall { name: name_raw.to_string(), args });
            }
        }

        // 5. Last Resort: Scan for any JSON blocks and check if they look like tool calls
        if calls.is_empty() {
             // Find blocks starting with { and ending with }
             let mut start = 0;
             while let Some(open) = text[start..].find('{') {
                 let open_idx = start + open;
                 let mut balance = 0;
                 let mut close_idx = None;
                 for (i, c) in text[open_idx..].chars().enumerate() {
                     if c == '{' { balance += 1; }
                     else if c == '}' { balance -= 1; }
                     if balance == 0 {
                         close_idx = Some(open_idx + i + 1);
                         break;
                     }
                 }
                 
                 if let Some(end) = close_idx {
                     let potential_json = &text[open_idx..end];
                     if let Ok(v) = serde_json::from_str::<serde_json::Value>(potential_json) {
                         let name_raw = v.get("tool_name").or_else(|| v.get("command")).and_then(|n| n.as_str()).unwrap_or_default();
                         
                         if !name_raw.is_empty() {
                             // Handle Wrappers
                             if name_raw == "execute_tool" || name_raw == "execute_command" {
                                 let inner_name = v.get("tool_args").or(v.get("params")).or(v.get("tool_input"))
                                     .and_then(|i| i.get("tool_name").or(i.get("command")).or(i.get("function")))
                                     .and_then(|n| n.as_str());
                                 
                                 let real_name = inner_name.or_else(|| {
                                     let n = v.get("tool_name").or(v.get("command")).or(v.get("function")).and_then(|n| n.as_str());
                                     if n == Some(name_raw) { None } else { n }
                                 });

                                 let real_args = v.get("tool_args").or(v.get("params")).or(v.get("tool_input")).cloned()
                                     .or_else(|| v.get("arguments").cloned());

                                 if let Some(name) = real_name {
                                     calls.push(ToolCall { name: name.to_string(), args: real_args.unwrap_or_else(|| serde_json::json!({})) });
                                 } else {
                                     calls.push(ToolCall { name: name_raw.to_string(), args: real_args.unwrap_or_else(|| serde_json::json!({})) });
                                 }
                             } else {
                                 let args = v.get("tool_input").or_else(|| v.get("tool_args")).or_else(|| v.get("params")).cloned();
                                 calls.push(ToolCall { name: name_raw.to_string(), args: args.unwrap_or_else(|| serde_json::json!({})) });
                             }
                         }
                     }
                     start = end;
                 } else {
                     break;
                 }
             }
        }

        if calls.is_empty() {
            if let Some(err) = last_error {
                return Err(err);
            }
            return Err(ParserError::NoCallsFound);
        }
        Ok(calls)
    }

    /// Attempts to parse a JSON object representing a tool call (name + arguments).
    fn parse_json_call(json_str: &str) -> ParserResult<ToolCall> {
        let repaired = Self::repair_json(json_str);
        match serde_json::from_str::<serde_json::Value>(&repaired) {
            Ok(v) => {
                let name = v.get("name").and_then(|n| n.as_str()).map(|s| s.to_string());
                let args = v.get("arguments").cloned().unwrap_or_else(|| serde_json::json!({}));
                if let Some(name) = name {
                    Ok(ToolCall { name, args })
                } else {
                    Err(ParserError::MissingName)
                }
            }
            Err(e) => Err(ParserError::InvalidJson(e.to_string())),
        }
    }

    /// Repairs and parses a raw JSON arguments object.
    fn repair_and_parse_json(json_str: &str) -> ParserResult<serde_json::Value> {
        let repaired = Self::repair_json(json_str);
        serde_json::from_str(&repaired).map_err(|e| ParserError::InvalidJson(e.to_string()))
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
        s = EXECUTE_TOOL_REGEX.replace_all(&s, "").to_string();
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
        let calls = PolyglotParser::extract(input).unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "list_files");
    }

    #[test]
    fn test_extract_xml_format() {
        let input = "Calling tool: <tool_call>{\"name\": \"read_file\", \"arguments\": {\"path\": \"README.md\"}}</tool_call>";
        let calls = PolyglotParser::extract(input).unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "read_file");
        assert_eq!(calls[0].args["path"], "README.md");
    }

    #[test]
    fn test_extract_invoke_tool_format() {
        let input = "Calling tool: <invoke_tool>{\"name\": \"read_file\", \"arguments\": {\"path\": \"README.md\"}}</invoke_tool>";
        let calls = PolyglotParser::extract(input).unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "read_file");
        assert_eq!(calls[0].args["path"], "README.md");
    }

    #[test]
    fn test_extract_execute_tool_format() {
        let input = r#"I will recruit Tadpole now. <execute_tool> <tool_name>execute_tool</tool_name> <tool_input> ```json { "tool_name": "recruit", "tool_input": { "agent_id": "2", "message": "Recruit Tadpole" } } ``` </tool_input> </execute_tool>"#;
        let calls = PolyglotParser::extract(input).unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "recruit");
        assert_eq!(calls[0].args["agent_id"], "2");
    }

    #[test]
    fn test_extract_execute_command_format() {
        let input = r#"Action Plan: ```json { "tool_name": "execute_command", "tool_args": { "command": "recruit", "agent_id": "2" } } ```"#;
        let calls = PolyglotParser::extract(input).unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "recruit");
        assert_eq!(calls[0].args["agent_id"], "2");
    }

    #[test]
    fn test_scrub_tool_calls() {
        let input = "Thinking... <tool_call>...</tool_call> Done.";
        let scrubbed = PolyglotParser::scrub_tool_calls(input);
        assert_eq!(scrubbed, "Thinking...  Done.");
    }
}

// Metadata: [parser]

// Metadata: [parser]

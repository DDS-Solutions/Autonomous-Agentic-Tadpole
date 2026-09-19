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
//! - **Telemetry Link**: Search `[parser]` in tracing logs.
//! - **Failure Path**: Heavily nested JSON that exceeds regex recursion limits
//!   or multiple conflicting tool calls in a single turn.
//! - **Trace Scope**: `server-rs::agent::runner::parser`
//!

use crate::agent::types::ToolCall;
use once_cell::sync::Lazy;
use regex::Regex;
use thiserror::Error;
use tracing::{debug, info, trace, warn};

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

/// Maximum input length accepted for extraction to prevent denial-of-service on pathological outputs (1 MB).
const MAX_INPUT_LEN: usize = 1_048_576;

/// Resilient parser for extracting tool calls from raw model output.
pub struct PolyglotParser;

static XML_TOOL_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?s)<(?:tool_call|invoke_tool)>(.*?)</(?:tool_call|invoke_tool)>").unwrap()
});

static GEMMA_PREFIX_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?s)<\|tool_call\|>call:([a-zA-Z0-9_-]+)").unwrap()
});

static BARE_CALL_PREFIX_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?s)(?:\n|^|\s)call:([a-zA-Z0-9_-]+)").unwrap());

static EXECUTE_TOOL_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?s)<execute_tool>\s*<tool_name>(.*?)</tool_name>\s*<tool_input>(.*?)</tool_input>\s*</execute_tool>").unwrap()
});

static FUNCTION_START_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?s)<function=([a-zA-Z0-9_-]+)").unwrap()
});

static KEY_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"([{,]\s*)([a-zA-Z_]\w*)\s*:").unwrap());

static COMMA_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r",\s*([\]}])").unwrap());

static BARE_PATTERN: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?s)^(?:call:)?([a-zA-Z0-9_-]+)(\{.*?\})$").unwrap());

static WORD_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r"^[a-zA-Z0-9_-]+$").unwrap());

impl PolyglotParser {
    /// Extracts all tool calls from the raw text, trying multiple formats.
    pub fn extract(text: &str) -> ParserResult<Vec<ToolCall>> {
        let text = if text.len() > MAX_INPUT_LEN {
            warn!(
                target: "server_rs::agent::runner::parser",
                "[parser] Input length {} exceeds MAX_INPUT_LEN {}, truncating scan",
                text.len(),
                MAX_INPUT_LEN
            );
            &text[..MAX_INPUT_LEN]
        } else {
            text
        };

        let mut calls = Vec::new();
        let mut last_error = None;

        // 1. Try XML-like JSON format: <tool_call>{...}</tool_call> or <invoke_tool>{...}</invoke_tool>
        for cap in XML_TOOL_REGEX.captures_iter(text) {
            if let Some(json_str) = cap.get(1) {
                match Self::parse_json_call(json_str.as_str()) {
                    Ok(call) => {
                        debug!(target: "server_rs::agent::runner::parser", "[parser] Extracted XML tool call '{}'", call.name);
                        calls.push(call);
                    }
                    Err(e) => last_error = Some(e),
                }
            }
        }

        // 2. Try Gemma native format: <|tool_call|>call:name{...}<tool_call|> or <|tool_call|>
        let mut gemma_offset = 0;
        while let Some(cap) = GEMMA_PREFIX_REGEX.captures(&text[gemma_offset..]) {
            let rel_start = cap.get(0).unwrap().start();
            let abs_start = gemma_offset + rel_start;
            let abs_match_end = gemma_offset + cap.get(0).unwrap().end();
            let name = cap.get(1).map(|m| m.as_str().to_string()).unwrap_or_default();

            if let Some((brace_start, brace_end)) = Self::scan_balanced_object(text, abs_match_end) {
                // Ensure brace immediately follows name (ignoring optional whitespace)
                if text[abs_match_end..brace_start].trim().is_empty() {
                    let args_raw = &text[brace_start..brace_end];
                    match Self::repair_and_parse_json(args_raw) {
                        Ok(args) => {
                            debug!(target: "server_rs::agent::runner::parser", "[parser] Extracted Gemma call '{}'", name);
                            calls.push(ToolCall { name, args });
                        }
                        Err(e) => last_error = Some(e),
                    }
                    let mut advance = brace_end;
                    let lookahead = &text[brace_end..];
                    if lookahead.starts_with("<|tool_call|>") {
                        advance += "<|tool_call|>".len();
                    } else if lookahead.starts_with("<tool_call|>") {
                        advance += "<tool_call|>".len();
                    }
                    gemma_offset = advance.max(abs_start + 1);
                    continue;
                }
            }
            gemma_offset = abs_match_end.max(abs_start + 1);
        }

        // 3. Try Groq / Llama 3 / open models function-call format: <function=name>{...}</function> or <function=name>(...)
        let mut search_offset = 0;
        while let Some(cap) = FUNCTION_START_REGEX.captures(&text[search_offset..]) {
            let rel_start = cap.get(0).unwrap().start();
            let abs_start = search_offset + rel_start;
            if let Some((name, args_str, raw_segment)) = Self::extract_single_function_call(&text[abs_start..]) {
                match Self::repair_and_parse_json(&args_str) {
                    Ok(args) => {
                        debug!(target: "server_rs::agent::runner::parser", "[parser] Extracted function call '{}'", name);
                        calls.push(ToolCall { name, args });
                    }
                    Err(e) => last_error = Some(e),
                }
                search_offset = abs_start + raw_segment.len().max(1);
            } else {
                search_offset = abs_start + cap.get(0).unwrap().end().max(1);
            }
        }

        // 4. Fallback: Bare call format (only if no calls found yet to avoid false positives)
        if calls.is_empty() {
            let mut bare_offset = 0;
            while let Some(cap) = BARE_CALL_PREFIX_REGEX.captures(&text[bare_offset..]) {
                let rel_start = cap.get(0).unwrap().start();
                let abs_start = bare_offset + rel_start;
                let abs_match_end = bare_offset + cap.get(0).unwrap().end();
                let name = cap.get(1).map(|m| m.as_str().to_string()).unwrap_or_default();

                if let Some((brace_start, brace_end)) = Self::scan_balanced_object(text, abs_match_end) {
                    if text[abs_match_end..brace_start].trim().is_empty() {
                        let args_raw = &text[brace_start..brace_end];
                        match Self::repair_and_parse_json(args_raw) {
                            Ok(args) => {
                                debug!(target: "server_rs::agent::runner::parser", "[parser] Extracted bare call '{}'", name);
                                calls.push(ToolCall { name, args });
                            }
                            Err(e) => last_error = Some(e),
                        }
                        bare_offset = brace_end.max(abs_start + 1);
                        continue;
                    }
                }
                bare_offset = abs_match_end.max(abs_start + 1);
            }
        }

        // 5. Recovery: Hallucinated <execute_tool> format
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

            // First attempt to parse directly or unwrap wrapper
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(json_str) {
                if let Some(call) = Self::unwrap_wrapper(&v) {
                    debug!(target: "server_rs::agent::runner::parser", "[parser] Extracted execute_tool call '{}' via unwrap_wrapper", call.name);
                    calls.push(call);
                    continue;
                }
            }

            // Fallback: use name_raw if valid identifier
            if WORD_PATTERN.is_match(name_raw) {
                if let Ok(args) = Self::repair_and_parse_json(json_str) {
                    calls.push(ToolCall {
                        name: name_raw.to_string(),
                        args,
                    });
                }
            }
        }

        // 6. Last Resort: Scan for fenced JSON blocks with intent prefixes
        if calls.is_empty() {
            let mut start = 0;
            while let Some((open_idx, end)) = Self::scan_balanced_object(text, start) {
                let prefix_trimmed = text[..open_idx].trim_end();
                let suffix_trimmed = text[end..].trim_start();
                let is_in_fence = (prefix_trimmed.ends_with("```json") || prefix_trimmed.ends_with("```"))
                    || suffix_trimmed.starts_with("```");
                let has_intent_prefix = prefix_trimmed.ends_with("Action:")
                    || prefix_trimmed.ends_with("Action Plan:")
                    || prefix_trimmed.ends_with("tool_call:")
                    || prefix_trimmed.ends_with("invoke:")
                    || prefix_trimmed.ends_with("tool_name:")
                    || prefix_trimmed.ends_with("Execute:");

                if is_in_fence || has_intent_prefix {
                    let potential_json = &text[open_idx..end];
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(potential_json) {
                        if let Some(call) = Self::tool_call_from_value(&v) {
                            debug!(target: "server_rs::agent::runner::parser", "[parser] Extracted tool call '{}' from last-resort scan", call.name);
                            calls.push(call);
                        }
                    }
                }
                start = end;
            }
        }

        // 7. Recovery: Truncated tool call at EOF (unclosed tag due to model token limit)
        if calls.is_empty() {
            let tags = ["<tool_call>", "<invoke_tool>", "<|tool_call|>"];
            for tag in tags {
                if let Some(idx) = text.rfind(tag) {
                    let remainder = text[idx + tag.len()..].trim();
                    if !remainder.is_empty() && (remainder.starts_with('{') || remainder.starts_with("call:")) {
                        debug!(target: "server_rs::agent::runner::parser", "[parser] Attempting recovery of unclosed EOF tag '{}'", tag);
                        if let Ok(call) = Self::parse_json_call(remainder) {
                            info!(target: "server_rs::agent::runner::parser", "[parser] Successfully salvaged truncated EOF tool call '{}'", call.name);
                            calls.push(call);
                            break;
                        }
                    }
                }
            }
        }

        // Deduplicate across extraction formats to prevent double-execution
        let initial_count = calls.len();
        let mut seen = std::collections::HashSet::new();
        calls.retain(|c| seen.insert((c.name.clone(), c.args.to_string())));
        if calls.len() < initial_count {
            warn!(
                target: "server_rs::agent::runner::parser",
                "[parser] Deduplicated {} redundant tool call(s) across formats (kept {})",
                initial_count - calls.len(),
                calls.len()
            );
        }

        if calls.is_empty() {
            if let Some(err) = last_error {
                return Err(err);
            }
            return Err(ParserError::NoCallsFound);
        }
        Ok(calls)
    }

    /// Accurately scans for a balanced JSON object starting at or after `start`.
    /// Tracks string literals and escape characters to ignore curly braces inside quotes.
    /// Returns `Some((open_brace_idx, close_brace_idx_exclusive))`.
    pub(crate) fn scan_balanced_object(s: &str, start: usize) -> Option<(usize, usize)> {
        let bytes = s.as_bytes();
        let open_idx = bytes[start..].iter().position(|&b| b == b'{')? + start;
        let mut depth: usize = 0;
        let mut in_string = false;
        let mut escape = false;

        for (i, &b) in bytes[open_idx..].iter().enumerate() {
            if in_string {
                if escape {
                    escape = false;
                } else if b == b'\\' {
                    escape = true;
                } else if b == b'"' {
                    in_string = false;
                }
            } else {
                match b {
                    b'"' => in_string = true,
                    b'{' => depth += 1,
                    b'}' => {
                        if depth > 0 {
                            depth -= 1;
                            if depth == 0 {
                                return Some((open_idx, open_idx + i + 1));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        None
    }

    /// Attempts to parse a JSON object representing a tool call (name + arguments).
    fn parse_json_call(json_str: &str) -> ParserResult<ToolCall> {
        let trimmed = json_str.trim();

        // 1. Try standard JSON object tool call parsing WITHOUT heuristic repair first
        // (Heuristic repair with regexes is lossy and corrupts valid code strings)
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
            if let Some(call) = Self::tool_call_from_value(&v) {
                return Ok(call);
            }
        }

        // 2. Try parsing with heuristic JSON repair as fallback
        let repaired = Self::repair_json(trimmed);
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&repaired) {
            if let Some(call) = Self::tool_call_from_value(&v) {
                debug!(target: "server_rs::agent::runner::parser", "[parser] Repaired malformed JSON for call '{}'", call.name);
                return Ok(call);
            }
        }

        // 3. Try parsing as bare call format (e.g. call:name{...} or name{...}) inside XML tags
        if let Some(cap) = BARE_PATTERN.captures(trimmed) {
            let name = cap
                .get(1)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            let args_raw = cap.get(2).map(|m| m.as_str()).unwrap_or("{}");
            if let Ok(args) = Self::repair_and_parse_json(args_raw) {
                if WORD_PATTERN.is_match(&name) {
                    return Ok(ToolCall { name, args });
                }
            }
        }

        // 4. Try parsing as a single word representing a tool name with no arguments
        if WORD_PATTERN.is_match(trimmed) {
            return Ok(ToolCall {
                name: trimmed.to_string(),
                args: serde_json::json!({}),
            });
        }

        // 5. Return the original JSON error if everything else failed
        match serde_json::from_str::<serde_json::Value>(&repaired) {
            Ok(_) => Err(ParserError::MissingName),
            Err(e) => Err(ParserError::InvalidJson(e.to_string())),
        }
    }

    /// Extracts a ToolCall from a serde_json::Value, handling standard schemas, flat arguments, and wrappers.
    fn tool_call_from_value(v: &serde_json::Value) -> Option<ToolCall> {
        // Direct name field: {"name": "...", "arguments": ...}
        if let Some(name) = v.get("name").and_then(|n| n.as_str()) {
            if !WORD_PATTERN.is_match(name) {
                trace!(target: "server_rs::agent::runner::parser", "[parser] Rejected invalid tool name '{}'", name);
                return None;
            }
            let args = if let Some(arguments) = v.get("arguments") {
                if let Some(s) = arguments.as_str() {
                    serde_json::from_str(s).unwrap_or_else(|_| arguments.clone())
                } else {
                    arguments.clone()
                }
            } else if let Some(args) = v.get("args").or_else(|| v.get("parameters")).or_else(|| v.get("input")) {
                args.clone()
            } else {
                // Flat arguments: take all fields except "name"
                let mut map = serde_json::Map::new();
                if let Some(obj) = v.as_object() {
                    for (k, val) in obj {
                        if k != "name" {
                            map.insert(k.clone(), val.clone());
                        }
                    }
                }
                serde_json::Value::Object(map)
            };
            return Some(ToolCall {
                name: name.to_string(),
                args,
            });
        }

        // Wrapper format: execute_tool / execute_command or {"tool_name": "..."}
        Self::unwrap_wrapper(v)
    }

    /// Extracts a normalized ToolCall from a JSON value that might be wrapped in execute_tool / execute_command structures.
    fn unwrap_wrapper(v: &serde_json::Value) -> Option<ToolCall> {
        let name_candidate = v
            .get("tool_name")
            .or_else(|| v.get("command"))
            .or_else(|| v.get("name"))
            .or_else(|| v.get("action"))
            .and_then(|n| n.as_str())?;

        let is_wrapper = name_candidate == "execute_tool" || name_candidate == "execute_command";

        let (final_name, raw_args) = if is_wrapper {
            let inner_obj = v
                .get("tool_args")
                .or_else(|| v.get("params"))
                .or_else(|| v.get("tool_input"))
                .or_else(|| v.get("input"));

            let inner_name = inner_obj
                .and_then(|i| {
                    i.get("tool_name")
                        .or_else(|| i.get("command"))
                        .or_else(|| i.get("function"))
                        .or_else(|| i.get("name"))
                        .or_else(|| i.get("action"))
                })
                .and_then(|n| n.as_str());

            let real_name = match inner_name {
                Some(n) if n != name_candidate => n,
                _ => {
                    let n = v
                        .get("tool_name")
                        .or_else(|| v.get("command"))
                        .or_else(|| v.get("function"))
                        .and_then(|n| n.as_str());
                    if n == Some(name_candidate) {
                        inner_name.unwrap_or(name_candidate)
                    } else {
                        n.unwrap_or(name_candidate)
                    }
                }
            };

            let real_args = inner_obj
                .map(|i| {
                    let mut args = i.clone();
                    if let Some(obj) = args.as_object_mut() {
                        obj.remove("tool_name");
                        obj.remove("command");
                        obj.remove("function");
                        obj.remove("name");
                        obj.remove("action");
                    }
                    args
                })
                .or_else(|| v.get("arguments").cloned())
                .unwrap_or_else(|| serde_json::json!({}));

            (real_name, real_args)
        } else {
            let args = v
                .get("tool_input")
                .or_else(|| v.get("tool_args"))
                .or_else(|| v.get("params"))
                .or_else(|| v.get("arguments"))
                .or_else(|| v.get("input"))
                .cloned()
                .unwrap_or_else(|| {
                    let mut obj = v.clone();
                    if let Some(map) = obj.as_object_mut() {
                        map.remove("tool_name");
                        map.remove("command");
                        map.remove("name");
                        map.remove("action");
                    }
                    obj
                });
            (name_candidate, args)
        };

        if !WORD_PATTERN.is_match(final_name) {
            trace!(target: "server_rs::agent::runner::parser", "[parser] Rejected invalid tool name '{}'", final_name);
            return None;
        }

        Some(ToolCall {
            name: final_name.to_string(),
            args: raw_args,
        })
    }

    /// Repairs and parses a raw JSON arguments object.
    fn repair_and_parse_json(json_str: &str) -> ParserResult<serde_json::Value> {
        let trimmed = json_str.trim();
        // Try raw parse first without repair to avoid corrupting valid code strings
        if let Ok(v) = serde_json::from_str(trimmed) {
            return Ok(v);
        }
        let repaired = Self::repair_json(trimmed);
        serde_json::from_str(&repaired).map_err(|e| ParserError::InvalidJson(e.to_string()))
    }

    /// Performs heuristic and structural repair on malformed or truncated JSON strings.
    /// - Adds quotes to unquoted keys.
    /// - Removes trailing commas.
    /// - Closes unclosed string literals truncated at EOF.
    /// - Resolves trailing colons by appending `null`.
    /// - Balances unclosed object and array brackets in LIFO order.
    pub fn repair_json(json_str: &str) -> String {
        let mut s = json_str.trim().to_string();
        if s.is_empty() {
            return "{}".to_string();
        }

        // 1. Fix unquoted keys: { key: "value" } -> { "key": "value" }
        s = KEY_REGEX.replace_all(&s, r#"$1"$2":"#).to_string();

        // 2. Remove trailing commas before existing closing brackets: [1, 2,] -> [1, 2]
        s = COMMA_REGEX.replace_all(&s, r"$1").to_string();

        // 3. Structural repair for truncated JSON:
        let mut in_string = false;
        let mut escape = false;
        let mut stack = Vec::new();

        for c in s.chars() {
            if in_string {
                if escape {
                    escape = false;
                } else if c == '\\' {
                    escape = true;
                } else if c == '"' {
                    in_string = false;
                }
            } else {
                match c {
                    '"' => in_string = true,
                    '{' => stack.push('}'),
                    '[' => stack.push(']'),
                    '}' => {
                        if let Some(pos) = stack.iter().rposition(|&x| x == '}') {
                            stack.remove(pos);
                        }
                    }
                    ']' => {
                        if let Some(pos) = stack.iter().rposition(|&x| x == ']') {
                            stack.remove(pos);
                        }
                    }
                    _ => {}
                }
            }
        }

        // If string was truncated mid-quote, close the quote
        if in_string {
            s.push('"');
        }

        // Remove any dangling trailing commas or colons before closing
        let mut trimmed_end = s.trim_end().to_string();
        while trimmed_end.ends_with(',') || trimmed_end.ends_with(':') {
            if trimmed_end.ends_with(':') {
                trimmed_end.push_str(" null");
                break;
            } else if trimmed_end.ends_with(',') {
                trimmed_end.pop();
                trimmed_end = trimmed_end.trim_end().to_string();
            }
        }
        s = trimmed_end;

        // Close any remaining unclosed brackets in LIFO order
        while let Some(closing_bracket) = stack.pop() {
            s.push(closing_bracket);
        }

        // If string didn't start with { or [ but ended with }, heal the start
        if !s.starts_with('{') && !s.starts_with('[') && s.ends_with('}') {
            s.insert(0, '{');
        }

        s
    }

    /// Extracts a single tool call from a function tag, using a balanced-brace counter to support nested JSON arguments.
    /// Returns Option<(function_name, arguments_json_string, raw_matched_segment_to_strip)>.
    pub(crate) fn extract_single_function_call(s: &str) -> Option<(String, String, String)> {
        let start_match = FUNCTION_START_REGEX.captures(s)?;
        let name = start_match.get(1)?.as_str().to_string();
        let match_start = start_match.get(0)?.start();
        let start_search_idx = start_match.get(0)?.end();

        // Find the balanced '{...}' after the tag start using the string-aware scanner
        let (brace_start, brace_end) = Self::scan_balanced_object(s, start_search_idx)?;

        // Ensure the separator between function name and opening brace is only '>', '(', whitespace, or combinations
        let sep = s[start_search_idx..brace_start].trim();
        if !sep.is_empty() && sep != ">" && sep != ">(" && sep != "(" {
            return None;
        }

        let args_json = &s[brace_start..brace_end];

        // Find the end of the entire matched segment including optional </function> or >
        let mut match_end = brace_end;
        let lookahead = &s[brace_end..];
        if lookahead.starts_with("</function>") {
            match_end += "</function>".len();
        } else if lookahead.starts_with("</function>>") {
            match_end += "</function>>".len();
        } else if lookahead.starts_with('>') {
            match_end += 1;
        }

        // Also consume any trailing closed parenthesis commonly hallucinated: ({"path": ...})
        if s[match_end..].starts_with(')') {
            match_end += 1;
        }

        let raw_match = &s[match_start..match_end];
        Some((name, args_json.to_string(), raw_match.to_string()))
    }

    /// Removes all detected tool call blocks from the text to get the clean assistant message.
    pub fn scrub_tool_calls(text: &str) -> String {
        let mut s = text.to_string();
        s = XML_TOOL_REGEX.replace_all(&s, "").to_string();
        s = EXECUTE_TOOL_REGEX.replace_all(&s, "").to_string();

        // Scrub Gemma calls
        while let Some(cap) = GEMMA_PREFIX_REGEX.captures(&s) {
            let start = cap.get(0).unwrap().start();
            let after_name = cap.get(0).unwrap().end();
            if let Some((_, brace_end)) = Self::scan_balanced_object(&s, after_name) {
                let mut end = brace_end;
                let lookahead = &s[brace_end..];
                if lookahead.starts_with("<|tool_call|>") {
                    end += "<|tool_call|>".len();
                } else if lookahead.starts_with("<tool_call|>") {
                    end += "<tool_call|>".len();
                }
                s.replace_range(start..end, "");
            } else {
                break;
            }
        }

        // Scrub bare calls
        while let Some(cap) = BARE_CALL_PREFIX_REGEX.captures(&s) {
            let start = cap.get(0).unwrap().start();
            let after_name = cap.get(0).unwrap().end();
            if let Some((_, brace_end)) = Self::scan_balanced_object(&s, after_name) {
                s.replace_range(start..brace_end, "");
            } else {
                break;
            }
        }

        // Scrub all function tag call blocks
        while let Some((_, _, raw)) = Self::extract_single_function_call(&s) {
            s = s.replacen(&raw, "", 1);
        }

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
    fn test_extract_xml_with_bare_call() {
        let input = "Here is the tool call: <tool_call>read_file{\"path\": \"foo\"}</tool_call>";
        let calls = PolyglotParser::extract(input).unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "read_file");
        assert_eq!(calls[0].args.get("path").unwrap().as_str().unwrap(), "foo");
    }

    #[test]
    fn test_extract_xml_with_single_word() {
        let input = "Done. <tool_call>complete_mission</tool_call>";
        let calls = PolyglotParser::extract(input).unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "complete_mission");
        assert!(calls[0].args.is_object());
    }

    #[test]
    fn test_scrub_tool_calls() {
        let input = "Thinking... <tool_call>...</tool_call> Done.";
        let scrubbed = PolyglotParser::scrub_tool_calls(input);
        assert_eq!(scrubbed, "Thinking...  Done.");
    }

    #[test]
    fn test_unfenced_json_is_not_extracted_as_tool() {
        let input = "Here is an example package.json configuration:\n{ \"name\": \"my-app\", \"command\": \"build\" }\nLet me know what you think!";
        let res = PolyglotParser::extract(input);
        assert!(res.is_err(), "Conversational unfenced JSON without intent markers should not be parsed as a tool call");
    }

    #[test]
    fn test_extract_function_tag_standard() {
        let input = "<function=search>{\"query\": \"tadpole os\"}</function>";
        let calls = PolyglotParser::extract(input).unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "search");
        assert_eq!(calls[0].args["query"], "tadpole os");
    }

    #[test]
    fn test_extract_function_tag_nested_json() {
        let input = "<function=search>{\"query\": \"hello\", \"filters\": {\"category\": \"news\"}}</function>";
        let calls = PolyglotParser::extract(input).unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "search");
        assert_eq!(calls[0].args["query"], "hello");
        assert_eq!(calls[0].args["filters"]["category"], "news");
    }

    #[test]
    fn test_extract_function_tag_hallucinated_parens() {
        let input = "<function=write_file>({\"path\": \"test.txt\", \"content\": \"hello world\"})";
        let calls = PolyglotParser::extract(input).unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "write_file");
        assert_eq!(calls[0].args["path"], "test.txt");
        assert_eq!(calls[0].args["content"], "hello world");
    }

    #[test]
    fn test_scrub_function_tags() {
        let input = "Checking repo... <function=search>{\"query\": \"foo\"}</function> Done search.";
        let scrubbed = PolyglotParser::scrub_tool_calls(input);
        assert_eq!(scrubbed, "Checking repo...  Done search.");
    }

    #[test]
    fn test_fallback_scanner_non_ascii_unicode() {
        let input = "Action:\n{\"tool_name\": \"search\", \"query\": \"café 👋 🚀\", \"details\": \"über cool\"}";
        let calls = PolyglotParser::extract(input).unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "search");
        assert_eq!(calls[0].args["query"], "café 👋 🚀");
    }

    #[test]
    fn test_extract_gemma_symmetric_token() {
        let input = "I will search now. <|tool_call|>call:list_files{\"path\": \".\"}<|tool_call|>";
        let calls = PolyglotParser::extract(input).unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "list_files");
    }

    #[test]
    fn test_repair_truncated_json_string() {
        let input = r#"{"name": "test", "args": {"code": "print(1)"#;
        let repaired = PolyglotParser::repair_json(input);
        let val: serde_json::Value = serde_json::from_str(&repaired).expect("Should parse healed JSON");
        assert_eq!(val["name"], "test");
        assert_eq!(val["args"]["code"], "print(1)");
    }

    #[test]
    fn test_repair_truncated_json_array() {
        let input = r#"{"items": [1, 2,"#;
        let repaired = PolyglotParser::repair_json(input);
        let val: serde_json::Value = serde_json::from_str(&repaired).expect("Should parse healed JSON");
        assert_eq!(val["items"], serde_json::json!([1, 2]));
    }

    #[test]
    fn test_repair_truncated_json_trailing_colon() {
        let input = r#"{"name": "run", "args":"#;
        let repaired = PolyglotParser::repair_json(input);
        let val: serde_json::Value = serde_json::from_str(&repaired).expect("Should parse healed JSON");
        assert_eq!(val["name"], "run");
        assert!(val["args"].is_null());
    }

    #[test]
    fn test_valid_json_with_code_string_not_corrupted() {
        let input = r#"<tool_call>{"name": "write_file", "arguments": {"path": "a.rs", "content": "struct S { name: String } // note, ref: cell"}}</tool_call>"#;
        let calls = PolyglotParser::extract(input).expect("Valid JSON with Rust struct code must not be corrupted by repair_json");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "write_file");
        let content = calls[0].args["content"].as_str().unwrap();
        assert!(content.contains("struct S { name: String }"));
    }

    #[test]
    fn test_scan_balanced_object_nested_and_strings() {
        let input = r#"<function=filter>{"pattern": "}", "depth": 2, "options": {"enabled": true}}</function>"#;
        let calls = PolyglotParser::extract(input).expect("String literal containing closing brace must not truncate arguments");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "filter");
        assert_eq!(calls[0].args["pattern"], "}");
        assert_eq!(calls[0].args["depth"], 2);
        assert_eq!(calls[0].args["options"]["enabled"], true);
    }

    #[test]
    fn test_gemma_nested_json() {
        let input = r#"I will call tool. <|tool_call|>call:configure{"network": {"proxy": "http://127.0.0.1:8080", "retry": 3}, "active": true}<|tool_call|>"#;
        let calls = PolyglotParser::extract(input).expect("Gemma call with nested JSON must not be truncated at the first brace");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "configure");
        assert_eq!(calls[0].args["network"]["retry"], 3);
        assert_eq!(calls[0].args["active"], true);
    }

    #[test]
    fn test_wrapper_stale_args_stripped_and_validated() {
        let input = r#"Action Plan: ```json { "tool_name": "execute_command", "tool_args": { "command": "recruit", "agent_id": "2", "depth": 1 } } ```"#;
        let calls = PolyglotParser::extract(input).expect("Execute command wrapper should be resolved");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "recruit");
        assert!(calls[0].args.get("command").is_none(), "command wrapper field must be pruned from tool args");
        assert!(calls[0].args.get("tool_name").is_none(), "tool_name wrapper field must be pruned from tool args");
        assert_eq!(calls[0].args["agent_id"], "2");
        assert_eq!(calls[0].args["depth"], 1);
    }

    #[test]
    fn test_wrapper_invalid_tool_name_rejected() {
        let input = r#"Action Plan: ```json { "tool_name": "execute_command", "tool_args": { "command": "rm -rf /", "target": "root" } } ```"#;
        let res = PolyglotParser::extract(input);
        assert!(res.is_err(), "Non-identifier tool names with spaces or operators must be rejected");
    }

    #[test]
    fn test_cross_format_dedup() {
        let input = r#"
        <tool_call>{"name": "reboot", "arguments": {"force": true}}</tool_call>
        <execute_tool>
          <tool_name>reboot</tool_name>
          <tool_input>{"force": true}</tool_input>
        </execute_tool>
        "#;
        let calls = PolyglotParser::extract(input).expect("Should extract tool call");
        assert_eq!(calls.len(), 1, "Duplicate tool call across XML and wrapper formats must be collapsed into one");
    }

    #[test]
    fn test_truncated_xml_tag_at_eof_salvaged() {
        let input = r#"I will execute search now. <tool_call>{"name": "find_file", "arguments": {"pattern": "main.rs"#;
        let calls = PolyglotParser::extract(input).expect("Truncated tool call at EOF without closing tag must be salvaged");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "find_file");
        assert_eq!(calls[0].args["pattern"], "main.rs");
    }

    #[test]
    fn test_flat_arguments_fallback() {
        let input = r#"<tool_call>{"name": "get_weather", "location": "Paris", "units": "celsius"}</tool_call>"#;
        let calls = PolyglotParser::extract(input).expect("Flat arguments must be preserved in args dictionary");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "get_weather");
        assert_eq!(calls[0].args["location"], "Paris");
        assert_eq!(calls[0].args["units"], "celsius");
    }
}





// Metadata: [parser]

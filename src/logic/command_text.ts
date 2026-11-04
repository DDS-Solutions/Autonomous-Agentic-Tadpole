/**
 * @docs ARCHITECTURE:Logic
 *
 * ### AI Assist Note
 * Parses and validates command text before routing it to application actions.
 *
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Malformed quoting, invalid directive length, or unsafe text.
 * - **Telemetry Link**: Check System event_bus logs for directive validation errors.
 */

import { event_bus } from '../services/event_bus';
import type { Agent } from '../types';

declare const validatedDirectiveBrand: unique symbol;
export type ValidatedDirective = string & { readonly [validatedDirectiveBrand]: true };

export const MAX_DIRECTIVE_LENGTH = 500;

/**
 * Validates directive length against system constraints.
 */
export function validate_directive_length(message: string): { valid: boolean; error?: string } {
    if (message.length > MAX_DIRECTIVE_LENGTH) {
        return {
            valid: false,
            error: `Message exceeds ${MAX_DIRECTIVE_LENGTH} character limit (${message.length} chars). Please shorten it.`
        };
    }
    return { valid: true };
}

/**
 * Hardened Directive Sanitizer
 * Strips unprintable control characters and common injection vectors (shell expansion, backticks, env vars, template tags)
 * while preserving legitimate structural whitespace (newlines, carriage returns, tabs).
 */
export function sanitize_directive(text: string): string {
    let sanitized = text
        // Keep \n (0x0A), \r (0x0D), \t (0x09); strip all other unprintable control characters
        // eslint-disable-next-line no-control-regex
        .replace(/[\x00-\x08\x0B\x0C\x0E-\x1F\x7F]/g, '')
        .replace(/\r\n?/g, '\n');

    // 1. Recursive/nested shell command substitution $(...) with self-DoS loop limiter
    let prev = '';
    let passes = 0;
    while (prev !== sanitized && sanitized.includes('$(') && passes++ < 10) {
        prev = sanitized;
        sanitized = sanitized.replace(/\$\((?:[^()]|\([^()]*\))*\)/g, '[REDACTED_SHELL]');
    }
    sanitized = sanitized.replace(/\$\([^\s)]*/g, '[REDACTED_SHELL]');

    // 2. Shell backticks (both paired and unclosed)
    sanitized = sanitized.replace(/`[^`]*`/g, '[REDACTED_TICKS]');
    sanitized = sanitized.replace(/`/g, '[REDACTED_TICKS]');

    // 3. Environment variable expansion ($VAR or ${VAR})
    sanitized = sanitized.replace(/\$(\{[a-zA-Z0-9_]+\}|[a-zA-Z_][a-zA-Z0-9_]*)/g, '[REDACTED_VAR]');

    // 4. Template tags: {{...}}
    sanitized = sanitized.replace(/\{\{[^}]*\}\}/g, '[REDACTED_TEMPLATE]');

    return sanitized.trim();
}

/**
 * Word-boundary tactical intent detection.
 * Prevents false positives on normal words (e.g. "build", "preview", "review", "guide", "suit", "seed"),
 * avoids conversational false positives on standalone "see" (e.g. "I see what you mean"),
 * and disallows infrastructure / swarm / agent status queries from hijacking the session to the local browser specialist.
 */
export function check_if_tactical(text: string): boolean {
    if (!text || typeof text !== 'string') return false;

    // Infrastructure and swarm queries belong to the orchestrator/telemetry, not UI DOM analysis
    const infra_pattern = /\b(swarm|cluster|agent|agents|node|nodes|orchestrator|backend|server|database|repo|repository|pipeline|deployment|infra|infrastructure)\b/i;
    const explicit_ui_visual = /\b(screen|screens|dom|button|buttons|on[- ]screen|in the ui)\b/i;

    if (infra_pattern.test(text) && !explicit_ui_visual.test(text)) {
        return false;
    }

    // Require explicit UI/visual keywords; do not hijack on generic 'status' or 'health' alone
    const tactical_pattern = /\b(screen|screens|button|buttons|what is this|where is|show me|look at|dom|user interface)\b|\bui\b|\bview\b|\bviews\b|\b(see this|see screen|can you see)\b/i;
    return tactical_pattern.test(text);
}

/**
 * Lexical Analysis: Splits command string by whitespace (spaces, tabs, newlines),
 * preserving quotes and handling escaped quotes cleanly.
 */
export function parse_command_tokens(command_text: string): string[] {
    const parts: string[] = [];
    let current_part = '';
    let in_quotes = false;
    let quote_char = '';

    for (let i = 0; i < command_text.length; i++) {
        const char = command_text[i];

        // Check for quote character with backslash-escape detection
        if (char === '"' || char === "'") {
            let backslash_count = 0;
            let k = i - 1;
            while (k >= 0 && command_text[k] === '\\') {
                backslash_count++;
                k--;
            }
            const is_escaped = (backslash_count % 2) === 1;

            if (!is_escaped) {
                if (in_quotes && char === quote_char) {
                    in_quotes = false;
                    parts.push(current_part);
                    current_part = '';
                    continue;
                } else if (!in_quotes) {
                    // Flush any unquoted predecessor before starting quoted string
                    if (current_part) {
                        parts.push(current_part);
                        current_part = '';
                    }
                    in_quotes = true;
                    quote_char = char;
                    continue;
                }
            }
        }

        // Delimiter: any whitespace character when not in quotes
        if (/\s/.test(char) && !in_quotes) {
            if (current_part) {
                parts.push(current_part);
                current_part = '';
            }
        } else {
            current_part += char;
        }
    }

    if (in_quotes) {
        event_bus.emit_log({
            source: 'System',
            text: 'Syntax Warning: Unclosed quote detected in command string.',
            severity: 'warning'
        });
    }

    if (current_part) {
        parts.push(current_part);
    }

    return parts;
}

/**
 * Disambiguated agent resolution helper.
 * Resolves exact match first, then unique prefix match. Rejects ambiguous matches.
 */
export function resolve_agent_unique(
    name_or_id: string | undefined,
    safe_agents: Agent[]
): { agent: Agent | null; error: string | null } {
    if (!name_or_id || !name_or_id.trim()) {
        return {
            agent: null,
            error: 'Missing agent name. Usage: @<agent-name> <message> or /send <agent-name> <message>'
        };
    }

    const trimmed = name_or_id.trim();
    const lower = trimmed.toLowerCase();

    // 1. Exact name match (case-insensitive)
    const exact_name = safe_agents.find(a => a.name.toLowerCase() === lower);
    if (exact_name) return { agent: exact_name, error: null };

    // 2. Exact ID match
    const exact_id = safe_agents.find(a => a.id === trimmed || a.id.toLowerCase() === lower);
    if (exact_id) return { agent: exact_id, error: null };

    // 3. Prefix match (starts with)
    const prefix_matches = safe_agents.filter(a =>
        a.name.toLowerCase().startsWith(lower) || a.id.toLowerCase().startsWith(lower)
    );

    if (prefix_matches.length === 1) {
        return { agent: prefix_matches[0], error: null };
    }

    if (prefix_matches.length > 1) {
        const match_list = prefix_matches.map(a => a.name).join(', ');
        return {
            agent: null,
            error: `Ambiguous agent match for "${name_or_id}". Multiple matches found: [${match_list}]. Please specify the exact name.`
        };
    }

    // 4. Substring contains match
    const contains_matches = safe_agents.filter(a =>
        a.name.toLowerCase().includes(lower)
    );

    if (contains_matches.length === 1) {
        return { agent: contains_matches[0], error: null };
    }

    if (contains_matches.length > 1) {
        const match_list = contains_matches.map(a => a.name).join(', ');
        return {
            agent: null,
            error: `Ambiguous agent match for "${name_or_id}". Multiple matches found: [${match_list}]. Please specify the exact name.`
        };
    }

    const available_list = safe_agents.map(a => a.name).slice(0, 10).join(', ') + (safe_agents.length > 10 ? '...' : '');
    return {
        agent: null,
        error: `Agent "${name_or_id}" not found. Available: ${available_list || 'none'}`
    };
}

export type Agent_Lookup = (name_or_id: string | undefined) => Agent | null;

/**
 * Resolves the primary Architect or Lead Coordinator agent from the active roster.
 *
 * @param safe_agents - Validated array of active agents.
 * @param prefer_architect_role - When true (tactical escalation), prioritizes role/name matching 'architect' before 'CEO'.
 */
export function resolve_lead_coordinator(safe_agents: Agent[], prefer_architect_role = false): Agent | undefined {
    if (prefer_architect_role) {
        return (
            safe_agents.find(
                a =>
                    a.role?.toLowerCase() === 'architect' ||
                    a.name.toLowerCase().includes('architect') ||
                    a.name === 'Tadpole_Alpha'
            ) ||
            safe_agents.find(a => a.name === 'CEO') ||
            safe_agents[0]
        );
    }

    return (
        safe_agents.find(
            a =>
                a.name === 'Tadpole_Alpha' ||
                a.name === 'CEO' ||
                a.role?.toLowerCase() === 'architect'
        ) || safe_agents[0]
    );
}

/**
 * Sanitizes a raw directive string and validates that it is non-empty and within length bounds.
 * Emits the appropriate error log to `event_bus` when validation fails.
 */
export function prepare_validated_directive(raw_text: string, usage_hint: string): ValidatedDirective | null {
    const message = sanitize_directive(raw_text);
    if (!message) {
        event_bus.emit_log({
            source: 'System',
            text: usage_hint,
            severity: 'error'
        });
        return null;
    }

    const len_check = validate_directive_length(message);
    if (!len_check.valid) {
        event_bus.emit_log({
            source: 'System',
            text: len_check.error!,
            severity: 'error'
        });
        return null;
    }

    return message as ValidatedDirective;
}

/**
 * Executes Sentinel-tier tactical UI interception via the local Browser Specialist,
 * escalating to the Computer Architect if the tactical threshold is exceeded.
 */

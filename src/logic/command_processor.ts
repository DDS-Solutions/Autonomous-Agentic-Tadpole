/**
 * @docs ARCHITECTURE:Logic
 * @docs OPERATIONS_MANUAL:Commands
 * 
 * ### AI Assist Note
 * **NLP Orchestrator**: Manages the translation of user intent (slash commands, @mentions, #clusters) into actionable system directives. 
 * Implements lexical analysis with quote preservation, deterministic sanitization, and disambiguated agent resolution.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Lexical parsing errors, ambiguous agent resolution, command syntax exceptions, or API timeouts.
 * - **Telemetry Link**: Search for `[CommandProcessor]` in `event_bus` logs or `process_command` trace spans.
 */

import { event_bus } from '../services/event_bus';
import { agent_api_service } from '../services/agent_api_service';
import { system_api_service } from '../services/system_api_service';
import { resolve_agent_model_config } from '../utils/model_utils';
import { use_workspace_store } from '../stores/workspace_store';
import { use_sovereign_store } from '../stores/sovereign_store';
import { get_settings } from '../stores/settings_store';
import { use_skill_store } from '../stores/skill_store';
import type { Agent } from '../types';
import { browser_inference_service } from '../services/browser_inference';
import { use_browser_specialist_store } from '../stores/browser_specialist_store';
import { use_trace_store } from '../stores/trace_store';
import { v4 as uuidv4 } from 'uuid';
import { scan_and_redact_secrets } from '../utils/security_utils';

/** Return value from process_command indicating if the log should be cleared. */
export interface Command_Result {
    /** If true, the Terminal should wipe its local log state. */
    should_clear_logs: boolean;
}

/** State for two-step deploy confirmation flow */
let pending_deploy_confirmation = false;
let deploy_confirm_timeout: ReturnType<typeof setTimeout> | null = null;

export function reset_deploy_confirmation_for_testing(): void {
    pending_deploy_confirmation = false;
    if (deploy_confirm_timeout) {
        clearTimeout(deploy_confirm_timeout);
        deploy_confirm_timeout = null;
    }
}

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
 * Prevents false positives on normal words (e.g. "build", "preview", "review", "guide", "suit", "seed")
 * and avoids conversational false positives on standalone "see" (e.g. "I see what you mean").
 */
export function check_if_tactical(text: string): boolean {
    const tactical_pattern = /\b(status|healthy|health|screen|screens|button|buttons|what is this|where is|show me|look at|dom|user interface)\b|\bui\b|\bview\b|\bviews\b|\b(see this|see screen|can you see)\b/i;
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

/**
 * process_command
 * Processes a single slash-command string from the user.
 * Supports standard slash commands (/help, /clear), agent-specific targeting (@agent), 
 * cluster-specific targeting (#cluster), and swarm broadcasts.
 */
export async function process_command(
    raw_command_text: string,
    agents: Agent[],
    is_safe_mode?: boolean,
    active_scope: 'agent' | 'cluster' | 'swarm' = 'swarm',
    target_node?: string
): Promise<Command_Result> {
    const telemetry_source = '[CommandProcessor]';

    // 0. Pre-Flight DLP Shield: Scan & Redact Secrets before processing or cloud forwarding
    const secret_scan = scan_and_redact_secrets(raw_command_text);
    let command_text = raw_command_text;
    if (secret_scan.has_secrets) {
        command_text = secret_scan.sanitized;
        event_bus.emit_log({
            source: 'System',
            text: `🛡️ [Pre-Flight DLP Shield] Redacted ${secret_scan.redacted_count} credential(s) (${secret_scan.detected_types.join(', ')}) from outgoing prompt.`,
            severity: 'warning'
        });
    }

    const safe_agents = Array.isArray(agents) ? agents : [];

    // 1. Lexical Analysis with whitespace delimiters and quote support
    const parts = parse_command_tokens(command_text);
    if (parts.length === 0) return { should_clear_logs: false };

    const cmd = parts[0].toLowerCase();
    const args = parts.slice(1);
    const settings = get_settings();

    // 1.1 Tactical Interception (Sentinel Tier)
    if (settings.sentinel_mode && !cmd.startsWith('/') && !cmd.startsWith('@') && !cmd.startsWith('#')) {
        const is_tactical = check_if_tactical(command_text);
        if (is_tactical) {
            console.debug(`${telemetry_source} [TieredRouting] Tactical intent detected. Routing to Browser Specialist.`);
            event_bus.emit_log({ source: 'System', text: '🧠 Browser Specialist analyzing tactical intent...', severity: 'info' });

            const specialist_store = use_browser_specialist_store.getState();
            const trace_store = use_trace_store.getState();

            const span_id = uuidv4().substring(0, 16);
            const trace_id = trace_store.active_trace_id || uuidv4().replace(/-/g, '');
            const start_time = performance.now();

            trace_store.add_span({
                id: span_id,
                trace_id,
                name: 'tactical_ui_analysis',
                agent_id: 'Browser Specialist',
                mission_id: 'active_session',
                start_time: Date.now(),
                status: 'running',
                attributes: { intent: command_text }
            });

            try {
                const analysis = await specialist_store.analyze_dom(command_text);
                const duration = performance.now() - start_time;
                const should_escalate = use_browser_specialist_store.getState().last_escalate || analysis.includes('ESCALATE_TO_ARCHITECT');

                trace_store.update_span(span_id, {
                    status: 'success',
                    end_time: Date.now(),
                    attributes: { 
                        intent: command_text, 
                        latency_ms: duration,
                        is_escalated: should_escalate
                    }
                });

                event_bus.emit_log({ 
                    source: 'Agent', 
                    agent_id: 'Browser Specialist', 
                    text: analysis, 
                    severity: 'success' 
                });

                if (should_escalate) {
                    event_bus.emit_log({
                        source: 'System',
                        text: '⚠️ Tactical threshold exceeded. Escalating to Computer Architect...',
                        severity: 'warning'
                    });

                    // Route escalation directly to Architect agent
                    const architect = safe_agents.find(a => 
                        a.role?.toLowerCase() === 'architect' || 
                        a.name.toLowerCase().includes('architect') || 
                        a.name === 'Tadpole_Alpha'
                    ) || safe_agents.find(a => a.name === 'CEO') || safe_agents[0];

                    if (architect) {
                        const escalated_raw = `[ESCALATED_TACTICAL]: ${command_text}`;
                        const escalated_msg = escalated_raw.length > MAX_DIRECTIVE_LENGTH
                            ? escalated_raw.substring(0, MAX_DIRECTIVE_LENGTH - 3) + '...'
                            : escalated_raw;

                        await dispatch_directive(
                            architect, 
                            escalated_msg, 
                            !!is_safe_mode, 
                            'agent', 
                            undefined, 
                            undefined, 
                            `@${architect.name}`
                        );
                    }
                    return { should_clear_logs: false };
                }

                return { should_clear_logs: false };
            } catch (err: unknown) {
                const msg = err instanceof Error ? err.message : String(err);
                trace_store.update_span(span_id, {
                    status: 'error',
                    end_time: Date.now(),
                    attributes: { error: msg }
                });
                event_bus.emit_log({
                    source: 'System',
                    text: `Tactical UI analysis error: ${msg}`,
                    severity: 'error'
                });
                return { should_clear_logs: false };
            }
        }
    }

    const find_agent = (name_or_id: string | undefined): Agent | null => {
        const { agent, error } = resolve_agent_unique(name_or_id, safe_agents);
        if (error) {
            event_bus.emit_log({ source: 'System', text: error, severity: 'error' });
            return null;
        }
        return agent;
    };

    // 2. Routing based on command prefix
    if (cmd.startsWith('/')) {
        return handle_slash_command(cmd, args, safe_agents, find_agent, is_safe_mode);
    }

    if (cmd.startsWith('@')) {
        return handle_agent_mention(cmd, args, find_agent, is_safe_mode);
    }

    if (cmd.startsWith('#')) {
        return handle_cluster_mention(cmd, args, safe_agents, is_safe_mode);
    }

    // 3. Auto-Routing based on active UI scope
    if (active_scope !== 'swarm' && target_node) {
        console.debug(`${telemetry_source} Auto-routing intent to ${active_scope}:${target_node}`);
        if (active_scope === 'cluster') {
            return handle_cluster_mention(`#${target_node}`, [command_text], safe_agents, is_safe_mode);
        } else {
            return handle_agent_mention(`@${target_node}`, [command_text], find_agent, is_safe_mode);
        }
    }

    // 4. Default Swarm Broadcast
    return handle_swarm_broadcast(parts, safe_agents, is_safe_mode);
}

/**
 * Consolidates directive dispatching logic.
 * Enforces mandatory DLP and shell sanitization on every directive prior to transmission.
 */
async function dispatch_directive(
    agent: Agent,
    raw_message: string,
    is_safe_mode: boolean,
    scope: 'agent' | 'cluster' = 'agent',
    cluster_id?: string,
    department?: string,
    display_name?: string
): Promise<void> {
    const settings = get_settings();
    const { model_id, provider } = resolve_agent_model_config(agent, settings.default_model);
    const target_display = display_name || agent.name;

    // Enforce universal sanitization at the dispatch boundary
    const message = sanitize_directive(raw_message);
    if (!message) {
        event_bus.emit_log({ source: 'System', text: 'Cannot dispatch empty directive.', severity: 'error' });
        return;
    }

    // 1. Log & Echo
    event_bus.emit_log({ source: 'User', text: `→ ${target_display}: ${message}`, severity: 'info' });

    // 2. Neural Link Acknowledgment (cancellable timer to avoid race-after-failure)
    let ack_timer: ReturnType<typeof setTimeout> | undefined;
    ack_timer = setTimeout(() => {
        const clean_name = (target_display.startsWith('@') || target_display.startsWith('#'))
            ? target_display.substring(1)
            : target_display;

        const reply = scope === 'cluster'
            ? `Neural Link: Distributing directive to ${clean_name}...`
            : `Neural Link: Routing directive to ${clean_name}...`;
        event_bus.emit_log({ source: 'System', text: reply, severity: 'info' });
        use_sovereign_store.getState().add_message({
            sender_id: 'system',
            sender_name: 'Neural System',
            agent_id: agent.id,
            text: reply,
            scope: scope
        });
    }, 100);

    // 3. Predictive Skills (Local browser specialist filtering)
    let enabled_skills: string[] | undefined = undefined;
    if (scope === 'agent' && settings.sentinel_mode) {
        const skill_store = use_skill_store.getState();
        const all_skill_names = [
            ...skill_store.manifests.map(m => m.name),
            ...skill_store.scripts.map(s => s.name),
            ...skill_store.mcp_tools.map(t => t.name)
        ];
        if (all_skill_names.length > 5) {
            try {
                enabled_skills = await browser_inference_service.predict_relevant_skills(message, all_skill_names);
                if (enabled_skills && enabled_skills.length > 0) {
                    event_bus.emit_log({ 
                        source: 'System', 
                        text: `🧠 Gemma predictive filter: [${enabled_skills.join(', ')}]`, 
                        severity: 'info' 
                    });
                }
            } catch (e) {
                console.warn('[CommandProcessor] Predictive filtering failed:', e);
            }
        }
    }

    // 4. Trigger API Call
    const active_node_id = use_sovereign_store.getState().active_node_id;
    try {
        const task_id = await agent_api_service.send_command(
            agent.id, 
            message, 
            model_id, 
            provider, 
            cluster_id, 
            department, 
            undefined, 
            undefined, 
            !!is_safe_mode, 
            undefined, 
            undefined, 
            active_node_id || undefined, 
            enabled_skills
        );

        // Clear ack timer on rapid successful resolution to prevent out-of-order acknowledgment logs
        if (ack_timer) {
            clearTimeout(ack_timer);
            ack_timer = undefined;
        }

        // 5. Asynchronous Audit Tracking (Non-blocking background subscription)
        if (scope === 'agent' && (message.toLowerCase().includes('audit') || message.toLowerCase().includes('integrity'))) {
            void agent_api_service.poll_task_status(agent.id, task_id).then(status => {
                if (status === 'success') {
                    event_bus.emit_log({ source: 'System', text: `✅ Task ${task_id.slice(0, 8)} resolved: Mission Objectives Met.`, severity: 'success' });
                } else if (status === 'error') {
                    event_bus.emit_log({ source: 'System', text: `❌ Task ${task_id.slice(0, 8)} resolved: Failure Detected in Audit Trail.`, severity: 'error' });
                }
            }).catch(() => {/* Ignore polling errors in background */});
        }
    } catch (err) {
        if (ack_timer) {
            clearTimeout(ack_timer);
        }
        const error_msg = err instanceof Error ? err.message : String(err);
        event_bus.emit_log({
            source: 'System',
            text: `${scope === 'cluster' ? 'Cluster' : 'Neural'} link failed: ${error_msg}`,
            severity: 'error'
        });
    }
}

/**
 * Handles all standard slash commands.
 */
async function handle_slash_command(
    cmd: string,
    args: string[],
    safe_agents: Agent[],
    find_agent: (name_or_id: string | undefined) => Agent | null,
    is_safe_mode?: boolean
): Promise<Command_Result> {
    switch (cmd) {
        // ────────────── HELP ──────────────
        case '/help': {
            event_bus.emit_log({
                source: 'System',
                text: [
                    '📋 Available Commands:',
                    '  /help              — Show this command reference',
                    '  /clear             — Clear terminal log history',
                    '  /status            — Swarm cluster operational summary',
                    '  /deploy            — Request production deployment (requires /deploy confirm)',
                    '  /pre-pr            — Trigger Pre-PR Quality Gates',
                    '  /config <name>     — View detailed agent configuration',
                    '  /switch <name> [1-3] — Switch active model slot (1-3)',
                    '  /pause <name>      — Pause agent execution',
                    '  /resume <name>     — Resume paused agent',
                    '  /send <name> <msg> — Direct message to agent',
                    '  @<agent> <msg>     — Shorthand agent directive targeting',
                    '  #<cluster> <msg>   — Shorthand mission cluster targeting',
                    '  /swarm status      — Show mission cluster inventory',
                    '  /swarm optimize    — Trigger swarm re-alignment'
                ].join('\n'),
                severity: 'info'
            });
            return { should_clear_logs: false };
        }

        // ────────────── CLEAR ──────────────
        case '/clear': {
            event_bus.clear_history();
            return { should_clear_logs: true };
        }

        // ────────────── STATUS ──────────────
        case '/status': {
            const active = safe_agents.filter(a => a.status === 'active' || a.status === 'thinking' || a.status === 'coding').length;
            const idle = safe_agents.filter(a => a.status === 'idle').length;
            const offline = safe_agents.filter(a => a.status === 'offline').length;
            const total_tokens = safe_agents.reduce((sum, a) => sum + (a.tokens_used || 0), 0);

            event_bus.emit_log({
                source: 'System',
                text: `Swarm Status: ${active} active · ${idle} idle · ${offline} offline | Total tokens: ${(total_tokens / 1000).toFixed(1)}k`,
                severity: 'success'
            });
            return { should_clear_logs: false };
        }

        // ────────────── DEPLOY (Stateful 2-Step Confirmation Flow) ──────────────
        case '/deploy': {
            const is_confirm = args[0]?.toLowerCase() === 'confirm';

            if (!is_confirm) {
                pending_deploy_confirmation = true;
                if (deploy_confirm_timeout) clearTimeout(deploy_confirm_timeout);
                deploy_confirm_timeout = setTimeout(() => {
                    pending_deploy_confirmation = false;
                    deploy_confirm_timeout = null;
                }, 30000); // 30 second expiration window

                event_bus.emit_log({
                    source: 'System',
                    text: '⚠️ Production deployment requested. Type "/deploy confirm" within 30 seconds to execute.',
                    severity: 'warning'
                });
                return { should_clear_logs: false };
            }

            // User typed "/deploy confirm"
            if (!pending_deploy_confirmation) {
                event_bus.emit_log({
                    source: 'System',
                    text: '❌ No pending deployment request. Type "/deploy" first to initiate the confirmation challenge.',
                    severity: 'error'
                });
                return { should_clear_logs: false };
            }

            // Validated challenge
            pending_deploy_confirmation = false;
            if (deploy_confirm_timeout) {
                clearTimeout(deploy_confirm_timeout);
                deploy_confirm_timeout = null;
            }

            event_bus.emit_log({
                source: 'System',
                text: '🚀 Triggering deployment to Swarm Bunker via /engine/deploy...',
                severity: 'warning'
            });

            try {
                const data = await system_api_service.deploy_engine();
                event_bus.emit_log({
                    source: 'System',
                    text: `✅ Deployment successful. Output: ${(data.output || '').slice(-300)}`,
                    severity: 'success'
                });
            } catch (e: unknown) {
                const error_msg = e instanceof Error ? e.message : String(e);
                event_bus.emit_log({
                    source: 'System',
                    text: `❌ Deployment error: ${error_msg}`,
                    severity: 'error'
                });
            }
            return { should_clear_logs: false };
        }

        // ────────────── PRE-PR (Quality Gate) ──────────────
        case '/pre-pr': {
            event_bus.emit_log({
                source: 'System',
                text: '🔍 Starting Pre-PR Quality Gate verification...',
                severity: 'info'
            });

            try {
                const data = await system_api_service.pre_pr_engine();
                if (data.status === 'success') {
                    event_bus.emit_log({
                        source: 'System',
                        text: `✅ Pre-PR Gate succeeded!\n\n${data.output || ''}`,
                        severity: 'success'
                    });
                } else {
                    event_bus.emit_log({
                        source: 'System',
                        text: `❌ Pre-PR Gate failed!\n\nOutput:\n${data.output || ''}\n\nError details:\n${data.error || ''}`,
                        severity: 'error'
                    });
                }
            } catch (e: unknown) {
                const error_msg = e instanceof Error ? e.message : String(e);
                event_bus.emit_log({
                    source: 'System',
                    text: `❌ Pre-PR Gate fault: ${error_msg}`,
                    severity: 'error'
                });
            }
            return { should_clear_logs: false };
        }

        // ────────────── CONFIG ──────────────
        case '/config': {
            if (!args[0]) {
                event_bus.emit_log({
                    source: 'System',
                    text: 'Usage: /config <agent-name>',
                    severity: 'error'
                });
                return { should_clear_logs: false };
            }

            const agent = find_agent(args[0]);
            if (!agent) return { should_clear_logs: false };

            const raw_cfg = agent.model_config as Record<string, unknown> | undefined;
            const prompt_preview = raw_cfg?.systemPrompt || raw_cfg?.system_prompt;
            const prompt_str = typeof prompt_preview === 'string' ? prompt_preview.substring(0, 80) + '...' : '(none)';

            event_bus.emit_log({
                source: 'System',
                text: [
                    `⚙️ Config for ${agent.name}:`,
                    `  Model: ${agent.model}`,
                    `  Temperature: ${raw_cfg?.temperature ?? 'default'}`,
                    `  Status: ${agent.status}`,
                    `  Prompt: ${prompt_str}`,
                ].join('\n'),
                severity: 'info'
            });
            return { should_clear_logs: false };
        }

        // ────────────── PAUSE ──────────────
        case '/pause': {
            const agent = find_agent(args[0]);
            if (!agent) return { should_clear_logs: false };

            try {
                const success = await agent_api_service.pause_agent(agent.id);
                event_bus.emit_log({
                    source: 'System',
                    text: success
                        ? `⏸️ Agent ${agent.name} paused via TadpoleOS.`
                        : `⏸️ Agent ${agent.name} paused locally (TadpoleOS offline).`,
                    severity: 'warning'
                });
            } catch (err: unknown) {
                const msg = err instanceof Error ? err.message : String(err);
                event_bus.emit_log({
                    source: 'System',
                    text: `Failed to pause agent ${agent.name}: ${msg}`,
                    severity: 'error'
                });
            }
            return { should_clear_logs: false };
        }

        // ────────────── RESUME ──────────────
        case '/resume': {
            const agent = find_agent(args[0]);
            if (!agent) return { should_clear_logs: false };

            try {
                const success = await agent_api_service.resume_agent(agent.id);
                event_bus.emit_log({
                    source: 'System',
                    text: success
                        ? `▶️ Agent ${agent.name} resumed via TadpoleOS.`
                        : `▶️ Agent ${agent.name} resumed locally (TadpoleOS offline).`,
                    severity: 'success'
                });
            } catch (err: unknown) {
                const msg = err instanceof Error ? err.message : String(err);
                event_bus.emit_log({
                    source: 'System',
                    text: `Failed to resume agent ${agent.name}: ${msg}`,
                    severity: 'error'
                });
            }
            return { should_clear_logs: false };
        }

        // ────────────── SEND ──────────────
        case '/send': {
            if (!args[0]) {
                event_bus.emit_log({
                    source: 'System',
                    text: 'Usage: /send <agent-name> <message>',
                    severity: 'error'
                });
                return { should_clear_logs: false };
            }

            const agent = find_agent(args[0]);
            if (!agent) return { should_clear_logs: false };

            const message = sanitize_directive(args.slice(1).join(' '));
            if (!message) {
                event_bus.emit_log({
                    source: 'System',
                    text: 'Usage: /send <agent-name> <message>',
                    severity: 'error'
                });
                return { should_clear_logs: false };
            }

            const len_check = validate_directive_length(message);
            if (!len_check.valid) {
                event_bus.emit_log({
                    source: 'System',
                    text: len_check.error!,
                    severity: 'error'
                });
                return { should_clear_logs: false };
            }

            await dispatch_directive(agent, message, !!is_safe_mode, 'agent', undefined, undefined, agent.name);
            return { should_clear_logs: false };
        }

        // ────────────── SWARM ──────────────
        case '/swarm': {
            const workspace_store = use_workspace_store.getState();
            const sub_cmd = args[0]?.toLowerCase();

            if (sub_cmd === 'status') {
                const cluster_info = workspace_store.clusters.map(c =>
                    `🔹 ${c.name} [${c.theme.toUpperCase()}]\n` +
                    `  Alpha: ${safe_agents.find(a => a.id === c.alpha_id)?.name || 'NONE'}\n` +
                    `  Objective: ${c.objective || 'No objective set'}\n` +
                    `  Collaborators: ${c.collaborators.length}`
                ).join('\n\n');

                event_bus.emit_log({
                    source: 'System',
                    text: `🌐 Mission Cluster Inventory:\n\n${cluster_info}`,
                    severity: 'info'
                });
            } else if (sub_cmd === 'optimize') {
                event_bus.emit_log({
                    source: 'System',
                    text: '⚡ Initiating global swarm optimization...',
                    severity: 'warning'
                });

                workspace_store.clusters.forEach(cluster => {
                    workspace_store.generate_proposal(cluster.id);
                    const proposal = use_workspace_store.getState().active_proposals[cluster.id];

                    if (proposal) {
                        setTimeout(() => {
                            event_bus.emit_log({
                                source: 'Agent',
                                agent_id: safe_agents.find(a => a.id === cluster.alpha_id)?.name || 'Alpha Node',
                                text: proposal.reasoning,
                                severity: 'info'
                            });
                        }, 500 + Math.random() * 1000);
                    }
                });
            } else {
                event_bus.emit_log({
                    source: 'System',
                    text: 'Usage: /swarm <status|optimize>',
                    severity: 'error'
                });
            }
            return { should_clear_logs: false };
        }

        // ────────────── SWITCH ──────────────
        case '/switch': {
            if (!args[0] || !args[1]) {
                event_bus.emit_log({ source: 'System', text: 'Usage: /switch <agent-name> <1|2|3>', severity: 'error' });
                return { should_clear_logs: false };
            }

            const agent = find_agent(args[0]);
            if (!agent) return { should_clear_logs: false };

            const slot_val = parseInt(args[1], 10);
            if (isNaN(slot_val) || slot_val < 1 || slot_val > 3) {
                event_bus.emit_log({ source: 'System', text: 'Invalid slot. Use 1, 2, or 3.', severity: 'error' });
                return { should_clear_logs: false };
            }

            const slot = slot_val as 1 | 2 | 3;
            try {
                await agent_api_service.update_agent(agent.id, { active_model_slot: slot });
                event_bus.emit_log({
                    source: 'System',
                    text: `Agent ${agent.name} switched to Neural Slot ${slot}.`,
                    severity: 'success'
                });
            } catch (err: unknown) {
                const msg = err instanceof Error ? err.message : String(err);
                event_bus.emit_log({
                    source: 'System',
                    text: `Failed to switch slot for ${agent.name}: ${msg}`,
                    severity: 'error'
                });
            }
            return { should_clear_logs: false };
        }

        default: {
            event_bus.emit_log({
                source: 'System',
                text: `Unknown command: ${cmd}. Type /help for available commands.`,
                severity: 'error'
            });
            return { should_clear_logs: false };
        }
    }
}

/**
 * Handles targeting an individual agent via @mention.
 */
async function handle_agent_mention(
    cmd: string,
    args: string[],
    find_agent: (name_or_id: string | undefined) => Agent | null,
    is_safe_mode?: boolean
): Promise<Command_Result> {
    const target_name = cmd.substring(1).replace(/:$/, '');
    if (!target_name.trim()) {
        event_bus.emit_log({
            source: 'System',
            text: 'Missing agent name. Usage: @<agent-name> <message>',
            severity: 'error'
        });
        return { should_clear_logs: false };
    }

    const agent = find_agent(target_name);
    if (agent) {
        const raw_message = args.join(' ');
        const message = sanitize_directive(raw_message);
        if (!message) {
            event_bus.emit_log({
                source: 'System',
                text: `Usage: @${agent.name} <message>`,
                severity: 'error'
            });
            return { should_clear_logs: false };
        }
        const len_check = validate_directive_length(message);
        if (!len_check.valid) {
            event_bus.emit_log({
                source: 'System',
                text: len_check.error!,
                severity: 'error'
            });
            return { should_clear_logs: false };
        }
        await dispatch_directive(agent, message, !!is_safe_mode, 'agent', undefined, undefined, `@${agent.name}`);
    }
    return { should_clear_logs: false };
}

/**
 * Handles targeting a specific mission cluster via #name.
 */
async function handle_cluster_mention(
    cmd: string,
    args: string[],
    safe_agents: Agent[],
    is_safe_mode?: boolean
): Promise<Command_Result> {
    const cluster_name = cmd.substring(1).toLowerCase();
    if (!cluster_name.trim()) {
        event_bus.emit_log({
            source: 'System',
            text: 'Missing cluster name. Usage: #<cluster-name> <message>',
            severity: 'error'
        });
        return { should_clear_logs: false };
    }

    const workspace_store = use_workspace_store.getState();
    const cluster = workspace_store.clusters.find(c => (c.name?.toLowerCase() === cluster_name) || (c.id?.toLowerCase() === cluster_name));

    if (!cluster) {
        event_bus.emit_log({
            source: 'System',
            text: `Cluster "${cluster_name}" not found.`,
            severity: 'error'
        });
        return { should_clear_logs: false };
    }

    if (!cluster.alpha_id) {
        event_bus.emit_log({
            source: 'System',
            text: `Cluster "${cluster.name}" lacks an assigned Alpha node.`,
            severity: 'error'
        });
        return { should_clear_logs: false };
    }

    const alpha_agent = safe_agents.find(a => a.id === cluster.alpha_id);
    if (!alpha_agent) {
        event_bus.emit_log({
            source: 'System',
            text: `Cluster "${cluster.name}" Alpha node (ID: ${cluster.alpha_id}) not found in active agent roster.`,
            severity: 'error'
        });
        return { should_clear_logs: false };
    }

    const raw_message = args.join(' ');
    const message = sanitize_directive(raw_message);
    if (!message) {
        event_bus.emit_log({
            source: 'System',
            text: `Usage: #${cluster.name} <message>`,
            severity: 'error'
        });
        return { should_clear_logs: false };
    }
    const len_check = validate_directive_length(message);
    if (!len_check.valid) {
        event_bus.emit_log({
            source: 'System',
            text: len_check.error!,
            severity: 'error'
        });
        return { should_clear_logs: false };
    }

    await dispatch_directive(alpha_agent, message, !!is_safe_mode, 'cluster', cluster.id, cluster.department, `#${cluster.name}`);
    return { should_clear_logs: false };
}

/**
 * Handles broadcasting to the entire swarm.
 * Dispatches to the swarm lead coordinator node (Tadpole_Alpha / CEO / primary agent).
 */
async function handle_swarm_broadcast(
    parts: string[],
    safe_agents: Agent[],
    is_safe_mode?: boolean
): Promise<Command_Result> {
    if (safe_agents.length === 0) {
        event_bus.emit_log({
            source: 'System',
            text: 'Swarm broadcast failed: No active agents available in roster.',
            severity: 'error'
        });
        return { should_clear_logs: false };
    }

    const raw_message = parts.join(' ');
    const message = sanitize_directive(raw_message);
    if (!message) return { should_clear_logs: false };

    event_bus.emit_log({ source: 'User', text: `Swarm Broadcast: ${message}`, severity: 'info' });

    const reply = `Broadcasting to swarm: ${message.substring(0, 30)}...`;
    event_bus.emit_log({
        source: 'System',
        text: reply,
        severity: 'info'
    });
    use_sovereign_store.getState().add_message({
        sender_id: 'system',
        sender_name: 'Neural System',
        text: reply,
        scope: 'swarm'
    });

    // Find coordinator node to receive the swarm broadcast
    const lead_agent = safe_agents.find(a => 
        a.name === 'Tadpole_Alpha' || 
        a.name === 'CEO' || 
        a.role?.toLowerCase() === 'architect'
    ) || safe_agents[0];

    if (lead_agent) {
        await dispatch_directive(lead_agent, message, !!is_safe_mode, 'agent', undefined, undefined, 'Swarm Broadcast');
    }

    return { should_clear_logs: false };
}

// Metadata: [command_processor]

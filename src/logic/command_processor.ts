/**
 * @docs ARCHITECTURE:Logic
 * @docs OPERATIONS_MANUAL:Commands
 *
 * ### AI Assist Note
 * **NLP Command Orchestrator Facade**: Single entry boundary for user chat/terminal commands.
 * Enforces Pre-Flight DLP secret redaction unconditionally before delegating to modular
 * handlers under `src/logic/commands/` (`tactical_interceptor`, `slash_handlers`, `directive_dispatcher`).
 *
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Lexical parsing errors, ambiguous agent resolution, or API timeouts.
 * - **Telemetry Link**: Search for `[CommandProcessor]` in `event_bus` logs or `process_command` trace spans.
 */

import { event_bus } from '../services/event_bus';
import { get_settings } from '../stores/settings_store';
import type { Agent, Execution_Mode } from '../types';
import { scan_and_redact_secrets } from '../utils/security_utils';
import {
    check_if_tactical,
    parse_command_tokens,
    resolve_agent_unique,
    type Agent_Lookup,
} from './command_text';
import {
    handle_agent_mention,
    handle_cluster_mention,
    handle_swarm_broadcast,
    type Command_Result,
} from './commands/directive_dispatcher';
import { handle_tactical_interception } from './commands/tactical_interceptor';
import {
    handle_slash_command,
    reset_deploy_confirmation_for_testing,
} from './commands/slash_handlers';

export {
    MAX_DIRECTIVE_LENGTH,
    check_if_tactical,
    parse_command_tokens,
    resolve_agent_unique,
    resolve_lead_coordinator,
    sanitize_directive,
    validate_directive_length,
} from './command_text';
export { reset_deploy_confirmation_for_testing };
export type { Command_Result };

/**
 * Processes a single slash-command or natural-language string from the operator.
 * Supports slash commands (`/help`, `/clear`, `/deploy`), agent targeting (`@agent`),
 * cluster targeting (`#cluster`), tactical Browser Sentinel routing, and swarm broadcasts.
 */
export async function process_command(
    raw_command_text: string,
    agents: Agent[],
    is_safe_mode?: boolean,
    active_scope: 'agent' | 'cluster' | 'swarm' = 'swarm',
    target_node?: string,
    execution_mode?: Execution_Mode,
    recent_findings?: string
): Promise<Command_Result> {
    const telemetry_source = '[CommandProcessor]';
    const effective_safe_mode = execution_mode ? execution_mode !== 'swarm' : is_safe_mode;

    // 0. Pre-Flight DLP Shield: Scan & Redact Secrets unconditionally before any downstream routing
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
        if (check_if_tactical(command_text)) {
            return handle_tactical_interception(command_text, safe_agents, is_safe_mode);
        }
    }

    const find_agent: Agent_Lookup = (name_or_id: string | undefined): Agent | null => {
        const { agent, error } = resolve_agent_unique(name_or_id, safe_agents);
        if (error) {
            event_bus.emit_log({ source: 'System', text: error, severity: 'error' });
            return null;
        }
        return agent;
    };

    // 2. Routing based on command prefix
    if (cmd.startsWith('/')) {
        return handle_slash_command(cmd, args, safe_agents, find_agent, effective_safe_mode, execution_mode, recent_findings);
    }

    if (cmd.startsWith('@')) {
        return handle_agent_mention(cmd, args, find_agent, effective_safe_mode, execution_mode, recent_findings);
    }

    if (cmd.startsWith('#')) {
        return handle_cluster_mention(cmd, args, safe_agents, effective_safe_mode, execution_mode, recent_findings);
    }

    // 3. Auto-Routing based on active UI scope
    if (active_scope !== 'swarm' && target_node) {
        console.debug(`${telemetry_source} Auto-routing intent to ${active_scope}:${target_node}`);
        if (active_scope === 'cluster') {
            return handle_cluster_mention(`#${target_node}`, [command_text], safe_agents, effective_safe_mode, execution_mode, recent_findings);
        } else {
            return handle_agent_mention(`@${target_node}`, [command_text], find_agent, effective_safe_mode, execution_mode, recent_findings);
        }
    }

    // 4. Default Swarm Broadcast
    return handle_swarm_broadcast(parts, safe_agents, effective_safe_mode, execution_mode, recent_findings);
}

// Metadata: [command_processor]

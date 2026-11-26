/**
 * @docs ARCHITECTURE:Logic
 * @docs OPERATIONS_MANUAL:Commands
 *
 * ### AI Assist Note
 * **Directive Dispatcher**: Handles validated outbound directive transmission,
 * cancellable Neural Link acknowledgments, predictive Browser Sentinel skill filtering,
 * and `@agent` / `#cluster` / swarm broadcast routing.
 *
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Agent/cluster resolution failure, API timeout, or stale acknowledgment timer firing after rapid error.
 * - **Telemetry Link**: Search `[directive_dispatcher]` or `[CommandProcessor]` in `event_bus` logs.
 */

import { event_bus } from '../../services/event_bus';
import { agent_api_service } from '../../services/agent_api_service';
import { resolve_agent_model_config } from '../../utils/model_utils';
import { use_workspace_store } from '../../stores/workspace_store';
import { use_sovereign_store } from '../../stores/sovereign_store';
import { get_settings } from '../../stores/settings_store';
import { use_skill_store } from '../../stores/skill_store';
import { browser_inference_service } from '../../services/browser_inference';
import type { Agent, Execution_Mode } from '../../types';
import {
    prepare_validated_directive,
    resolve_lead_coordinator,
    type Agent_Lookup,
    type ValidatedDirective,
} from '../command_text';

/** Return value from command handlers indicating if the terminal log should be cleared. */
export interface Command_Result {
    /** If true, the Terminal should wipe its local log state. */
    should_clear_logs: boolean;
}

/**
 * Consolidates directive dispatching logic.
 * Enforces validated/sanitized directive payloads prior to API transmission.
 */
export async function dispatch_directive(
    agent: Agent,
    message: ValidatedDirective,
    is_safe_mode: boolean,
    scope: 'agent' | 'cluster' = 'agent',
    cluster_id?: string,
    department?: string,
    display_name?: string,
    execution_mode?: Execution_Mode,
    recent_findings?: string
): Promise<void> {
    const settings = get_settings();
    const { model_id, provider } = resolve_agent_model_config(agent, settings.default_model);
    const target_display = display_name || agent.name;

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
            enabled_skills,
            '0',
            true,
            undefined,
            undefined,
            execution_mode,
            recent_findings
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
 * Handles targeting an individual agent via `@mention`.
 */
export async function handle_agent_mention(
    cmd: string,
    args: string[],
    find_agent: Agent_Lookup,
    is_safe_mode?: boolean,
    execution_mode?: Execution_Mode,
    recent_findings?: string
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
        const message = prepare_validated_directive(args.join(' '), `Usage: @${agent.name} <message>`);
        if (!message) return { should_clear_logs: false };

        await dispatch_directive(agent, message, !!is_safe_mode, 'agent', undefined, undefined, `@${agent.name}`, execution_mode, recent_findings);
    }
    return { should_clear_logs: false };
}

/**
 * Handles targeting a specific mission cluster via `#name`.
 */
export async function handle_cluster_mention(
    cmd: string,
    args: string[],
    safe_agents: Agent[],
    is_safe_mode?: boolean,
    execution_mode?: Execution_Mode,
    recent_findings?: string
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

    const message = prepare_validated_directive(args.join(' '), `Usage: #${cluster.name} <message>`);
    if (!message) return { should_clear_logs: false };

    await dispatch_directive(alpha_agent, message, !!is_safe_mode, 'cluster', cluster.id, cluster.department, `#${cluster.name}`, execution_mode, recent_findings);
    return { should_clear_logs: false };
}

/**
 * Handles broadcasting to the entire swarm via the lead coordinator node.
 */
export async function handle_swarm_broadcast(
    parts: string[],
    safe_agents: Agent[],
    is_safe_mode?: boolean,
    execution_mode?: Execution_Mode,
    recent_findings?: string
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
    const message = prepare_validated_directive(
        raw_message,
        'Swarm broadcast must contain a message of 500 characters or fewer.'
    );
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

    const lead_agent = resolve_lead_coordinator(safe_agents, false);
    if (lead_agent) {
        await dispatch_directive(lead_agent, message, !!is_safe_mode, 'agent', undefined, undefined, 'Swarm Broadcast', execution_mode, recent_findings);
    }

    return { should_clear_logs: false };
}

// Metadata: [directive_dispatcher]

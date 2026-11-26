/**
 * @docs ARCHITECTURE:Logic
 * @docs OPERATIONS_MANUAL:Commands
 *
 * ### AI Assist Note
 * **Slash Command Registry**: Modular slash-command handlers (`/help`, `/clear`, `/status`,
 * `/deploy`, `/pre-pr`, `/config`, `/pause`, `/resume`, `/send`, `/swarm`, `/switch`) plus
 * an encapsulated `DeployConfirmationGate` timer state machine.
 *
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Unrecognized slash command, expired `/deploy` confirmation window, or backend RPC fault.
 * - **Telemetry Link**: Search `[slash_handlers]` in `event_bus` logs.
 */

import { event_bus } from '../../services/event_bus';
import { agent_api_service } from '../../services/agent_api_service';
import { system_api_service } from '../../services/system_api_service';
import { use_workspace_store } from '../../stores/workspace_store';
import type { Agent, Execution_Mode } from '../../types';
import { prepare_validated_directive, type Agent_Lookup } from '../command_text';
import { dispatch_directive, type Command_Result } from './directive_dispatcher';

/**
 * Encapsulated two-step confirmation gate for `/deploy` with automatic TTL expiration.
 */
export class DeployConfirmationGate {
    private pending = false;
    private timer: ReturnType<typeof setTimeout> | null = null;
    private readonly ttl_ms: number;

    constructor(ttl_ms = 30_000) {
        this.ttl_ms = ttl_ms;
    }

    public is_pending(): boolean {
        return this.pending;
    }

    public request(): void {
        this.pending = true;
        if (this.timer) {
            clearTimeout(this.timer);
        }
        this.timer = setTimeout(() => {
            this.pending = false;
            this.timer = null;
        }, this.ttl_ms);
    }

    public consume(): boolean {
        if (!this.pending) {
            return false;
        }
        this.reset();
        return true;
    }

    public reset(): void {
        this.pending = false;
        if (this.timer) {
            clearTimeout(this.timer);
            this.timer = null;
        }
    }
}

export const deploy_confirmation_gate = new DeployConfirmationGate();

export function reset_deploy_confirmation_for_testing(): void {
    deploy_confirmation_gate.reset();
}

export interface Slash_Command_Context {
    args: string[];
    safe_agents: Agent[];
    find_agent: Agent_Lookup;
    is_safe_mode?: boolean;
    execution_mode?: Execution_Mode;
    recent_findings?: string;
}

type Slash_Handler = (ctx: Slash_Command_Context) => Command_Result | Promise<Command_Result>;

function handle_help_command(): Command_Result {
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

function handle_clear_command(): Command_Result {
    event_bus.clear_history();
    return { should_clear_logs: true };
}

function handle_status_command(safe_agents: Agent[]): Command_Result {
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

async function handle_deploy_command(args: string[]): Promise<Command_Result> {
    const is_confirm = args[0]?.toLowerCase() === 'confirm';

    if (!is_confirm) {
        deploy_confirmation_gate.request();
        event_bus.emit_log({
            source: 'System',
            text: '⚠️ Production deployment requested. Type "/deploy confirm" within 30 seconds to execute.',
            severity: 'warning'
        });
        return { should_clear_logs: false };
    }

    if (!deploy_confirmation_gate.consume()) {
        event_bus.emit_log({
            source: 'System',
            text: '❌ No pending deployment request. Type "/deploy" first to initiate the confirmation challenge.',
            severity: 'error'
        });
        return { should_clear_logs: false };
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

async function handle_pre_pr_command(): Promise<Command_Result> {
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

function handle_config_command(args: string[], find_agent: Agent_Lookup): Command_Result {
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

async function handle_agent_lifecycle_command(
    action: 'pause' | 'resume',
    args: string[],
    find_agent: Agent_Lookup
): Promise<Command_Result> {
    const agent = find_agent(args[0]);
    if (!agent) return { should_clear_logs: false };

    const is_pause = action === 'pause';
    const icon = is_pause ? '⏸️' : '▶️';
    const past_tense = is_pause ? 'paused' : 'resumed';
    const severity = is_pause ? 'warning' : 'success';

    try {
        const success = is_pause
            ? await agent_api_service.pause_agent(agent.id)
            : await agent_api_service.resume_agent(agent.id);

        event_bus.emit_log({
            source: 'System',
            text: success
                ? `${icon} Agent ${agent.name} ${past_tense} via TadpoleOS.`
                : `${icon} Agent ${agent.name} ${past_tense} locally (TadpoleOS offline).`,
            severity
        });
    } catch (err: unknown) {
        const msg = err instanceof Error ? err.message : String(err);
        event_bus.emit_log({
            source: 'System',
            text: `Failed to ${action} agent ${agent.name}: ${msg}`,
            severity: 'error'
        });
    }
    return { should_clear_logs: false };
}

async function handle_send_command(ctx: Slash_Command_Context): Promise<Command_Result> {
    if (!ctx.args[0]) {
        event_bus.emit_log({
            source: 'System',
            text: 'Usage: /send <agent-name> <message>',
            severity: 'error'
        });
        return { should_clear_logs: false };
    }

    const agent = ctx.find_agent(ctx.args[0]);
    if (!agent) return { should_clear_logs: false };

    const message = prepare_validated_directive(ctx.args.slice(1).join(' '), 'Usage: /send <agent-name> <message>');
    if (!message) return { should_clear_logs: false };

    await dispatch_directive(agent, message, !!ctx.is_safe_mode, 'agent', undefined, undefined, agent.name, ctx.execution_mode, ctx.recent_findings);
    return { should_clear_logs: false };
}

function handle_swarm_command(args: string[], safe_agents: Agent[]): Command_Result {
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

async function handle_switch_command(args: string[], find_agent: Agent_Lookup): Promise<Command_Result> {
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

const SLASH_COMMAND_REGISTRY: Record<string, Slash_Handler> = {
    '/help': () => handle_help_command(),
    '/clear': () => handle_clear_command(),
    '/status': (ctx) => handle_status_command(ctx.safe_agents),
    '/deploy': (ctx) => handle_deploy_command(ctx.args),
    '/pre-pr': () => handle_pre_pr_command(),
    '/config': (ctx) => handle_config_command(ctx.args, ctx.find_agent),
    '/pause': (ctx) => handle_agent_lifecycle_command('pause', ctx.args, ctx.find_agent),
    '/resume': (ctx) => handle_agent_lifecycle_command('resume', ctx.args, ctx.find_agent),
    '/send': (ctx) => handle_send_command(ctx),
    '/swarm': (ctx) => handle_swarm_command(ctx.args, ctx.safe_agents),
    '/switch': (ctx) => handle_switch_command(ctx.args, ctx.find_agent),
};

/**
 * Dispatches standard slash commands via the declarative handler registry.
 */
export async function handle_slash_command(
    cmd: string,
    args: string[],
    safe_agents: Agent[],
    find_agent: Agent_Lookup,
    is_safe_mode?: boolean,
    execution_mode?: Execution_Mode,
    recent_findings?: string
): Promise<Command_Result> {
    const handler = SLASH_COMMAND_REGISTRY[cmd];
    if (!handler) {
        event_bus.emit_log({
            source: 'System',
            text: `Unknown command: ${cmd}. Type /help for available commands.`,
            severity: 'error'
        });
        return { should_clear_logs: false };
    }

    return handler({
        args,
        safe_agents,
        find_agent,
        is_safe_mode,
        execution_mode,
        recent_findings,
    });
}

// Metadata: [slash_handlers]

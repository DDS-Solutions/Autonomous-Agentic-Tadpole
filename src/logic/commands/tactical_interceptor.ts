/**
 * @docs ARCHITECTURE:Logic
 * @docs OPERATIONS_MANUAL:Commands
 *
 * ### AI Assist Note
 * **Tactical Sentinel Interceptor**: Routes tactical UI/DOM queries to the in-browser
 * Browser Specialist with OpenTelemetry-style trace spans and automatic escalation to
 * the lead Computer Architect when anomaly thresholds are exceeded.
 *
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Browser Specialist inference error, trace span leak, or missing Architect escalation target.
 * - **Telemetry Link**: Search `[tactical_interceptor]` or `[TieredRouting]` in console logs.
 */

import { v4 as uuidv4 } from 'uuid';
import { event_bus } from '../../services/event_bus';
import { use_browser_specialist_store } from '../../stores/browser_specialist_store';
import { use_trace_store } from '../../stores/trace_store';
import type { Agent } from '../../types';
import {
    MAX_DIRECTIVE_LENGTH,
    prepare_validated_directive,
    resolve_lead_coordinator,
} from '../command_text';
import { dispatch_directive, type Command_Result } from './directive_dispatcher';

export async function handle_tactical_interception(
    command_text: string,
    safe_agents: Agent[],
    is_safe_mode?: boolean
): Promise<Command_Result> {
    console.debug('[CommandProcessor] [TieredRouting] Tactical intent detected. Routing to Browser Specialist.');
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

            const architect = resolve_lead_coordinator(safe_agents, true);
            if (architect) {
                const escalated_raw = `[ESCALATED_TACTICAL]: ${command_text}`;
                const escalated_msg = escalated_raw.length > MAX_DIRECTIVE_LENGTH
                    ? escalated_raw.substring(0, MAX_DIRECTIVE_LENGTH - 3) + '...'
                    : escalated_raw;
                const validated_message = prepare_validated_directive(
                    escalated_msg,
                    'Tactical escalation could not be prepared.'
                );

                if (validated_message) {
                    await dispatch_directive(
                        architect,
                        validated_message,
                        !!is_safe_mode,
                        'agent',
                        undefined,
                        undefined,
                        `@${architect.name}`
                    );
                }
            }
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

// Metadata: [tactical_interceptor]

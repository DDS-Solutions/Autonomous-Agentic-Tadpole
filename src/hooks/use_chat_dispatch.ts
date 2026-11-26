/**
 * @docs ARCHITECTURE:Logic
 *
 * ### AI Assist Note
 * **Chat Dispatch Hook**: Encapsulates message submission, freshest-state sync from Zustand,
 * 3-mode execution (`chat` / `plan` / `swarm`), brainstorm-to-swarm handoff (`build_brainstorm_context`),
 * `/pre-pr` quality gate trigger, and fault handling.
 *
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Stale target_node during dispatch or `process_command` fault.
 * - **Telemetry Link**: Search `[useChatDispatch]` in browser console.
 */

import type React from 'react';
import { useState, useCallback, useMemo } from 'react';
import { use_settings_store, resolve_execution_mode, type Execution_Mode } from '../stores/settings_store';
import { use_sovereign_store, type Sovereign_Scope, type Chat_Message } from '../stores/sovereign_store';
import { process_command } from '../logic/command_processor';
import { system_api_service } from '../services/system_api_service';
import { build_brainstorm_context } from '../utils/brainstorm_handoff';
import type { Agent } from '../types';
import { i18n } from '../i18n';

const TELEMETRY_SOURCE = '[useChatDispatch]';

export interface ChatDispatchState {
    input_text: string;
    set_input_text: React.Dispatch<React.SetStateAction<string>>;
    handle_send: () => Promise<void>;
    toggle_safety: () => void;
    is_safe_mode: boolean;
    execution_mode: Execution_Mode;
    set_execution_mode: (mode: Execution_Mode) => void;
    can_launch_swarm: boolean;
    launch_swarm_handoff: () => Promise<void>;
}

export function useChatDispatch(
    active_scope: Sovereign_Scope,
    target_node: string,
    agents: Agent[],
    selected_agent_id: string | null,
    add_message: (msg: Omit<Chat_Message, 'id' | 'timestamp'>) => void
): ChatDispatchState {
    const [input_text, set_input_text] = useState('');
    const { settings, update_setting } = use_settings_store();
    const messages = use_sovereign_store((s) => s.messages);
    const execution_mode = resolve_execution_mode(settings);
    const is_safe_mode = execution_mode !== 'swarm';

    const get_recent_brainstorm = useCallback((): string | undefined => {
        const current_messages = use_sovereign_store.getState().messages || [];
        return build_brainstorm_context(
            current_messages.map((m) => ({
                speaker: m.sender_name || (m.sender_id === '0' ? 'User' : 'Agent'),
                text: m.text,
                is_system: m.sender_id === 'system',
            }))
        );
    }, []);

    const can_launch_swarm = useMemo(() => {
        if (execution_mode === 'swarm') return false;
        return (messages || []).some((m) => m.sender_id !== 'system' && Boolean(m.text?.trim()));
    }, [execution_mode, messages]);

    const dispatch_text = useCallback(async (text: string, mode_override?: Execution_Mode) => {
        if (!text || !text.trim()) return;

        const brainstorm_context = get_recent_brainstorm();
        const user_msg = {
            sender_id: '0',
            sender_name: i18n.t('chat.overlord_name'),
            text: text,
            scope: active_scope,
            target_node: active_scope !== 'swarm' ? target_node : undefined
        };

        add_message(user_msg);

        // Preserved target-exclusive /pre-pr shortcut with direct chat status bubbles
        if (text.trim().toLowerCase() === '/pre-pr') {
            add_message({
                sender_id: 'system',
                sender_name: i18n.t('chat.system_name'),
                text: '🔍 Starting Pre-PR Quality Gate verification...',
                scope: active_scope,
            });
            try {
                const data = await system_api_service.pre_pr_engine();
                if (data.status === 'success') {
                    add_message({
                        sender_id: 'system',
                        sender_name: i18n.t('chat.system_name'),
                        text: `✅ Pre-PR Gate succeeded!\n\n${data.output || ''}`,
                        scope: active_scope,
                    });
                } else {
                    add_message({
                        sender_id: 'system',
                        sender_name: i18n.t('chat.system_name'),
                        text: `❌ Pre-PR Gate failed!\n\nOutput:\n${data.output || ''}\n\nError details:\n${data.error || ''}`,
                        scope: active_scope,
                    });
                }
            } catch (err: unknown) {
                const message = err instanceof Error ? err.message : 'Unknown command fault';
                add_message({
                    sender_id: 'system',
                    sender_name: i18n.t('chat.system_name'),
                    text: `❌ Pre-PR Gate fault: ${message}`,
                    scope: active_scope,
                });
            }
            return;
        }

        try {
            console.debug(`${TELEMETRY_SOURCE} Intent captured: ${text.substring(0, 50)}...`);

            // OVERLORD SYNC: Pull freshest state directly from stores to prevent ghost-targeting stale nodes
            const sovereign_state = use_sovereign_store.getState();
            const current_scope = sovereign_state.active_scope;
            const current_target_agent = sovereign_state.target_agent;
            const current_target_cluster = sovereign_state.target_cluster;

            let fresh_target = current_scope === 'cluster' ? current_target_cluster : current_target_agent;
            if (current_scope === 'agent' && selected_agent_id) {
                const agent = agents.find(a => a.id === selected_agent_id);
                if (agent) fresh_target = agent.name;
            }

            const current_settings = use_settings_store.getState().settings;
            const effective_mode = mode_override ?? resolve_execution_mode(current_settings);
            const current_safe_mode = effective_mode !== 'swarm';
            console.debug(`${TELEMETRY_SOURCE} [DISPATCH] Target: ${fresh_target}, Mode: ${effective_mode}, Safe_Mode: ${current_safe_mode}`);

            const result = await process_command(
                text,
                agents,
                current_safe_mode,
                current_scope,
                fresh_target,
                effective_mode,
                brainstorm_context
            );
            if (result?.should_clear_logs) {
                use_sovereign_store.getState().clear_history?.();
            }
        } catch (err: unknown) {
            const message = err instanceof Error ? err.message : 'Unknown command fault';
            add_message({
                sender_id: 'system',
                sender_name: i18n.t('chat.system_name'),
                text: i18n.t('chat.fault_detected', { message }),
                scope: active_scope,
            });
        }
    }, [active_scope, target_node, agents, add_message, selected_agent_id, get_recent_brainstorm]);

    const handle_send = useCallback(async () => {
        const text = input_text;
        if (!text || !text.trim()) return;
        set_input_text('');
        await dispatch_text(text);
    }, [input_text, dispatch_text]);

    const set_execution_mode = useCallback((mode: Execution_Mode) => {
        update_setting('execution_mode', mode);
    }, [update_setting]);

    const toggle_safety = useCallback(() => {
        const next_mode: Execution_Mode = execution_mode === 'swarm' ? 'chat' : 'swarm';
        update_setting('execution_mode', next_mode);
    }, [execution_mode, update_setting]);

    const launch_swarm_handoff = useCallback(async () => {
        update_setting('execution_mode', 'swarm');
        const directive = input_text.trim()
            || 'Execute the agreed plan from our prior brainstorm session and delegate tasks to the swarm.';
        if (input_text.trim()) {
            set_input_text('');
        }
        await dispatch_text(directive, 'swarm');
    }, [update_setting, input_text, dispatch_text]);

    return {
        input_text,
        set_input_text,
        handle_send,
        toggle_safety,
        is_safe_mode,
        execution_mode,
        set_execution_mode,
        can_launch_swarm,
        launch_swarm_handoff,
    };
}

// Metadata: [use_chat_dispatch]

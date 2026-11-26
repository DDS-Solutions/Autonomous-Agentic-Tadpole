/**
 * @docs ARCHITECTURE:Interface
 *
 * ### AI Assist Note
 * **UI Component**: Detachable `440×600` Command & Control (C2) interface for Swarm Intelligence.
 * Orchestrates triple-scope communication (Agent/Cluster/Swarm), 3-mode execution (Chat/Plan/Swarm)
 * with Brainstorm -> Swarm handoff, voice dictation with MediaRecorder fallback, Multiversal
 * timeline branches, and the Live Artifact Workspace.
 *
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Voice client initialization stall, popup blocker on detach, or message packet
 *   starvation during high-frequency telemetry storms.
 * - **Telemetry Link**: Search `[SovereignChat]` or `sovereign_store` in browser logs.
 */

import React, { useState, useEffect, useMemo, useCallback, useRef } from 'react';
import { motion, AnimatePresence, useDragControls } from 'framer-motion';
import { Zap } from 'lucide-react';
import clsx from 'clsx';
import { useShallow } from 'zustand/react/shallow';
import { use_sovereign_store } from '../stores/sovereign_store';
import { use_agent_store } from '../stores/agent_store';
import { use_workspace_store } from '../stores/workspace_store';
import { use_notification_store } from '../stores/notification_store';
import { useChatWindow } from '../hooks/use_chat_window';
import { useChatVoice } from '../hooks/use_chat_voice';
import { useChatDispatch } from '../hooks/use_chat_dispatch';
import { Chat_Content } from './chat/Chat_Content';
import { Chat_Detached_Portal } from './chat/Chat_Detached_Portal';
import { resolve_alpha } from '../utils/agent_hierarchy';
import { i18n } from '../i18n';

const MAX_RENDERED_MESSAGES = 300;

interface SovereignChatProps {
    isDetachedView?: boolean;
}

export const SovereignChat: React.FC<SovereignChatProps> = ({ isDetachedView }) => {
    // ── Store Subscriptions (Optimized via useShallow) ──────────────────────────────
    const {
        messages,
        active_scope,
        selected_agent_id,
        target_agent,
        target_cluster,
        is_detached,
        set_detached,
        set_scope,
        add_message,
        clear_history,
        set_selected_agent_id,
        set_target_agent,
        set_target_cluster,
        active_node_id,
        active_mission_id,
        session_leaves,
        fetch_session_history_action,
        fetch_mission_leaves_action,
        revert_to_node_action,
    } = use_sovereign_store(
        useShallow(s => ({
            messages: s.messages,
            active_scope: s.active_scope,
            selected_agent_id: s.selected_agent_id,
            target_agent: s.target_agent,
            target_cluster: s.target_cluster,
            is_detached: s.is_detached,
            set_detached: s.set_detached,
            set_scope: s.set_scope,
            add_message: s.add_message,
            clear_history: s.clear_history,
            set_selected_agent_id: s.set_selected_agent_id,
            set_target_agent: s.set_target_agent,
            set_target_cluster: s.set_target_cluster,
            active_node_id: s.active_node_id,
            active_mission_id: s.active_mission_id,
            session_leaves: s.session_leaves,
            fetch_session_history_action: s.fetch_session_history,
            fetch_mission_leaves_action: s.fetch_mission_leaves,
            revert_to_node_action: s.revert_to_node,
        }))
    );

    const fetch_session_history = useCallback((mission: string, leaf: string) =>
        fetch_session_history_action(mission, leaf).catch(() => { /* notification already shown */ }), [fetch_session_history_action]);
    const fetch_mission_leaves = useCallback((mission: string) =>
        fetch_mission_leaves_action(mission).catch(() => { /* notification already shown */ }), [fetch_mission_leaves_action]);
    const revert_to_node = useCallback((mission: string, node: string) =>
        revert_to_node_action(mission, node).catch(() => { /* notification already shown */ }), [revert_to_node_action]);

    const target_node = active_scope === 'cluster' ? target_cluster : target_agent;

    const agents = use_agent_store(s => s.agents);
    const is_agents_loading = use_agent_store(s => s.is_loading);
    const fetch_agents = use_agent_store(s => s.fetch_agents);
    const clusters = use_workspace_store(s => s.clusters);

    // ── Local UI State ───────────────────────────────────
    const [popup_blocked, set_popup_blocked] = useState(false);
    const [open_dropdown, set_open_dropdown] = useState<'agent' | 'cluster' | null>(null);
    const [show_transcript, set_show_transcript] = useState(false);
    const [show_branches, set_show_branches] = useState(false);
    const [is_workspace_open, set_is_workspace_open] = useState(false);
    const drag_controls = useDragControls();

    // ── Hooks ────────────────────────────────────────────
    const {
        is_minimized,
        constraints_ref,
        x_open,
        y_open,
        x_min,
        y_min,
        clamp_open_bounds,
        clamp_min_bounds,
        toggle_detach,
        perform_minimize_transform,
        perform_maximize_transform
    } = useChatWindow();

    const {
        input_text,
        set_input_text,
        handle_send,
        toggle_safety,
        is_safe_mode,
        execution_mode,
        set_execution_mode,
        can_launch_swarm,
        launch_swarm_handoff,
    } = useChatDispatch(active_scope, target_node, agents, selected_agent_id, add_message);

    const handle_speech_input = useCallback((spoken: string) => {
        set_input_text((prev) => {
            const trimmed_prev = prev.trim();
            return trimmed_prev ? `${trimmed_prev} ${spoken}` : spoken;
        });
    }, [set_input_text]);

    const {
        voice_status,
        voice_error,
        clear_voice_error,
        is_speech_enabled,
        is_speaking,
        toggle_voice,
        toggle_speech,
    } = useChatVoice(messages, selected_agent_id, agents, handle_speech_input);

    // ── Agent Sorting ────────────────────────────────────
    const sorted_agents = useMemo(() => {
        const get_score = (status: string) => {
            if (['active', 'thinking', 'coding'].includes(status)) return 0;
            if (status === 'idle') return 1;
            return 2;
        };
        return [...agents].sort((a, b) => {
            const score_a = get_score(a.status || 'offline');
            const score_b = get_score(b.status || 'offline');
            if (score_a !== score_b) return score_a - score_b;
            return a.name.localeCompare(b.name);
        });
    }, [agents]);

    // ── Auto-Selection Effects ───────────────────────────
    useEffect(() => {
        if (agents.length === 0) return;

        if (selected_agent_id) {
            const agent = agents.find(a => a.id === selected_agent_id);
            if (agent && agent.name !== target_agent) {
                set_target_agent(agent.name);
            }
            return;
        }

        const matched = agents.find(a =>
            (target_agent && (a.name.toLowerCase() === target_agent.toLowerCase() || a.id === target_agent)) ||
            (target_agent && (a.name.toLowerCase().includes(target_agent.toLowerCase()) || target_agent.toLowerCase().includes(a.name.toLowerCase())))
        ) ?? resolve_alpha(agents);

        if (matched) {
            set_target_agent(matched.name);
            set_selected_agent_id(matched.id);
        }
    }, [agents, selected_agent_id, target_agent, set_target_agent, set_selected_agent_id]);

    useEffect(() => {
        if (clusters.length > 0 && !target_cluster) {
            set_target_cluster(clusters[0].name);
        }
    }, [clusters, target_cluster, set_target_cluster]);

    const has_init_fetched = useRef(false);
    useEffect(() => {
        if (!has_init_fetched.current) {
            has_init_fetched.current = true;
            if (agents.length === 0 && !is_agents_loading) {
                void fetch_agents();
            }
        }
    }, [agents.length, fetch_agents, is_agents_loading]);

    // ── Message Filtering ────────────────────────────────
    const filtered_messages = useMemo(() => messages.filter(m => {
        if (active_scope === 'swarm') return true;

        if (active_scope === 'agent') {
            const target = (target_agent ?? '').toLowerCase();
            const sender_lower = (m.sender_name ?? '').toLowerCase();
            const target_node_lower = (m.target_node ?? '').toLowerCase();

            if (m.sender_id === '0') {
                const mentioned_target = target && m.text?.trim().toLowerCase().startsWith(`@${target}`);
                return !m.target_node || m.target_node === target_agent || mentioned_target || (selected_agent_id && m.agent_id === selected_agent_id) || m.scope === 'swarm';
            }

            if (m.scope === 'swarm') return true;

            if (m.sender_id === 'system') {
                return !m.agent_id || m.agent_id === selected_agent_id;
            }

            if (selected_agent_id && (m.sender_id === selected_agent_id || m.agent_id === selected_agent_id)) {
                return true;
            }

            if (target && (
                sender_lower.includes(target) ||
                target.includes(sender_lower) ||
                target_node_lower.includes(target) ||
                target.includes(target_node_lower)
            )) {
                return true;
            }

            const is_target_alpha = target.includes('nine') || target.includes('ceo') || target === '1';
            const is_msg_alpha = sender_lower.includes('nine') || sender_lower.includes('ceo') || m.sender_id === '1' || m.agent_id === '1';
            if (is_target_alpha && is_msg_alpha) {
                return true;
            }

            return false;
        }

        if (active_scope === 'cluster') {
            const cluster_lower = (target_cluster ?? '').toLowerCase();
            if (m.sender_id === '0') {
                const mentioned_cluster = cluster_lower && m.text?.trim().toLowerCase().startsWith(`#${cluster_lower}`);
                return !m.target_node || m.target_node === target_cluster || mentioned_cluster || m.scope === 'swarm';
            }
            return m.target_node === target_node || m.scope === 'swarm' || (m.scope === 'cluster' && !m.target_node);
        }

        return true;
    }), [messages, active_scope, selected_agent_id, target_agent, target_cluster, target_node]);

    // ── Header Interaction ───────────────────────────────
    const handle_header_click = useCallback(() => {
        if (is_minimized) perform_maximize_transform();
        else perform_minimize_transform();
    }, [is_minimized, perform_maximize_transform, perform_minimize_transform]);

    const handle_revert = useCallback(async (node_id: string) => {
        if (active_mission_id) {
            await revert_to_node(active_mission_id, node_id);
        }
    }, [active_mission_id, revert_to_node]);

    // ── Shared Content Props ─────────────────────────────
    const content_props = useMemo(() => ({
        active_scope,
        target_node,
        target_agent,
        target_cluster,
        selected_agent_id,
        is_speaking,
        voice_status,
        voice_error,
        on_clear_voice_error: clear_voice_error,
        show_transcript,
        set_show_transcript,
        messages: filtered_messages,
        max_rendered_messages: MAX_RENDERED_MESSAGES,
        input_text,
        set_input_text,
        on_send: handle_send,
        on_toggle_voice: toggle_voice,
        on_toggle_speech: toggle_speech,
        is_speech_enabled,
        on_toggle_safety: toggle_safety,
        is_safe_mode,
        execution_mode,
        on_change_execution_mode: set_execution_mode,
        can_launch_swarm,
        on_launch_swarm: () => { void launch_swarm_handoff(); },
        on_toggle_detach: toggle_detach,
        on_clear_history: clear_history,
        on_set_scope: set_scope,
        open_dropdown,
        set_open_dropdown,
        sorted_agents,
        set_target_agent,
        set_selected_agent_id,
        set_target_cluster,
        clusters,
        on_minimize: perform_minimize_transform,

        // Preserved Multiversal & Live Artifact Workspace props
        activeNodeId: active_node_id,
        activeMissionId: active_mission_id,
        sessionLeaves: session_leaves,
        onFetchHistory: fetch_session_history,
        onFetchLeaves: fetch_mission_leaves,
        showBranches: show_branches,
        setShowBranches: set_show_branches,
        isWorkspaceOpen: is_workspace_open,
        setWorkspaceOpen: set_is_workspace_open,
        onRevert: handle_revert,
    }), [
        active_scope,
        target_node,
        target_agent,
        target_cluster,
        selected_agent_id,
        is_speaking,
        voice_status,
        voice_error,
        clear_voice_error,
        show_transcript,
        set_show_transcript,
        filtered_messages,
        input_text,
        set_input_text,
        handle_send,
        toggle_voice,
        toggle_speech,
        is_speech_enabled,
        toggle_safety,
        is_safe_mode,
        execution_mode,
        set_execution_mode,
        can_launch_swarm,
        launch_swarm_handoff,
        toggle_detach,
        clear_history,
        set_scope,
        open_dropdown,
        set_open_dropdown,
        sorted_agents,
        set_target_agent,
        set_selected_agent_id,
        set_target_cluster,
        clusters,
        perform_minimize_transform,
        active_node_id,
        active_mission_id,
        session_leaves,
        fetch_session_history,
        fetch_mission_leaves,
        show_branches,
        is_workspace_open,
        handle_revert,
    ]);

    // ── Standalone Detached Window View (/detached?type=chat) ──
    if (isDetachedView) {
        return (
            <div className="w-screen h-screen bg-[color:var(--color-background)] text-white overflow-hidden flex flex-col">
                <Chat_Content
                    {...content_props}
                    is_detached={true}
                    container_props={{}}
                />
            </div>
        );
    }

    // ── Detached Portal Render Path ──────────────────────
    if (is_detached && !isDetachedView) {
        return (
            <Chat_Detached_Portal
                active_scope={active_scope}
                popup_blocked={popup_blocked}
                on_restore={() => set_detached(false)}
                on_popup_block={() => {
                    set_popup_blocked(true);
                    use_notification_store.getState().add_notification?.({
                        title: 'Popup Blocked',
                        message: i18n.t('chat.popup_blocked_warning', { defaultValue: 'Browser blocked the detached chat window. Allow popups for this site.' }),
                        severity: 'warning',
                        persistent: false,
                    });
                }}
                content_props={content_props}
            />
        );
    }

    // ── Inline Render Path ───────────────────────────────
    return (
        <>
            {!is_detached && (
                <div ref={constraints_ref} className="fixed inset-6 z-[100] pointer-events-none" />
            )}
            <AnimatePresence>
                {!is_minimized && (
                    <motion.div
                        key="open-chat"
                        style={{ x: x_open, y: y_open }}
                        initial={{ opacity: 0, scale: 0.9, filter: 'blur(10px)' }}
                        animate={{ opacity: 1, scale: 1, filter: 'blur(0px)' }}
                        exit={{ opacity: 0, scale: 0.9, filter: 'blur(10px)' }}
                        drag={!is_detached}
                        dragControls={drag_controls}
                        dragListener={false}
                        dragMomentum={false}
                        dragElastic={0}
                        dragConstraints={is_detached ? undefined : constraints_ref}
                        onDragEnd={clamp_open_bounds}
                        className={clsx(
                            "fixed z-50 flex flex-col overflow-hidden transition-[filter,opacity] duration-300 pointer-events-auto",
                            "bottom-6 right-6 w-[440px] h-[600px] max-h-[calc(100vh-3rem)] rounded-2xl border border-[color:var(--color-border)]/50 shadow-[0_30px_60px_-15px_rgba(0,0,0,0.7)] bg-[color:var(--color-surface)]/40 backdrop-blur-xl pointer-events-auto"
                        )}
                    >
                        <Chat_Content
                            {...content_props}
                            is_detached={false}
                            drag_controls={drag_controls}
                            on_header_click={handle_header_click}
                        />
                    </motion.div>
                )}
            </AnimatePresence>

            <AnimatePresence>
                {is_minimized && (
                    <motion.button
                        style={{ x: x_min, y: y_min }}
                        initial={{ scale: 0.8, opacity: 0 }}
                        animate={{ scale: 1, opacity: 1 }}
                        exit={{ scale: 0.8, opacity: 0 }}
                        drag
                        dragConstraints={constraints_ref}
                        dragMomentum={false}
                        dragElastic={0}
                        onDragEnd={clamp_min_bounds}
                        whileDrag={{ scale: 1.05 }}
                        onClick={() => {
                            perform_maximize_transform();
                        }}
                        className="fixed bottom-6 right-6 z-50 bg-zinc-100 text-zinc-950 px-5 py-3 rounded-2xl shadow-[0_10px_40px_-10px_rgba(255,255,255,0.3)] flex items-center gap-3 group border border-white cursor-grab active:cursor-grabbing"
                    >
                        <Zap size={20} className="group-hover:animate-pulse pointer-events-none" />
                        <span className="text-xs font-bold uppercase tracking-widest pointer-events-none">{i18n.t('chat.title')}</span>
                    </motion.button>
                )}
            </AnimatePresence>
        </>
    );
};

// Metadata: [SovereignChat]

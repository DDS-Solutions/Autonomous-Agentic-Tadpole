/**
 * @docs ARCHITECTURE:Interface
 * 
 * ### AI Assist Note
 * **UI Component**: Detachable Command & Control (C2) interface for Swarm Intelligence. 
 * Orchestrates triple-scope communication (Agent/Cluster/Swarm), real-time voice synthesis (Azure/Groq), and transcript buffering for autonomous agent logs.
 * 
 * ### 🏗️ Architecture Note
 * This component was decomposed from a 1054-line monolith into focused sub-modules:
 * - Hooks: `use_chat_voice`, `use_chat_dispatch`, `use_chat_window`
 * - UI: `Chat_Content`, `Chat_Header`, `Chat_Scope_Selector`, `Chat_Lineage_Breadcrumb`
 * - Portal: `Chat_Detached_Portal`
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Voice client initialization stall, portal window context loss (detachment), or message packet starvation during high-frequency telemetry storms.
 * - **Telemetry Link**: Search for `[SovereignChat]` or `sovereign_store` in browser logs.
 */

import React, { useState, useEffect, useMemo, useCallback, useRef } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { Zap } from 'lucide-react';
import clsx from 'clsx';
import { use_sovereign_store } from '../stores/sovereign_store';
import { use_agent_store } from '../stores/agent_store';
import { use_workspace_store } from '../stores/workspace_store';
import { useDragControls } from 'framer-motion';
import { useChatWindow } from '../hooks/use_chat_window';
import { useChatVoice } from '../hooks/use_chat_voice';
import { useChatDispatch } from '../hooks/use_chat_dispatch';
import { Chat_Content } from './chat/Chat_Content';
import { Chat_Detached_Portal } from './chat/Chat_Detached_Portal';
import { i18n } from '../i18n';

interface SovereignChatProps {
    isDetachedView?: boolean;
}

export const SovereignChat: React.FC<SovereignChatProps> = ({ isDetachedView }) => {
    const MAX_RENDERED_MESSAGES = 300;

    // ── Store Subscriptions ──────────────────────────────
    const messages = use_sovereign_store(s => s.messages);
    const active_scope = use_sovereign_store(s => s.active_scope);
    const selected_agent_id = use_sovereign_store(s => s.selected_agent_id);
    const target_agent = use_sovereign_store(s => s.target_agent);
    const target_cluster = use_sovereign_store(s => s.target_cluster);
    const is_detached = use_sovereign_store(s => s.is_detached);
    const set_detached = use_sovereign_store(s => s.set_detached);
    const set_scope = use_sovereign_store(s => s.set_scope);
    const add_message = use_sovereign_store(s => s.add_message);
    const clear_history = use_sovereign_store(s => s.clear_history);
    const set_selected_agent_id = use_sovereign_store(s => s.set_selected_agent_id);
    const set_target_agent = use_sovereign_store(s => s.set_target_agent);
    const set_target_cluster = use_sovereign_store(s => s.set_target_cluster);

    const active_node_id = use_sovereign_store(s => s.active_node_id);
    const active_mission_id = use_sovereign_store(s => s.active_mission_id);
    const session_leaves = use_sovereign_store(s => s.session_leaves);
    const fetch_session_history = use_sovereign_store(s => s.fetch_session_history);
    const fetch_mission_leaves = use_sovereign_store(s => s.fetch_mission_leaves);
    const revert_to_node = use_sovereign_store(s => s.revert_to_node);

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
        toggle_detach,
        perform_minimize_transform,
        perform_maximize_transform
    } = useChatWindow();

    const {
        voice_status,
        is_speech_enabled,
        is_speaking,
        toggle_voice,
        toggle_speech,
    } = useChatVoice(messages, selected_agent_id, agents);

    const {
        input_text,
        set_input_text,
        handle_send,
        toggle_safety,
        is_safe_mode,
    } = useChatDispatch(active_scope, target_node, agents, selected_agent_id, add_message);

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
        if (selected_agent_id) return;

        const is_ungetTarget = !target_agent || target_agent.toLowerCase() === 'ceo' || target_agent === 'Agent of Nine';
        if (is_ungetTarget) {
            const ceo = agents.find(a => a.role?.toLowerCase().includes('ceo') || a.name.toLowerCase().includes('nine'));
            if (ceo) {
                set_target_agent(ceo.name);
                set_selected_agent_id(ceo.id);
            } else {
                set_target_agent(agents[0].name);
                set_selected_agent_id(agents[0].id);
            }
        }
    }, [agents, selected_agent_id, target_agent, set_target_agent, set_selected_agent_id]);

    // Auto-select first cluster if none selected
    useEffect(() => {
        if (clusters.length > 0 && !target_node) {
            set_target_cluster(clusters[0].name);
        }
    }, [clusters, target_node, set_target_cluster]);

    // Lazy-load agents if store is empty
    const has_init_fetched = useRef(false);
    useEffect(() => {
        if (!has_init_fetched.current) {
            has_init_fetched.current = true;
            if (agents.length === 0 && !is_agents_loading) {
                fetch_agents();
            }
        }
    }, [agents.length, fetch_agents, is_agents_loading]);

    // ── Message Filtering ────────────────────────────────
    const filtered_messages = useMemo(() => messages.filter(m => {
        if (active_scope === 'swarm') return true;

        if (active_scope === 'agent') {
            const target = (target_agent ?? '').toLowerCase();
            return m.scope === 'agent' && (
                m.sender_id === '0' ||
                m.sender_id === selected_agent_id ||
                m.agent_id === selected_agent_id ||
                m.sender_name.toLowerCase().includes(target) ||
                ((target.includes('nine') || target.includes('ceo')) &&
                    (m.sender_name.toLowerCase().includes('nine') || m.sender_name.toLowerCase().includes('ceo') || m.sender_id === '1'))
            );
        }

        if (active_scope === 'cluster') {
            return m.sender_id === '0' || m.target_node === target_node || m.scope === 'swarm';
        }

        return true;
    }), [messages, active_scope, selected_agent_id, target_agent, target_node]);

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
    const content_props = {
        active_scope,
        target_node,
        target_agent,
        target_cluster,
        selected_agent_id,
        is_speaking,
        voice_status,
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

        // Multiversal & Workspace Props
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
    };

    // ── Detached Portal Render Path ──────────────────────
    if (is_detached && !isDetachedView) {
        return (
            <Chat_Detached_Portal
                active_scope={active_scope}
                popup_blocked={popup_blocked}
                on_restore={() => set_detached(false)}
                on_popup_block={() => set_popup_blocked(true)}
                content_props={content_props}
            />
        );
    }

    // ── Inline Render Path ───────────────────────────────
    return (
        <>
            {!is_detached && (
                <div ref={constraints_ref} className="fixed inset-x-0 inset-y-0 z-[100] pointer-events-none" style={{ padding: '24px' }} />
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
                        className={clsx(
                            "fixed z-50 flex flex-col overflow-hidden transition-[filter,opacity] duration-300 pointer-events-auto",
                            "bottom-6 right-6 w-[440px] h-[600px] rounded-2xl border border-[color:var(--color-border)]/50 shadow-[0_30px_60px_-15px_rgba(0,0,0,0.7)] bg-[color:var(--color-surface)]/40 backdrop-blur-xl pointer-events-auto"
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
                        whileDrag={{ scale: 1.05 }}
                        onClick={() => {
                            perform_maximize_transform();
                        }}
                        className="fixed bottom-6 right-6 z-50 bg-zinc-800 text-zinc-100 px-5 py-3 rounded-2xl flex items-center gap-3 group border border-zinc-700 cursor-grab active:cursor-grabbing hover:bg-zinc-700 transition-all"
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

/**
 * @docs ARCHITECTURE:Interface
 *
 * ### AI Assist Note
 * **UI Assembly Component**: Composes Sovereign Chat sub-components (Header, Multiversal Branches,
 * Breadcrumb, Scope Selector, Message List / Transcript, Suspended-Agent Resume Banner, Input Bar,
 * and ArtifactWorkspace) with RAF streaming scroll-lock backpressure.
 *
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Scroll container not auto-scrolling (if scroll_ref not attached) or user scroll lock.
 * - **Telemetry Link**: Search `[Chat_Content]` in React DevTools Profiler.
 */

import React, { useRef, useEffect } from 'react';
import { type DragControls, AnimatePresence, motion } from 'framer-motion';
import clsx from 'clsx';
import { AlertTriangle, Play } from 'lucide-react';
import { type Voice_Status } from '../../services/voice_client';
import { i18n } from '../../i18n';
import { type Sovereign_Scope, type Chat_Message, type SessionLeaf } from '../../stores/sovereign_store';
import { type Mission_Cluster } from '../../stores/workspace_store';
import type { Agent, Execution_Mode } from '../../types';
import { Chat_Header } from './Chat_Header';
import { Chat_Lineage_Breadcrumb } from './Chat_Lineage_Breadcrumb';
import { Chat_Scope_Selector } from './Chat_Scope_Selector';
import { Chat_Message_List } from './Chat_Message_List';
import { Chat_Input_Bar } from './Chat_Input_Bar';
import { agent_api_service } from '../../services/agent_api_service';
import { use_agent_registry_store } from '../../stores/agent_registry_store';
import { Buffered_Transcript_View } from '../transcript/Buffered_Transcript_View';
import { ArtifactWorkspace } from './ArtifactWorkspace';

export type { SessionLeaf };

export interface Chat_Content_Props {
    is_detached: boolean;
    active_scope: Sovereign_Scope;
    target_node: string;
    target_agent: string;
    target_cluster: string;
    selected_agent_id: string | null;

    // Voice state
    is_speaking: boolean;
    voice_status: Voice_Status;
    voice_error?: string | null;
    on_clear_voice_error?: () => void;

    // Transcript toggle
    show_transcript: boolean;
    set_show_transcript: (show: boolean) => void;

    // Messages
    messages: Chat_Message[];
    max_rendered_messages: number;

    // Input & dispatch
    input_text: string;
    set_input_text: (text: string) => void;
    on_send: () => Promise<void>;

    // Voice controls
    on_toggle_voice: () => void;
    on_toggle_speech: () => void;
    is_speech_enabled: boolean;

    // Safety & 3-Mode Execution
    on_toggle_safety: () => void;
    is_safe_mode: boolean;
    execution_mode?: Execution_Mode;
    on_change_execution_mode?: (mode: Execution_Mode) => void;
    can_launch_swarm?: boolean;
    on_launch_swarm?: () => void;

    // Window controls
    on_toggle_detach: () => void;
    on_clear_history: () => void;
    on_minimize: () => void;

    // Scope & target
    on_set_scope: (scope: Sovereign_Scope) => void;
    open_dropdown: 'agent' | 'cluster' | null;
    set_open_dropdown: (val: 'agent' | 'cluster' | null) => void;
    sorted_agents: Agent[];
    set_target_agent: (name: string) => void;
    set_selected_agent_id: (id: string) => void;
    set_target_cluster: (name: string) => void;
    clusters: Mission_Cluster[];

    // Preserved Multiversal & Workspace props
    activeNodeId?: string | null;
    activeMissionId?: string | null;
    sessionLeaves?: SessionLeaf[];
    onFetchHistory?: (mission_id: string, leaf_id: string) => Promise<void>;
    onFetchLeaves?: (mission_id: string) => Promise<void>;
    showBranches?: boolean;
    setShowBranches?: (show: boolean) => void;
    isWorkspaceOpen?: boolean;
    setWorkspaceOpen?: (open: boolean) => void;
    onRevert?: (node_id: string) => void;

    // Optional: inline-only
    drag_controls?: DragControls;
    on_header_click?: () => void;
    container_props?: React.HTMLAttributes<HTMLDivElement>;
}

export const Chat_Content: React.FC<Chat_Content_Props> = ({
    is_detached,
    active_scope,
    target_node,
    target_agent,
    target_cluster,
    selected_agent_id,
    is_speaking,
    voice_status,
    voice_error,
    on_clear_voice_error,
    show_transcript,
    set_show_transcript,
    messages,
    max_rendered_messages,
    input_text,
    set_input_text,
    on_send,
    on_toggle_voice,
    on_toggle_speech,
    is_speech_enabled,
    on_toggle_safety,
    is_safe_mode,
    execution_mode,
    on_change_execution_mode,
    can_launch_swarm,
    on_launch_swarm,
    on_toggle_detach,
    on_clear_history,
    on_minimize,
    on_set_scope,
    open_dropdown,
    set_open_dropdown,
    sorted_agents,
    set_target_agent,
    set_selected_agent_id,
    set_target_cluster,
    clusters,
    activeNodeId,
    activeMissionId,
    sessionLeaves = [],
    onFetchHistory,
    onFetchLeaves,
    showBranches,
    setShowBranches,
    isWorkspaceOpen = false,
    setWorkspaceOpen,
    onRevert,
    drag_controls,
    on_header_click,
    container_props,
}) => {
    const scroll_ref = useRef<HTMLDivElement>(null);
    const is_user_scrolled_up_ref = useRef(false);
    const raf_id_ref = useRef<number | null>(null);
    const prev_messages_len_ref = useRef(messages.length);

    const handle_scroll = () => {
        if (!scroll_ref.current) return;
        const { scrollTop, scrollHeight, clientHeight } = scroll_ref.current;
        const distanceFromBottom = scrollHeight - (scrollTop + clientHeight);
        is_user_scrolled_up_ref.current = distanceFromBottom > 80;
    };

    useEffect(() => {
        const isNewMessage = messages.length > prev_messages_len_ref.current;
        prev_messages_len_ref.current = messages.length;

        if (isNewMessage) {
            const lastMsg = messages[messages.length - 1];
            if (lastMsg && lastMsg.sender_id === '0') {
                is_user_scrolled_up_ref.current = false;
            }
        }

        if (is_user_scrolled_up_ref.current) {
            return;
        }

        if (raf_id_ref.current !== null) {
            cancelAnimationFrame(raf_id_ref.current);
        }

        raf_id_ref.current = requestAnimationFrame(() => {
            if (scroll_ref.current) {
                if (isNewMessage && typeof scroll_ref.current.scrollTo === 'function') {
                    scroll_ref.current.scrollTo({
                        top: scroll_ref.current.scrollHeight,
                        behavior: 'smooth'
                    });
                } else {
                    scroll_ref.current.scrollTop = scroll_ref.current.scrollHeight;
                }
            }
            raf_id_ref.current = null;
        });

        return () => {
            if (raf_id_ref.current !== null) {
                cancelAnimationFrame(raf_id_ref.current);
            }
        };
    }, [messages]);

    const targeted_agent = active_scope === 'agent'
        ? (sorted_agents || []).find(a => (selected_agent_id && a.id === selected_agent_id) || a.name === target_agent)
        : null;

    return (
        <div className="w-full h-full flex flex-col relative" {...container_props}>
            {!is_detached && <div className="neural-grid opacity-[0.05] absolute inset-0 pointer-events-none" />}

            {setWorkspaceOpen && (
                <ArtifactWorkspace
                    agentId={selected_agent_id || 'ceo'}
                    missionId={activeMissionId || 'global'}
                    isOpen={isWorkspaceOpen}
                    onClose={() => setWorkspaceOpen(false)}
                />
            )}

            <Chat_Header
                is_detached={is_detached}
                active_scope={active_scope}
                target_node={target_node}
                is_speaking={is_speaking}
                voice_status={voice_status}
                show_transcript={show_transcript}
                on_toggle_transcript={() => set_show_transcript(!show_transcript)}
                on_minimize={on_minimize}
                on_toggle_detach={on_toggle_detach}
                on_clear_history={on_clear_history}
                drag_controls={drag_controls}
                on_header_click={on_header_click}
                activeMissionId={activeMissionId}
                isWorkspaceOpen={isWorkspaceOpen}
                setWorkspaceOpen={setWorkspaceOpen}
                showBranches={showBranches}
                setShowBranches={setShowBranches}
                onFetchLeaves={onFetchLeaves}
            />

            {/* Preserved Multiversal Branch Selector Dropdown */}
            <AnimatePresence>
                {showBranches && (
                    <motion.div
                        initial={{ opacity: 0, height: 0 }}
                        animate={{ opacity: 1, height: 'auto' }}
                        exit={{ opacity: 0, height: 0 }}
                        className="bg-zinc-950 border-b border-zinc-800/50 overflow-hidden relative z-30"
                    >
                        <div className="p-3 flex flex-col gap-2">
                            <span className="text-[10px] text-zinc-500 font-bold uppercase tracking-wider">{i18n.t('chat.active_branches', { defaultValue: 'Active Timeline Branches' })}</span>
                            {sessionLeaves.length === 0 ? (
                                <span className="text-[10px] text-zinc-700 italic">{i18n.t('chat.no_branches', { defaultValue: 'No parallel branches detected. Use @fork to split reality.' })}</span>
                            ) : (
                                <div className="grid grid-cols-1 gap-1 max-h-40 overflow-y-auto custom-scrollbar pr-1">
                                    {sessionLeaves.map((leaf) => (
                                        <button
                                            key={leaf.id}
                                            onClick={() => {
                                                if (activeMissionId && onFetchHistory) void onFetchHistory(activeMissionId, leaf.id);
                                                setShowBranches?.(false);
                                            }}
                                            className={clsx(
                                                "text-left p-2 rounded border transition-all flex items-center justify-between group",
                                                activeNodeId === leaf.id
                                                    ? "bg-cyan-500/10 border-cyan-500/30 text-cyan-400"
                                                    : "bg-zinc-900 border-zinc-800 text-zinc-400 hover:border-zinc-700 hover:text-zinc-100"
                                            )}
                                        >
                                            <div className="flex flex-col min-w-0">
                                                <span className="text-[10px] font-mono truncate">{leaf.content.substring(0, 30)}...</span>
                                                <span className="text-[8px] opacity-50">{new Date(leaf.created_at).toLocaleString()}</span>
                                            </div>
                                            {activeNodeId === leaf.id && <div className="w-1.5 h-1.5 rounded-full bg-cyan-500 shadow-[0_0_8px_rgba(34,211,238,0.8)]" />}
                                        </button>
                                    ))}
                                </div>
                            )}
                        </div>
                    </motion.div>
                )}
            </AnimatePresence>

            {active_scope === 'agent' && (
                <Chat_Lineage_Breadcrumb target_agent={target_agent} />
            )}

            <Chat_Scope_Selector
                active_scope={active_scope}
                on_set_scope={on_set_scope}
                target_agent={target_agent}
                target_cluster={target_cluster}
                open_dropdown={open_dropdown}
                set_open_dropdown={set_open_dropdown}
                sorted_agents={sorted_agents}
                clusters={clusters}
                set_target_agent={set_target_agent}
                set_selected_agent_id={set_selected_agent_id}
                set_target_cluster={set_target_cluster}
                set_input_text={set_input_text}
            />

            {/* Messages Window */}
            <div
                ref={scroll_ref}
                onScroll={handle_scroll}
                className="relative z-10 flex-1 min-h-0 overflow-y-auto p-5 space-y-6 custom-scrollbar"
            >
                {show_transcript ? (
                    <Buffered_Transcript_View agent_id={selected_agent_id || undefined} />
                ) : (
                    <Chat_Message_List
                        messages={messages}
                        max_rendered={max_rendered_messages}
                        active_scope={active_scope}
                        target_node={target_node}
                        onRevert={onRevert}
                    />
                )}
            </div>

            {/* Preserved Suspended-Agent Resume Banner */}
            {active_scope === 'agent' && targeted_agent?.status === 'suspended' && (
                <div className="mx-4 mb-2 p-2.5 bg-amber-500/10 border border-amber-500/30 rounded-lg flex items-center justify-between text-xs text-amber-300 relative z-20 animate-in fade-in slide-in-from-bottom-2 duration-200">
                    <span className="flex items-center gap-1.5">
                        <AlertTriangle size={14} className="text-amber-400 shrink-0" />
                        <span>Agent <strong>{targeted_agent.name}</strong> is administratively suspended.</span>
                    </span>
                    <button
                        type="button"
                        onClick={async () => {
                            if (targeted_agent.id) {
                                try {
                                    await agent_api_service.resume_agent(targeted_agent.id);
                                    use_agent_registry_store.getState().update_agent(targeted_agent.id, { status: 'idle' });
                                } catch (err) {
                                    console.error('Failed to resume agent:', err);
                                }
                            }
                        }}
                        className="px-2.5 py-1 bg-amber-500/20 hover:bg-amber-500/30 border border-amber-500/40 rounded text-amber-200 font-semibold transition-colors flex items-center gap-1 cursor-pointer active:scale-95"
                    >
                        <Play size={12} />
                        Resume Agent
                    </button>
                </div>
            )}

            <Chat_Input_Bar
                active_scope={active_scope}
                is_safe_mode={is_safe_mode}
                execution_mode={execution_mode}
                on_change_execution_mode={on_change_execution_mode}
                can_launch_swarm={can_launch_swarm}
                on_launch_swarm={on_launch_swarm}
                is_speech_enabled={is_speech_enabled}
                is_speaking={is_speaking}
                input_value={input_text}
                on_change={set_input_text}
                on_send={on_send}
                on_toggle_voice={on_toggle_voice}
                on_toggle_speech={on_toggle_speech}
                on_toggle_safety={on_toggle_safety}
                is_listening={voice_status !== 'idle'}
                voice_error={voice_error}
                on_clear_voice_error={on_clear_voice_error}
            />
        </div>
    );
};

// Metadata: [Chat_Content]

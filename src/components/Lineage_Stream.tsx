/**
 * @docs ARCHITECTURE:Interface
 * 
 * ### AI Assist Note
 * **UI Component**: Hierarchical telemetry stream visualizing parent-child agent relationships. 
 * Maps OTel trace spans into a recursive tree structure, visualizing real-time completion status and latency.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Tree depth overflow leading to memory pressure, zombie spans if the `active_trace_id` changes during a stream flush, or drag-resize collision with internal scrollbars.
 * - **Telemetry Link**: Search for `[Lineage_Stream]` or `trace_stream` in UI tracing.
 */

import React, { useState, useRef, useMemo } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { Activity, Share2, Clock, GitCommit, ExternalLink, Minimize2 } from 'lucide-react';
import { use_agent_store } from '../stores/agent_store';
import { use_trace_store, type Trace_Node } from '../stores/trace_store';
import { use_tab_store } from '../stores/tab_store';
import { i18n } from '../i18n';
import clsx from 'clsx';
import { Tooltip } from './ui';
import { decodeAAAK, isAAAK } from '../utils/aaak_decoder';

// Failsafe Error Boundary for Lineage Tree
class LocalErrorBoundary extends React.Component<{ children: React.ReactNode }, { hasError: boolean }> {
    constructor(props: { children: React.ReactNode }) {
        super(props);
        this.state = { hasError: false };
    }
    
    static getDerivedStateFromError() {
        return { hasError: true };
    }
    
    componentDidCatch(error: Error, errorInfo: React.ErrorInfo) {
        console.error("[LocalErrorBoundary] Lineage Stream crashed:", error, errorInfo);
    }
    
    render() {
        if (this.state.hasError) {
            return (
                <div className="p-4 bg-red-950/20 border border-red-500/20 text-red-400 font-mono text-xs rounded-lg m-4">
                    ⚠️ Observability Link Degraded: Lineage stream rendering failure.
                </div>
            );
        }
        return this.props.children;
    }
}

// Recursive component to render the OTel trace tree
// PERF: Memoize to prevent full tree re-renders on minor status updates
const Trace_Tree_Node = React.memo(({ node, depth }: { node: Trace_Node; depth: number }): React.ReactElement => {
    const { get_agent } = use_agent_store();
    const agent_name = (node.agent_id && get_agent(node.agent_id)?.name) || node.agent_id || 'System';

    // Status colors
    const status_color = node.status === 'running'
        ? 'text-cyan-400 bg-cyan-400/10 border-cyan-400/30'
        : node.status === 'error'
            ? 'text-red-400 bg-red-400/10 border-red-400/30'
            : 'text-emerald-400 bg-emerald-400/10 border-emerald-400/30';

    return (
        <div className="w-full flex flex-col pt-2">
            <div
                className="flex items-start gap-3 relative"
                style={{ marginLeft: `${depth * 16}px` }}
            >
                {/* Visual line connecting parent to child */}
                {depth > 0 && (
                    <div className="absolute -left-4 top-4 w-4 h-px bg-zinc-700" />
                )}
                {depth > 0 && (
                    <div className="absolute -left-4 -top-full bottom-auto h-[calc(100%+16px)] w-px bg-zinc-700" />
                )}

                <div className="flex-1 p-3 bg-zinc-900/60 border border-zinc-800 rounded-xl hover:bg-zinc-800/80 transition-all">
                    <div className="flex items-center justify-between mb-2">
                        <div className="flex items-center gap-2">
                            <div className={`w-2 h-2 rounded-full shadow-[0_0_8px_currentColor] ${status_color}`} />
                            <span className="text-[10px] font-mono font-bold text-zinc-300 uppercase tracking-widest">{agent_name}</span>
                            <span className="text-[9px] text-zinc-500 font-mono px-1.5 py-0.5 rounded bg-zinc-950/50">
                                {node.name}
                            </span>
                            {node.attributes?.parent_node_id && node.attributes.parent_node_id !== 'root' && (
                                <span className="text-[8px] bg-cyan-500/20 text-cyan-400 border border-cyan-500/30 px-1.5 rounded-full font-bold uppercase tracking-tighter animate-pulse">
                                    Multiversal Branch
                                </span>
                            )}
                        </div>
                        <span className="text-[9px] text-zinc-600 font-mono flex items-center gap-1">
                            <Clock size={10} />
                            {node.end_time ? `${node.end_time - node.start_time}ms` : (i18n.t('trace.running') || 'Running')}
                        </span>
                    </div>

                    <div className="flex items-center gap-4 mt-2 pt-2 border-t border-zinc-800/50">
                        <span className="text-[8px] font-mono text-zinc-500 flex items-center gap-1">
                            <GitCommit size={10} /> {i18n.t('trace.span')}: {node.id.toUpperCase()}
                        </span>
                        {node.attributes && Object.keys(node.attributes).length > 0 && (
                            <div className="text-[8px] font-mono text-zinc-500 truncate max-w-[180px]">
                                {Object.entries(node.attributes).map(([k, v]) => {
                                    const val = String(v ?? '');
                                    return (
                                        <div key={k} className="flex gap-1 truncate">
                                            <span className="text-zinc-500">{k}:</span>
                                            {isAAAK(val) ? (
                                                <Tooltip content={decodeAAAK(val)} position="top">
                                                    <span className="text-zinc-400 cursor-help border-b border-zinc-800">{val}</span>
                                                </Tooltip>
                                            ) : (
                                                <span className="text-zinc-400 truncate">{val}</span>
                                            )}
                                        </div>
                                    );
                                })}
                            </div>
                        )}
                    </div>
                </div>
            </div>

            {/* Recursively render children */}
            {node.children && node.children.length > 0 && (
                <div className="flex flex-col relative w-full">
                    {(node.children || []).map((child): React.ReactElement => (
                        <Trace_Tree_Node key={child.id} node={child} depth={depth + 1} />
                    ))}
                </div>
            )}
        </div>
    );
}, (prev, next) => {
    return (
        prev.node.status === next.node.status &&
        prev.node.end_time === next.node.end_time &&
        prev.node.children === next.node.children &&
        prev.depth === next.depth
    );
});

/**
 * Lineage_Stream
 * A real-time hierarchical telemetry feed visualizing the swarm's activity and parent-child relationships.
 * Refactored for strict snake_case compliance and consistent prop propagation.
 */
export const Lineage_Stream: React.FC<{ is_detached_view?: boolean }> = ({ is_detached_view = false }): React.ReactElement => {
    const { active_trace_id, spans, get_trace_tree, set_active_trace } = use_trace_store();
    const { is_lineage_stream_detached, toggle_lineage_stream_detachment } = use_tab_store();
    const [sidebar_width, set_sidebar_width] = useState(380);
    const stream_ref = useRef<HTMLDivElement>(null);
    const resize_handlers_ref = useRef<{ on_move: (e: MouseEvent) => void; on_up: () => void } | null>(null);

    // SAFETY: Ensure event listeners are cleaned up if component unmounts during drag
    React.useEffect(() => {
        return () => {
            if (resize_handlers_ref.current) {
                document.removeEventListener('mousemove', resize_handlers_ref.current.on_move);
                document.removeEventListener('mouseup', resize_handlers_ref.current.on_up);
            }
        };
    }, []);

    // Extract unique traces available in the trace buffer
    const available_traces = useMemo(() => {
        const traces = new Set<string>();
        Object.values(spans || {}).forEach(s => {
            if (s?.trace_id) traces.add(s.trace_id);
        });
        return Array.from(traces);
    }, [spans]);

    // Fall back to most recent trace_id if active_trace_id is not explicitly set
    const effective_trace_id = useMemo(() => {
        if (active_trace_id) return active_trace_id;
        const all_spans = Object.values(spans || {});
        for (let i = all_spans.length - 1; i >= 0; i--) {
            if (all_spans[i]?.trace_id) {
                return all_spans[i].trace_id;
            }
        }
        return null;
    }, [active_trace_id, spans]);

    const active_tree = useMemo(() => {
        if (!effective_trace_id) return [];
        return get_trace_tree(effective_trace_id);
    }, [effective_trace_id, get_trace_tree, spans]);

    const handle_sidebar_resize_start = (e: React.MouseEvent): void => {
        if (is_detached_view) return;
        e.preventDefault();
        const start_x = e.clientX;
        const start_width = sidebar_width;

        const on_mouse_move = (move_event: MouseEvent): void => {
            const current_x = move_event.clientX;
            const delta_x = start_x - current_x;
            set_sidebar_width(Math.min(800, Math.max(300, start_width + delta_x)));
        };

        const on_mouse_up = (): void => {
            document.removeEventListener('mousemove', on_mouse_move);
            document.removeEventListener('mouseup', on_mouse_up);
            resize_handlers_ref.current = null;
        };

        resize_handlers_ref.current = { on_move: on_mouse_move, on_up: on_mouse_up };
        document.addEventListener('mousemove', on_mouse_move);
        document.addEventListener('mouseup', on_mouse_up);
    };

    return (
        <LocalErrorBoundary>
            <div
                className={clsx(
                    "flex flex-col bg-zinc-950/20 border-l border-zinc-900 overflow-hidden relative group/sidebar",
                    !is_detached_view && "flex-1 sovereign-card mb-4",
                    is_detached_view && "h-full"
                )}
                style={{ width: is_detached_view ? '100%' : sidebar_width }}
                ref={stream_ref}
            >
                {!is_detached_view && <div className="neural-grid opacity-[0.05]" />}
                {!is_detached_view && (
                    <div
                        onMouseDown={handle_sidebar_resize_start}
                        className="absolute inset-y-0 left-0 w-1 cursor-col-resize hover:bg-emerald-500/20 active:bg-emerald-500/40 transition-colors z-20"
                    />
                )}

                <Tooltip content={i18n.t('trace.tooltip')} position="bottom">
                    <div className="relative z-10 p-3 border-b border-zinc-800 bg-zinc-950 flex items-center justify-between transition-colors cursor-help">
                        <h3 className="sovereign-header-text flex items-center gap-2">
                            <Activity size={12} className="text-emerald-500" />
                            {i18n.t('trace.stream_title')}
                            {available_traces.length > 1 ? (
                                <select
                                    aria-label="Select Active Trace"
                                    value={effective_trace_id || ''}
                                    onChange={(e) => set_active_trace(e.target.value)}
                                    onClick={(e) => e.stopPropagation()}
                                    className="text-[9px] font-mono bg-zinc-900 border border-zinc-800 text-zinc-300 rounded px-1.5 py-0.5 max-w-[140px] truncate focus:outline-none focus:border-emerald-500 ml-2"
                                >
                                    {available_traces.map(tid => (
                                        <option key={tid} value={tid}>{tid}</option>
                                    ))}
                                </select>
                            ) : effective_trace_id ? (
                                <span className="text-[9px] font-mono px-2 py-0.5 rounded bg-zinc-900 border border-zinc-800 text-zinc-500 truncate max-w-[120px] ml-2 normal-case tracking-normal">
                                    {effective_trace_id}
                                </span>
                            ) : null}
                        </h3>
                        
                        <div className="flex items-center gap-3">
                            <div className="flex gap-1.5 mr-2">
                                <div className="w-2.5 h-2.5 rounded-full bg-zinc-800 border border-zinc-700"></div>
                                <div className="w-2.5 h-2.5 rounded-full bg-zinc-800 border border-zinc-700"></div>
                            </div>
                            <button
                                onClick={() => toggle_lineage_stream_detachment()}
                                className="p-1 hover:bg-zinc-800 rounded-md text-zinc-500 hover:text-zinc-200 transition-colors"
                                title={is_lineage_stream_detached ? i18n.t('trace_stream.recall_tooltip') : i18n.t('trace_stream.detach_tooltip')}
                            >
                                {is_lineage_stream_detached ? <Minimize2 size={14} /> : <ExternalLink size={14} />}
                            </button>
                        </div>
                    </div>
                </Tooltip>

                <div className="flex-1 overflow-y-auto p-4 space-y-2 custom-scrollbar relative">
                    <AnimatePresence>
                        {(active_tree || []).map((root_node): React.ReactElement => (
                            <motion.div
                                key={root_node.id}
                                initial={{ opacity: 0, scale: 0.95 }}
                                animate={{ opacity: 1, scale: 1 }}
                                exit={{ opacity: 0 }}
                            >
                                <Trace_Tree_Node node={root_node} depth={0} />
                            </motion.div>
                        ))}
                    </AnimatePresence>

                    {active_tree.length === 0 && (
                        <div className="flex flex-col items-center justify-center h-64 text-center px-4">
                            <Share2 size={32} className="text-zinc-600 mb-3 animate-pulse" />
                            <span className="sovereign-header-text !text-zinc-400 font-semibold tracking-wide">
                                {i18n.t('trace.waiting') || 'AWAITING TELEMETRY'}
                            </span>
                            <p className="text-[11px] font-mono text-zinc-500 mt-2 max-w-xs">
                                {i18n.t('trace.waiting_hint') || 'No active OTel execution trace tree. Launch an agent task or dispatch diagnostic telemetry.'}
                            </p>
                            <button
                                onClick={() => {
                                    const now = Date.now();
                                    const trace_id = `diagnostic-${now.toString(16)}`;
                                    const root_id = `span-diag-root-${now.toString(16)}`;
                                    const child_id = `span-diag-child-${now.toString(16)}`;
                                    const store = use_trace_store.getState();
                                    store.add_span({
                                        id: root_id,
                                        trace_id,
                                        name: 'MissionOrchestration::InfrastructureAudit',
                                        agent_id: '1',
                                        mission_id: 'mission-diagnostic',
                                        start_time: now - 850,
                                        status: 'running',
                                        attributes: { type: 'diagnostic_probe', severity: 'info' }
                                    });
                                    store.add_span({
                                        id: child_id,
                                        trace_id,
                                        parent_id: root_id,
                                        name: 'ToolExecution::parity_guard',
                                        agent_id: '2',
                                        mission_id: 'mission-diagnostic',
                                        start_time: now - 520,
                                        status: 'running',
                                        attributes: { tool: 'parity_guard', module: 'telemetry' }
                                    });
                                    store.set_active_trace(trace_id);
                                }}
                                className="mt-4 px-3 py-1.5 bg-emerald-950/40 hover:bg-emerald-900/60 border border-emerald-500/30 hover:border-emerald-500/60 text-emerald-400 text-[10px] font-mono rounded transition-colors uppercase tracking-wider flex items-center gap-1.5 cursor-pointer"
                            >
                                <Activity size={12} className="text-emerald-400" />
                                Dispatch Diagnostic Telemetry
                            </button>
                        </div>
                    )}
                </div>
            </div>
        </LocalErrorBoundary>
    );
};


// Metadata: [Lineage_Stream]

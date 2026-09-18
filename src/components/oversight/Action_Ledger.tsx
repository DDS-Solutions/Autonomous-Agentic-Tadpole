/**
 * @docs ARCHITECTURE:UI-Components
 * 
 * ### AI Assist Note
 * **Action_Ledger Component**
 * Ported from TadpoleOS-Dev design with HITL Approvals vs Auto-Approved segmented toggle.
 * Complies with docs/design.md Neural Glass tokens (Zinc-900 surface, 12px rounded-xl, backdrop-blur-xl, 1px border).
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Filtering mismatch or state desync between HITL and Auto views.
 * - **Telemetry Link**: Search `[Action_Ledger]` in observability traces.
 */

import React, { useState, useRef, useEffect, useMemo } from 'react';
import { createPortal } from 'react-dom';
import { Activity, Target, Search, Zap, ShieldCheck, X } from 'lucide-react';
import { motion, AnimatePresence } from 'framer-motion';
import clsx from 'clsx';
import type { LedgerEntry } from '../../data/mock_oversight';
import type { Mission_Cluster } from '../../stores/workspace_store';
import { Tooltip, Tw_Empty_State } from '../ui';
import { Z_INDEX_MAP } from '../ui/theme_tokens';
import { useViewportPosition } from '../../hooks/use_viewport_position';
import { i18n } from '../../i18n';
import { get_safe_date } from '../../utils/date_utils';
import { check_is_auto, get_entry_time } from '../../utils/oversight_utils';

interface ParamsCellProps {
    params: unknown;
}

const EMPTY_PARAMS: Record<string, unknown> = {};

const Params_Cell: React.FC<ParamsCellProps> = ({ params }) => {
    const [is_open, set_is_open] = useState(false);
    const [is_hovered, set_is_hovered] = useState(false);
    const cell_ref = useRef<HTMLDivElement>(null);
    const popover_ref = useRef<HTMLDivElement>(null);

    const show_box = is_open || is_hovered;

    const { coords, actual_position, update_position } = useViewportPosition({
        trigger_ref: cell_ref,
        content_ref: popover_ref,
        position: 'top',
        is_visible: show_box,
        offset: 8,
        padding: 8
    });

    const raw_params = params || EMPTY_PARAMS;
    const formatted_json = useMemo(() => {
        if (!show_box) return '';
        if (typeof raw_params === 'string') {
            try {
                return JSON.stringify(JSON.parse(raw_params), null, 2);
            } catch {
                return raw_params;
            }
        }
        return JSON.stringify(raw_params, null, 2);
    }, [raw_params, show_box]);

    const inline_json = useMemo(() => {
        return typeof raw_params === 'string' ? raw_params : JSON.stringify(raw_params);
    }, [raw_params]);

    useEffect(() => {
        if (!show_box) return;

        const handle_scroll_or_resize = () => {
            update_position();
        };

        window.addEventListener('scroll', handle_scroll_or_resize, true);
        window.addEventListener('resize', handle_scroll_or_resize);

        return () => {
            window.removeEventListener('scroll', handle_scroll_or_resize, true);
            window.removeEventListener('resize', handle_scroll_or_resize);
        };
    }, [show_box, update_position]);

    useEffect(() => {
        if (!is_open) return;

        const handle_click_outside = (e: MouseEvent) => {
            const target = e.target as Node;
            if (
                cell_ref.current && !cell_ref.current.contains(target) &&
                popover_ref.current && !popover_ref.current.contains(target)
            ) {
                set_is_open(false);
            }
        };

        const handle_key_down = (e: KeyboardEvent) => {
            if (e.key === 'Escape') {
                set_is_open(false);
            }
        };

        document.addEventListener('mousedown', handle_click_outside);
        document.addEventListener('keydown', handle_key_down);
        return () => {
            document.removeEventListener('mousedown', handle_click_outside);
            document.removeEventListener('keydown', handle_key_down);
        };
    }, [is_open]);

    return (
        <div 
            ref={cell_ref} 
            className="relative inline-block"
            onMouseEnter={() => {
                update_position();
                set_is_hovered(true);
            }}
            onMouseLeave={() => set_is_hovered(false)}
        >
            <button
                type="button"
                onClick={() => {
                    update_position();
                    set_is_open(prev => !prev);
                }}
                className={clsx(
                    "max-w-[140px] truncate text-[10px] font-mono px-2 py-1 rounded border transition-all cursor-pointer select-none text-left flex items-center justify-between gap-1",
                    is_open 
                        ? "bg-green-500/20 text-green-300 border-green-500/50 ring-1 ring-green-500/30" 
                        : "bg-zinc-950/30 hover:bg-zinc-950/50 text-zinc-400 border-zinc-800/50 hover:text-zinc-200"
                )}
                title={i18n.t('oversight.click_to_pin_params') || "Click to pin parameters box"}
            >
                <span className="truncate">{inline_json}</span>
            </button>

            {createPortal(
                <AnimatePresence>
                    {show_box && (
                        <motion.div
                            key="params-popover"
                            ref={popover_ref}
                            initial={{ opacity: 0, scale: 0.95, y: actual_position === 'top' ? 4 : -4 }}
                            animate={{ opacity: 1, scale: 1, y: 0 }}
                            exit={{ opacity: 0, scale: 0.95 }}
                            transition={{ duration: 0.15, ease: "easeOut" }}
                            style={{
                                position: 'fixed',
                                left: coords.x,
                                top: coords.y,
                                transform: actual_position === 'top' 
                                    ? 'translate(-50%, -100%)' 
                                    : 'translate(-50%, 0)',
                                zIndex: Z_INDEX_MAP.dialog + 50,
                            }}
                            className={clsx(
                                "w-96 max-w-md bg-zinc-950/95 border rounded-xl shadow-2xl p-3 backdrop-blur-xl flex flex-col text-left normal-case tracking-normal pointer-events-auto",
                                is_open ? "border-green-500/60 ring-2 ring-green-500/30 shadow-green-950/50" : "border-zinc-700/60"
                            )}
                            onMouseEnter={() => set_is_hovered(true)}
                            onMouseLeave={() => set_is_hovered(false)}
                            onClick={(e) => e.stopPropagation()}
                        >
                            <div className="text-[9px] font-bold text-zinc-400 uppercase tracking-wider mb-2 pb-1.5 border-b border-zinc-800/80 flex items-center justify-between select-none font-mono">
                                <span className="flex items-center gap-1.5 text-zinc-300">
                                    <span className={clsx("w-2 h-2 rounded-full", is_open ? "bg-green-400 animate-pulse" : "bg-cyan-400")} />
                                    {i18n.t('oversight.table_params') || 'Action Parameters'}
                                    {is_open && (
                                        <span className="text-[8px] bg-green-500/10 text-green-400 px-1.5 py-0.5 rounded border border-green-500/20 ml-1 font-bold">
                                            PINNED
                                        </span>
                                    )}
                                </span>

                                <div className="flex items-center gap-2">
                                    <span className="text-[8px] text-zinc-500 font-mono">JSON</span>
                                    {is_open && (
                                        <button
                                            type="button"
                                            onClick={() => set_is_open(false)}
                                            className="p-0.5 text-zinc-500 hover:text-zinc-200 hover:bg-zinc-800 rounded transition-colors cursor-pointer"
                                            title="Close"
                                        >
                                            <X size={12} />
                                        </button>
                                    )}
                                </div>
                            </div>

                            <div className="relative">
                                <pre className="font-mono text-[11px] text-zinc-300 leading-relaxed overflow-auto custom-scrollbar p-2.5 bg-zinc-950/60 rounded-lg border border-zinc-800/60 max-h-64 whitespace-pre-wrap break-all select-text">
                                    {formatted_json}
                                </pre>
                            </div>

                            {!is_open && (
                                <div className="mt-1.5 text-[8px] font-mono text-zinc-500 text-right select-none">
                                    {i18n.t('oversight.click_to_lock') || 'Click box to pin open & scroll'}
                                </div>
                            )}
                        </motion.div>
                    )}
                </AnimatePresence>,
                document.getElementById('portal-root') || document.body
            )}
        </div>
    );
};

interface ActionLedgerProps {
    ledger: LedgerEntry[];
    filter: string;
    set_filter: (filter: string) => void;
    selected_cluster_id: string;
    set_selected_cluster_id: (id: string) => void;
    clusters: Mission_Cluster[];
    resolve_agent_name: (id: string) => string;
}

export const Action_Ledger: React.FC<ActionLedgerProps> = ({
    ledger,
    filter,
    set_filter,
    selected_cluster_id,
    set_selected_cluster_id,
    clusters,
    resolve_agent_name
}) => {
    const [view_mode, set_view_mode] = useState<'hitl' | 'auto'>('hitl');

    // Filter ledger entries by search filter, cluster filter, and view_mode toggle
    const filtered_ledger = useMemo(() => {
        const search_lower = filter ? filter.toLowerCase() : '';

        return ledger.filter(entry => {
            // Cluster Filter
            if (selected_cluster_id && selected_cluster_id !== 'all') {
                const tc = entry.tool_call || entry;
                const entry_cluster = tc.cluster_id;
                if (entry_cluster && entry_cluster !== selected_cluster_id) return false;
            }

            // Approval type filter using DRY helper
            const is_auto = check_is_auto(entry);
            if (view_mode === 'hitl' && is_auto) return false;
            if (view_mode === 'auto' && !is_auto) return false;

            // Fast Search Filter
            if (search_lower) {
                const skill_name = (entry.tool_call?.skill || entry.skill || '').toLowerCase();
                if (skill_name.includes(search_lower)) return true;

                const agent_id = (entry.tool_call?.agent_id || entry.agent_id || '').toLowerCase();
                if (agent_id.includes(search_lower)) return true;

                const agent_name = resolve_agent_name(agent_id).toLowerCase();
                if (agent_name.includes(search_lower)) return true;

                const desc = (entry.tool_call?.description || '').toLowerCase();
                if (desc.includes(search_lower)) return true;

                const decision_str = (entry.decision || '').toLowerCase();
                if (decision_str.includes(search_lower)) return true;

                const params_str = JSON.stringify(entry.tool_call?.params || entry.params || {}).toLowerCase();
                if (params_str.includes(search_lower)) return true;

                return false;
            }

            return true;
        }).sort((a, b) => get_entry_time(b) - get_entry_time(a));
    }, [ledger, view_mode, filter, selected_cluster_id, resolve_agent_name]);

    return (
        <div className="bg-zinc-900/60 backdrop-blur-xl border border-zinc-800/60 rounded-xl overflow-hidden flex flex-col h-[600px] sovereign-transition">
            {/* Header Bar */}
            <div className="p-4 border-b border-zinc-800/80 flex flex-wrap items-center justify-between gap-3 bg-zinc-950/40">
                <div className="flex items-center gap-3">
                    <div className="flex items-center gap-2">
                        <Tooltip content={i18n.t('oversight.ledger_tooltip') || "Real-time record of all governance decisions"} position="right">
                            <Activity className="w-4 h-4 text-green-400 cursor-help" />
                        </Tooltip>
                        <h2 className="font-semibold text-zinc-100 font-sans">{i18n.t('oversight.ledger_title') || "Action Ledger"}</h2>
                    </div>

                    {/* Segmented Toggle Control (HITL Approvals vs Auto-Approved) */}
                    <div className="flex items-center bg-black/50 border border-zinc-800 rounded-lg p-0.5 font-mono text-[10px] select-none">
                        <button
                            type="button"
                            onClick={() => set_view_mode('hitl')}
                            className={clsx(
                                "px-2.5 py-1 rounded-md font-bold uppercase transition-all duration-200 cursor-pointer flex items-center gap-1.5",
                                view_mode === 'hitl'
                                    ? "bg-green-500/15 text-green-300 border border-green-500/30 shadow-sm"
                                    : "text-zinc-400 hover:text-zinc-200 hover:bg-zinc-800/40"
                            )}
                            title="Show Human-in-the-Loop Approvals and Rejections"
                        >
                            <ShieldCheck size={12} className={view_mode === 'hitl' ? "text-green-400" : "text-zinc-500"} />
                            HITL Approvals
                        </button>
                        <button
                            type="button"
                            onClick={() => set_view_mode('auto')}
                            className={clsx(
                                "px-2.5 py-1 rounded-md font-bold uppercase transition-all duration-200 cursor-pointer flex items-center gap-1.5",
                                view_mode === 'auto'
                                    ? "bg-cyan-500/15 text-cyan-300 border border-cyan-500/30 shadow-sm"
                                    : "text-zinc-400 hover:text-zinc-200 hover:bg-zinc-800/40"
                            )}
                            title="Show System & Agent Auto-Approved Actions"
                        >
                            <Zap size={12} className={view_mode === 'auto' ? "text-cyan-400" : "text-zinc-500"} />
                            Auto-Approved
                        </button>
                    </div>
                </div>

                <div className="flex items-center gap-3">
                    <div className="relative">
                        <Tooltip content={i18n.t('oversight.filter_cluster_tooltip') || "Filter logs by mission cluster"} position="top">
                            <Target className="w-4 h-4 absolute left-3 top-1/2 -translate-y-1/2 text-zinc-500 cursor-help" />
                        </Tooltip>
                        <select
                            value={selected_cluster_id}
                            onChange={(e) => set_selected_cluster_id(e.target.value)}
                            className="bg-black/50 border border-zinc-700/80 rounded-full pl-9 pr-8 py-1.5 text-xs text-zinc-200 focus:outline-none focus:border-green-500 appearance-none cursor-pointer font-mono"
                        >
                            <option value="all">{i18n.t('oversight.all_missions') || "All Mission Clusters"}</option>
                            {(clusters || []).map(c => (
                                <option key={c.id} value={c.id}>{c.name}</option>
                            ))}
                        </select>
                    </div>
                    <div className="relative">
                        <Tooltip content={i18n.t('oversight.search_ledger_tooltip') || "Search action parameters, agents, and skills"} position="top">
                            <Search className="w-4 h-4 absolute left-3 top-1/2 -translate-y-1/2 text-zinc-500 cursor-help" />
                        </Tooltip>
                        <input
                            type="text"
                            placeholder={i18n.t('oversight.filter_actions_placeholder') || "Filter actions..."}
                            value={filter}
                            onChange={(e) => set_filter(e.target.value)}
                            className="bg-black/50 border border-zinc-700/80 rounded-full pl-9 pr-4 py-1.5 text-xs text-zinc-200 focus:outline-none focus:border-green-500 w-48 font-mono"
                        />
                    </div>
                </div>
            </div>

            {/* Table Area */}
            <div className="overflow-auto flex-1 p-0 custom-scrollbar">
                <table className="w-full text-left text-sm">
                    <thead className="bg-zinc-950 text-zinc-400 sticky top-0 z-10 border-b border-zinc-800">
                        <tr>
                            <th className="p-3 font-mono text-[10px] font-bold uppercase tracking-wider text-zinc-400">{i18n.t('oversight.table_time') || "Time"}</th>
                            <th className="p-3 font-mono text-[10px] font-bold uppercase tracking-wider text-zinc-400">{i18n.t('oversight.table_agent') || "Agent"}</th>
                            <th className="p-3 font-mono text-[10px] font-bold uppercase tracking-wider text-zinc-400">{i18n.t('oversight.table_action') || "Action"}</th>
                            <th className="p-3 font-mono text-[10px] font-bold uppercase tracking-wider text-zinc-400">{i18n.t('oversight.table_params') || "Params"}</th>
                            <th className="p-3 font-mono text-[10px] font-bold uppercase tracking-wider text-zinc-400">{i18n.t('oversight.table_result') || "Result"}</th>
                        </tr>
                    </thead>
                    <tbody className="divide-y divide-zinc-800/50">
                        {filtered_ledger.map(entry => {
                            const is_auto = check_is_auto(entry);
                            const agent_id = entry.tool_call?.agent_id || entry.agent_id || '';
                            const agent_name = resolve_agent_name(agent_id);
                            const params_obj = entry.tool_call?.params || entry.params || {};

                            return (
                                <tr key={entry.id} className="hover:bg-zinc-800/20 transition-colors">
                                    <td className="p-3 text-zinc-500 whitespace-nowrap font-mono text-[10px]">
                                        {get_safe_date(entry, new Date())?.toLocaleTimeString() || '--:--:--'}
                                    </td>
                                    <td className="p-3">
                                        <div className="flex items-center gap-2">
                                            <div className="w-6 h-6 rounded-md bg-zinc-800 flex items-center justify-center text-[10px] font-mono font-bold text-zinc-300 border border-zinc-700">
                                                {agent_name.charAt(0)}
                                            </div>
                                            <span className="text-zinc-300 text-xs font-bold font-sans">
                                                {agent_name}
                                                {agent_id && <span className="text-zinc-500 font-mono text-[10px] ml-1">({agent_id})</span>}
                                            </span>
                                        </div>
                                    </td>
                                    <td className="p-3">
                                        <div className="flex items-center gap-2">
                                            {is_auto ? (
                                                <span className="px-1.5 py-0.5 rounded text-[8px] font-mono font-bold uppercase tracking-wider bg-cyan-500/10 text-cyan-400 border border-cyan-500/20 flex items-center gap-1">
                                                    <Zap size={9} />
                                                    AUTO
                                                </span>
                                            ) : (
                                                <span className={clsx(
                                                    "px-1.5 py-0.5 rounded text-[8px] font-mono font-bold uppercase tracking-wider",
                                                    entry.decision === 'approved'
                                                        ? "bg-green-500/10 text-green-400 border border-green-500/20"
                                                        : "bg-red-500/10 text-red-400 border border-red-500/20"
                                                )}>
                                                    {entry.decision}
                                                </span>
                                            )}
                                            <span className="font-mono text-[10px] text-green-400/90">
                                                {entry.tool_call?.skill || entry.skill || i18n.t('oversight.proposal_label') || "Action"}
                                            </span>
                                        </div>
                                    </td>
                                    <td className="p-3">
                                        <Params_Cell params={params_obj} />
                                    </td>
                                    <td className="p-3">
                                        {entry.decision === 'rejected' ? (
                                            <span className="text-red-400 font-mono text-xs uppercase font-bold tracking-wider">{i18n.t('oversight.blocked_label') || "BLOCKED"}</span>
                                        ) : (
                                            <span className={clsx("text-xs font-mono font-bold uppercase tracking-wider", (!entry.result || entry.result.success) ? "text-green-400" : "text-red-400")}>
                                                {(!entry.result || entry.result.success) ? (i18n.t('oversight.success_label') || "SUCCESS") : (i18n.t('oversight.failed_label') || "FAILED")}
                                                {entry.result?.duration_ms !== undefined && (
                                                    <span className="text-zinc-600 ml-1 font-normal font-mono text-[10px]">({entry.result.duration_ms}ms)</span>
                                                )}
                                            </span>
                                        )}
                                    </td>
                                </tr>
                            );
                        })}
                        {filtered_ledger.length === 0 && (
                            <tr>
                                <td colSpan={5} className="p-8 text-center">
                                    <Tw_Empty_State
                                        title={view_mode === 'hitl' ? (i18n.t('oversight.no_actions_title') || 'No HITL Approvals') : 'No Auto-Approved Actions'}
                                        description={view_mode === 'hitl' ? (i18n.t('oversight.no_actions_description') || 'No human-in-the-loop governance actions logged yet.') : 'No autonomous system or agent auto-approved actions logged in this timeframe.'}
                                    />
                                </td>
                            </tr>
                        )}
                    </tbody>
                </table>
            </div>
        </div>
    );
};

// Metadata: [Action_Ledger]

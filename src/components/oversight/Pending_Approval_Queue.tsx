/**
 * @docs ARCHITECTURE:Interface
 *
 * ### AI Assist Note
 * **Pending Approval Queue**: Renders awaiting human-in-the-loop (HITL) decisions with
 * risk tier classification, parent delegation prioritization badges, suggested remediation
 * chips, operator guidance inputs, and low-risk batch approval triggers.
 *
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Risk misclassification or broken guidance propagation into `decide_oversight`.
 * - **Telemetry Link**: Search `[Pending_Approval_Queue]` in UI logs.
 */

import React, { type Dispatch, type SetStateAction } from 'react';
import {
    AlertTriangle,
    Terminal as TerminalIcon,
    Network,
    Sparkles,
    CheckCheck,
} from 'lucide-react';
import { Tooltip } from '../ui';
import { i18n } from '../../i18n';
import { get_safe_date } from '../../utils/date_utils';
import { is_parent_delegation_directive, classify_skill_risk } from '../../utils/oversight_utils';
import type { OversightEntry } from '../../data/mock_oversight';

export interface Pending_Approval_Queue_Props {
    filtered_pending: OversightEntry[];
    low_risk_count: number;
    parent_delegation_count: number;
    focused_index: number;
    set_focused_index: Dispatch<SetStateAction<number>>;
    guidance_map: Record<string, string>;
    set_guidance_map: Dispatch<SetStateAction<Record<string, string>>>;
    get_remediation: (err: { error_code?: string; help_link?: string }) => { label: string } | null | undefined;
    on_decide: (id: string, decision: 'approved' | 'rejected') => Promise<boolean>;
    on_request_batch_approve: () => void;
}

export const Pending_Approval_Queue: React.FC<Pending_Approval_Queue_Props> = ({
    filtered_pending,
    low_risk_count,
    parent_delegation_count,
    focused_index,
    set_focused_index,
    guidance_map,
    set_guidance_map,
    get_remediation,
    on_decide,
    on_request_batch_approve,
}) => {
    if (filtered_pending.length === 0) {
        return null;
    }

    return (
        <div className="bg-zinc-900 border border-yellow-500/30 rounded-lg overflow-hidden">
            <div className="bg-yellow-500/10 p-3 border-b border-yellow-500/20 flex flex-wrap items-center justify-between gap-3">
                <div className="flex items-center gap-2.5 flex-wrap">
                    <AlertTriangle className="w-4 h-4 text-yellow-500 shrink-0" />
                    <h2 className="font-semibold text-yellow-100">
                        {i18n.t('oversight.awaiting_approval_title', { count: filtered_pending.length })}
                    </h2>
                    <div
                        data-testid="parent-delegation-header-badge"
                        className={`flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-semibold border shadow-sm transition-all ${
                            parent_delegation_count > 0
                                ? 'bg-cyan-950/80 text-cyan-300 border-cyan-500/40 animate-pulse'
                                : 'bg-zinc-800/80 text-zinc-300 border-zinc-700/60'
                        }`}
                        title="Parent delegation directives coordinate multi-agent flows across the swarm hierarchy. Approving parent directives first allows subordinate tasks to be generated and processed cleanly."
                    >
                        <Network className={`w-3.5 h-3.5 ${parent_delegation_count > 0 ? 'text-cyan-400' : 'text-zinc-400'}`} />
                        <span>
                            Parent Delegation Directives {parent_delegation_count > 0 ? `(${parent_delegation_count})` : ''} — Coordinating Complex Multi-Agent Flows
                        </span>
                    </div>
                </div>
                <div className="flex items-center gap-2 flex-wrap">
                    <span className="text-[10px] font-mono text-zinc-400 bg-zinc-950/70 border border-zinc-800 px-2 py-0.5 rounded-md hidden sm:inline">
                        Keys: j/k focus · a approve · r reject · g guide
                    </span>
                    {low_risk_count > 0 && (
                        <button
                            type="button"
                            data-testid="batch-approve-low-risk-btn"
                            onClick={on_request_batch_approve}
                            title="Approve all low-risk read-only skills (mutating/shell actions are never batch-approved)"
                            aria-label={`Approve ${low_risk_count} low-risk actions`}
                            className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-lg bg-emerald-500/15 hover:bg-emerald-500/25 border border-emerald-500/40 text-emerald-300 text-[10px] font-mono font-bold uppercase tracking-wider focus:outline-none focus:ring-2 focus:ring-emerald-500"
                        >
                            <CheckCheck size={12} />
                            Approve Low-Risk ({low_risk_count})
                        </button>
                    )}
                    {parent_delegation_count > 0 && (
                        <span className="text-[11px] font-mono text-cyan-400 bg-cyan-950/80 px-2 py-0.5 rounded border border-cyan-500/30">
                            Prioritize parent delegation directives first
                        </span>
                    )}
                </div>
            </div>
            <div className="divide-y divide-zinc-800">
                {filtered_pending.map((entry, idx) => {
                    const skill_name = entry.tool_call?.skill || entry.skill;
                    const is_parent_directive = is_parent_delegation_directive(skill_name);
                    const risk_tier = classify_skill_risk(skill_name, entry.tool_call?.params ?? entry.params);
                    const is_focused = idx === Math.min(focused_index, filtered_pending.length - 1);
                    const risk_badge_cls =
                        risk_tier === 'low'
                            ? 'bg-emerald-500/10 text-emerald-400 border-emerald-500/30'
                            : risk_tier === 'medium'
                            ? 'bg-amber-500/10 text-amber-300 border-amber-500/30'
                            : 'bg-rose-500/10 text-rose-300 border-rose-500/30';

                    const params_obj = (entry.tool_call?.params || entry.params || {}) as Record<string, unknown>;
                    const direct_hint = typeof params_obj.remediation === 'string' ? params_obj.remediation : undefined;
                    const code_hint = entry.error_code
                        ? get_remediation({ error_code: entry.error_code, help_link: entry.help_link })?.label
                        : undefined;
                    const circuit_hint = (skill_name || '').includes('CIRCUIT_BREAKER')
                        ? 'Pivot execution strategy, verify parameters, or modify tool arguments.'
                        : undefined;
                    const suggested_remediation = direct_hint || code_hint || circuit_hint;

                    return (
                        <div
                            key={entry.id}
                            onClick={() => set_focused_index(idx)}
                            data-focused={is_focused ? 'true' : 'false'}
                            className={`p-4 flex flex-col md:flex-row gap-4 items-start md:items-center justify-between animate-in fade-in slide-in-from-top-2 transition-colors ${
                                is_parent_directive ? 'bg-cyan-950/15 border-l-4 border-l-cyan-500' : ''
                            } ${is_focused ? 'ring-1 ring-inset ring-emerald-500/50 bg-zinc-800/30' : ''}`}
                        >
                            <div className="space-y-1">
                                <div className="flex items-center gap-2 flex-wrap">
                                    <span
                                        data-testid={`risk-badge-${entry.id}`}
                                        className={`text-[10px] font-mono font-bold uppercase tracking-widest px-2 py-0.5 rounded-md border ${risk_badge_cls}`}
                                    >
                                        {risk_tier} risk
                                    </span>
                                    <span className="text-xs font-mono bg-zinc-800 text-zinc-300 px-2 py-0.5 rounded">
                                        {entry.tool_call?.agent_id || entry.agent_id || i18n.t('oversight.unknown_agent')}
                                    </span>
                                    <span className="text-sm font-medium text-green-400 flex items-center gap-1">
                                        <TerminalIcon className="w-3 h-3" />
                                        {skill_name || i18n.t('oversight.capability_proposal')}
                                    </span>
                                    {is_parent_directive && (
                                        <span
                                            data-testid="parent-directive-item-badge"
                                            className="text-xs font-semibold bg-cyan-500/20 text-cyan-300 border border-cyan-500/40 px-2 py-0.5 rounded flex items-center gap-1"
                                        >
                                            <Network className="w-3 h-3 text-cyan-400" />
                                            Parent Delegation Directive
                                        </span>
                                    )}
                                    <span className="text-xs text-zinc-500">
                                        {get_safe_date(entry)?.toLocaleTimeString() || '--:--:--'}
                                    </span>
                                </div>
                                <p className="text-zinc-300">
                                    {entry.tool_call?.description || entry.description || i18n.t('oversight.awaiting_authorization')}
                                </p>
                                <pre className="text-xs bg-zinc-950/70 p-2 rounded text-zinc-400 font-mono overflow-auto max-w-2xl">
                                    {JSON.stringify(params_obj, null, 2)}
                                </pre>
                                {suggested_remediation && (
                                    <div className="pt-2 flex items-center gap-1.5 flex-wrap">
                                        <span className="text-[11px] text-zinc-400 font-medium">Suggested Remediation:</span>
                                        <button
                                            type="button"
                                            data-testid={`suggested-remediation-btn-${entry.id}`}
                                            onClick={() => set_guidance_map(prev => ({ ...prev, [entry.id]: suggested_remediation }))}
                                            className="text-[11px] bg-cyan-950/60 hover:bg-cyan-900/80 text-cyan-300 border border-cyan-500/30 hover:border-cyan-400/60 rounded px-2 py-0.5 transition-colors text-left flex items-center gap-1 cursor-pointer"
                                            title="Click to copy suggested remediation into guidance input"
                                        >
                                            <Sparkles className="w-3 h-3 text-cyan-400 shrink-0" />
                                            <span>{suggested_remediation}</span>
                                        </button>
                                    </div>
                                )}
                                <div className="pt-2">
                                    <input
                                        type="text"
                                        data-testid={`oversight-guidance-input-${entry.id}`}
                                        placeholder="Optional operator guidance / instructions for agent..."
                                        value={guidance_map[entry.id] || ''}
                                        onChange={(e) => set_guidance_map(prev => ({ ...prev, [entry.id]: e.target.value }))}
                                        className="w-full text-xs bg-zinc-950/70 border border-zinc-800 rounded px-2.5 py-1.5 text-zinc-200 placeholder-zinc-500 focus:outline-none focus:border-cyan-500/70 transition-colors"
                                    />
                                </div>
                            </div>
                            <div className="flex gap-2 w-full md:w-auto">
                                <Tooltip content={i18n.t('oversight.approve_action_tooltip')} position="top">
                                    <button
                                        type="button"
                                        onClick={() => void on_decide(entry.id, 'approved')}
                                        className="flex-1 md:flex-none bg-green-500/20 hover:bg-green-500/30 text-green-400 px-4 py-2 rounded border border-green-500/30 font-medium transition-colors"
                                    >
                                        {i18n.t('oversight.approve_button')}
                                    </button>
                                </Tooltip>
                                <Tooltip content={i18n.t('oversight.reject_action_tooltip')} position="top">
                                    <button
                                        type="button"
                                        onClick={() => void on_decide(entry.id, 'rejected')}
                                        className="flex-1 md:flex-none bg-red-500/20 hover:bg-red-500/30 text-red-400 px-4 py-2 rounded border border-red-500/30 font-medium transition-colors"
                                    >
                                        {i18n.t('oversight.reject_button')}
                                    </button>
                                </Tooltip>
                            </div>
                        </div>
                    );
                })}
            </div>
        </div>
    );
};

// Metadata: [Pending_Approval_Queue]

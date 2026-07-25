/**
 * @docs ARCHITECTURE:UI-Components
 * 
 * ### AI Assist Note
 * **Auto_Approved_Panel Component**
 * Read-only observability panel displaying system & agent auto-approved actions.
 * Follows design.md Neural Glass aesthetic tokens (Zinc-900 surface, 12px rounded, 1px border, backdrop-blur).
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Missing or malformed auto_approved entries in ledger payload.
 * - **Telemetry Link**: Search `[Auto_Approved_Panel]` in observability traces.
 */

import React, { useMemo } from 'react';
import { Zap, ShieldCheck, Clock, CheckCircle } from 'lucide-react';
import type { LedgerEntry } from '../../data/mock_oversight';
import { Tw_Empty_State } from '../ui';
import { get_safe_date } from '../../utils/date_utils';
import { check_is_auto, get_entry_time } from '../../utils/oversight_utils';

interface AutoApprovedPanelProps {
    ledger: LedgerEntry[];
    resolve_agent_name: (id: string) => string;
}

export const Auto_Approved_Panel: React.FC<AutoApprovedPanelProps> = ({
    ledger,
    resolve_agent_name
}) => {
    // Filter strictly for auto-approved actions using DRY helper
    const auto_entries = useMemo(() => {
        return ledger.filter(check_is_auto)
            .sort((a, b) => get_entry_time(b) - get_entry_time(a));
    }, [ledger]);

    return (
        <div className="bg-zinc-900/60 backdrop-blur-xl border border-zinc-800/60 rounded-xl overflow-hidden sovereign-transition">
            {/* Header Bar */}
            <div className="p-4 border-b border-zinc-800/80 flex items-center justify-between bg-zinc-950/40">
                <div className="flex items-center gap-3">
                    <div className="p-1.5 bg-cyan-500/10 rounded-lg border border-cyan-500/20">
                        <Zap className="w-4 h-4 text-cyan-400" />
                    </div>
                    <div>
                        <div className="flex items-center gap-2">
                            <h2 className="font-bold text-sm text-zinc-100 uppercase tracking-wide font-mono">
                                Auto-Approved Observability Panel
                            </h2>
                            <span className="px-2 py-0.5 rounded-full text-[10px] font-mono font-bold bg-cyan-500/15 text-cyan-300 border border-cyan-500/30">
                                {auto_entries.length} READ-ONLY
                            </span>
                        </div>
                        <p className="text-[11px] text-zinc-400 font-mono mt-0.5">
                            Real-time stream of safe diagnostic & policy auto-approved agent actions
                        </p>
                    </div>
                </div>

                <div className="flex items-center gap-2 text-[10px] font-mono text-zinc-500 bg-black/40 px-3 py-1.5 rounded-lg border border-zinc-800">
                    <ShieldCheck className="w-3.5 h-3.5 text-cyan-400" />
                    <span>POLICY: SAFE_SKILLS_PASS_THROUGH</span>
                </div>
            </div>

            {/* List / Table Area */}
            <div className="max-h-[280px] overflow-auto custom-scrollbar p-0">
                {auto_entries.length > 0 ? (
                    <div className="divide-y divide-zinc-800/40">
                        {auto_entries.map(entry => {
                            const agent_id = entry.tool_call?.agent_id || entry.agent_id || '';
                            const agent_name = resolve_agent_name(agent_id);
                            const skill_name = entry.tool_call?.skill || entry.skill || 'Diagnostic Action';
                            const timestamp = get_safe_date(entry, new Date())?.toLocaleTimeString() || '--:--:--';
                            const duration_ms = entry.result?.duration_ms;

                            return (
                                <div key={entry.id} className="p-3.5 hover:bg-zinc-800/30 transition-colors flex items-center justify-between gap-4">
                                    <div className="flex items-center gap-3 min-w-0">
                                        <div className="w-7 h-7 rounded-lg bg-cyan-500/10 border border-cyan-500/20 flex items-center justify-center text-xs font-mono font-bold text-cyan-300 shrink-0">
                                            {agent_name.charAt(0)}
                                        </div>
                                        <div className="min-w-0">
                                            <div className="flex items-center gap-2 flex-wrap">
                                                <span className="text-xs font-bold text-zinc-200">
                                                    {agent_name}
                                                </span>
                                                <span className="text-[9px] font-mono font-bold text-cyan-400 bg-cyan-500/10 px-1.5 py-0.5 rounded border border-cyan-500/20 uppercase">
                                                    {skill_name}
                                                </span>
                                                <span className="text-[9px] font-mono text-emerald-400 bg-emerald-500/10 px-1.5 py-0.5 rounded border border-emerald-500/20 flex items-center gap-1">
                                                    <CheckCircle size={10} />
                                                    AUTO-APPROVED
                                                </span>
                                            </div>
                                            <p className="text-[11px] text-zinc-400 font-mono truncate mt-0.5">
                                                {entry.tool_call?.description || JSON.stringify(entry.tool_call?.params || entry.params || {})}
                                            </p>
                                        </div>
                                    </div>

                                    <div className="flex items-center gap-3 shrink-0 text-right font-mono text-[10px] text-zinc-500">
                                        {duration_ms !== undefined && (
                                            <span className="text-zinc-400 bg-black/40 px-2 py-0.5 rounded border border-zinc-800">
                                                {duration_ms}ms
                                            </span>
                                        )}
                                        <span className="flex items-center gap-1 text-zinc-400">
                                            <Clock size={11} />
                                            {timestamp}
                                        </span>
                                    </div>
                                </div>
                            );
                        })}
                    </div>
                ) : (
                    <div className="p-8 text-center">
                        <Tw_Empty_State
                            title="No Auto-Approved Actions"
                            description="All high-scrutiny agent actions are currently routed through human-in-the-loop oversight."
                        />
                    </div>
                )}
            </div>
        </div>
    );
};

// Metadata: [Auto_Approved_Panel]

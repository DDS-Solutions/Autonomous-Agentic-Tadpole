/**
 * @docs ARCHITECTURE:Interface
 * @docs design:Components
 *
 * ### AI Assist Note
 * **Overlord Readiness Card**: High-density Neural Glass diagnostic panel that executes
 * the native Rust `POST /v1/benchmarks/readiness` 4-probe check (SQLite WAL/FK, Model Tier,
 * Security Guardrails, Fleet Governance) and displays actionable remediation guidance.
 *
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Unauthenticated 401 or engine offline; falls back cleanly to error state without fake metrics.
 * - **Telemetry Link**: Search `[Readiness_Card]` in browser console logs.
 */

import React, { useState } from 'react';
import { Activity, AlertTriangle, CheckCircle2, Loader2, ShieldCheck, XCircle } from 'lucide-react';
import { tadpole_os_service } from '../../services/tadpoleos_service';
import type { Readiness_Report } from '../../services/system_api_service';
import { Tooltip } from '../ui';

interface Readiness_Card_Props {
    compact?: boolean;
    on_complete?: (report: Readiness_Report) => void;
}

export const Readiness_Card: React.FC<Readiness_Card_Props> = ({ compact = false, on_complete }) => {
    const [report, set_report] = useState<Readiness_Report | null>(null);
    const [is_running, set_is_running] = useState(false);
    const [error, set_error] = useState<string | null>(null);

    const handle_run_probe = async () => {
        set_is_running(true);
        set_error(null);
        try {
            const res = await tadpole_os_service.run_readiness_probe();
            set_report(res);
            on_complete?.(res);
            console.debug('[Readiness_Card] Probe completed with score:', res.score);
        } catch (err) {
            const msg = err instanceof Error ? err.message : 'Unable to reach readiness endpoint.';
            set_error(msg);
            console.warn('[Readiness_Card] Probe failed:', msg);
        } finally {
            set_is_running(false);
        }
    };

    const verdict_styles = (verdict: Readiness_Report['verdict']) => {
        if (verdict === 'ready') return 'bg-emerald-500/10 text-emerald-400 border-emerald-500/30';
        if (verdict === 'degraded') return 'bg-amber-500/10 text-amber-400 border-amber-500/30';
        return 'bg-rose-500/10 text-rose-400 border-rose-500/30';
    };

    const status_icon = (status: 'pass' | 'warn' | 'fail') => {
        if (status === 'pass') return <CheckCircle2 size={14} className="text-[#22c55e] shrink-0" />;
        if (status === 'warn') return <AlertTriangle size={14} className="text-amber-400 shrink-0" />;
        return <XCircle size={14} className="text-rose-400 shrink-0" />;
    };

    return (
        <section
            aria-label="Overlord Fleet Readiness Probe"
            className="rounded-xl border border-zinc-800/80 hover:border-zinc-700/80 transition-colors backdrop-blur-md p-4 space-y-4"
            style={{ backgroundColor: 'color-mix(in srgb, #18181b 65%, transparent)' }}
        >
            <div className="flex flex-wrap items-center justify-between gap-3">
                <div className="flex items-center gap-3">
                    <div className="w-9 h-9 rounded-lg bg-zinc-900 border border-zinc-800 flex items-center justify-center">
                        <ShieldCheck size={18} className="text-[#22c55e]" />
                    </div>
                    <div>
                        <p className="text-[10px] font-mono font-bold uppercase tracking-widest text-zinc-500">
                            PRE-FLIGHT VERIFICATION
                        </p>
                        <h3 className="text-sm font-semibold text-zinc-100">
                            Overlord Readiness Check
                        </h3>
                    </div>
                </div>

                <div className="flex items-center gap-3">
                    {report && (
                        <div className={`px-2.5 py-1 rounded-lg border font-mono text-[10px] font-bold uppercase tracking-widest ${verdict_styles(report.verdict)}`}>
                            {report.score}/100 · {report.verdict.replace('_', ' ')}
                        </div>
                    )}
                    <Tooltip content="Run native 4-probe engine check (DB, Providers, Guardrails, Fleet)" position="top">
                        <button
                            type="button"
                            onClick={() => void handle_run_probe()}
                            disabled={is_running}
                            title="Run Overlord Readiness Check"
                            aria-label="Run Overlord Readiness Check"
                            className="inline-flex items-center gap-2 px-3 py-1.5 rounded-lg bg-zinc-800 hover:bg-zinc-700 text-zinc-100 border border-zinc-700/60 text-xs font-mono font-bold uppercase tracking-wider transition-colors focus:outline-none focus:ring-2 focus:ring-emerald-500 disabled:opacity-50"
                        >
                            {is_running ? <Loader2 size={14} className="animate-spin" /> : <Activity size={14} className="text-[#22c55e]" />}
                            {is_running ? 'Probing...' : 'Run Readiness Check'}
                        </button>
                    </Tooltip>
                </div>
            </div>

            {error && (
                <div className="rounded-lg border border-rose-500/30 bg-rose-500/10 px-3 py-2 text-xs font-mono text-rose-300">
                    Readiness check unavailable: {error}
                </div>
            )}

            {report && (
                <div className="space-y-3">
                    <div className="flex flex-wrap items-center gap-3 text-[10px] font-mono uppercase tracking-widest text-zinc-400">
                        <span className="px-2 py-0.5 rounded-md bg-zinc-900 border border-zinc-800 text-zinc-300">
                            TIER: {report.detected_tier.replace('_', ' ')}
                        </span>
                        <span className="px-2 py-0.5 rounded-md bg-zinc-900 border border-zinc-800 text-zinc-300">
                            REC DEPTH: ≤ {report.recommended_max_depth}
                        </span>
                        <span className="px-2 py-0.5 rounded-md bg-zinc-900 border border-zinc-800 text-zinc-400 tabular-nums">
                            {report.total_latency_ms.toFixed(1)} ms
                        </span>
                    </div>

                    <div className={compact ? 'grid grid-cols-1 sm:grid-cols-2 gap-2' : 'grid grid-cols-1 md:grid-cols-2 gap-3'}>
                        {report.checks.map((chk) => (
                            <div
                                key={chk.id}
                                className="rounded-lg bg-zinc-950/70 border border-zinc-800/70 p-3 space-y-1.5"
                            >
                                <div className="flex items-center justify-between gap-2">
                                    <div className="flex items-center gap-2 min-w-0">
                                        {status_icon(chk.status)}
                                        <span className="text-xs font-semibold text-zinc-200 truncate">{chk.label}</span>
                                    </div>
                                    <span className="text-[10px] font-mono text-zinc-500 tabular-nums shrink-0">
                                        {chk.latency_ms.toFixed(1)}ms
                                    </span>
                                </div>
                                <p className="text-xs text-zinc-400 leading-relaxed">{chk.detail}</p>
                                {chk.remediation && (
                                    <p className="text-[11px] font-mono text-amber-300/90 bg-amber-500/5 border border-amber-500/20 rounded-md px-2 py-1">
                                        Action: {chk.remediation}
                                    </p>
                                )}
                            </div>
                        ))}
                    </div>
                </div>
            )}
        </section>
    );
};

// Metadata: [Readiness_Card]

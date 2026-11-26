/**
 * @docs ARCHITECTURE:Interface
 *
 * ### AI Assist Note
 * **Oversight Stats & Emergency Controls Bar**: Renders connection/demo status banners,
 * cluster-scoped oversight KPI counters, emergency kill-switch triggers, and security hub navigation.
 *
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Kill-switch buttons disabled unexpectedly or stale offline banner state.
 * - **Telemetry Link**: Search `[Oversight_Stats_Bar]` in UI logs.
 */

import React from 'react';
import {
    Shield,
    CheckCircle,
    XCircle,
    Clock,
    WifiOff,
    ShieldCheck,
    KeyRound,
} from 'lucide-react';
import { Tooltip } from '../ui';
import { i18n } from '../../i18n';

export interface Oversight_Stats_Bar_Props {
    stats: { pending: number; approved: number; rejected: number };
    is_online: boolean;
    is_simulated: boolean;
    connection_error: string | null;
    on_retry_connection: () => void;
    on_enable_demo_preview: () => void;
    on_exit_demo_preview: () => void;
    on_halt_agents: () => void;
    on_shutdown_engine: () => void;
    on_open_security: () => void;
}

export const Oversight_Stats_Bar: React.FC<Oversight_Stats_Bar_Props> = ({
    stats,
    is_online,
    is_simulated,
    connection_error,
    on_retry_connection,
    on_enable_demo_preview,
    on_exit_demo_preview,
    on_halt_agents,
    on_shutdown_engine,
    on_open_security,
}) => {
    return (
        <>
            {/* Honest Connection / Auth Banner (No Silent Fake Data Injection) */}
            {connection_error && !is_simulated && (
                <div
                    data-testid="oversight-connection-banner"
                    className="bg-zinc-900/90 border border-zinc-800 backdrop-blur-md p-4 rounded-xl flex flex-wrap items-center justify-between gap-3"
                >
                    <div className="flex items-center gap-3">
                        <div className="p-2 bg-zinc-950 border border-zinc-800 rounded-lg">
                            <KeyRound size={16} className="text-amber-400" />
                        </div>
                        <div>
                            <p className="text-xs font-mono font-bold text-zinc-100 uppercase tracking-widest">
                                Live Oversight Disconnected
                            </p>
                            <p className="text-xs text-zinc-400 font-mono">
                                {connection_error} — Enter your session token or verify the engine is running.
                            </p>
                        </div>
                    </div>
                    <div className="flex items-center gap-2">
                        <button
                            type="button"
                            onClick={() => window.dispatchEvent(new CustomEvent('app:open-setup-wizard'))}
                            className="text-[10px] font-mono font-bold uppercase tracking-wider px-3 py-1.5 rounded-lg bg-zinc-100 hover:bg-white text-zinc-950 focus:outline-none focus:ring-2 focus:ring-emerald-500"
                        >
                            Enter Session Token
                        </button>
                        <button
                            type="button"
                            onClick={on_retry_connection}
                            className="text-[10px] font-mono font-bold uppercase tracking-wider px-3 py-1.5 rounded-lg bg-zinc-800 hover:bg-zinc-700 text-zinc-200 border border-zinc-700 focus:outline-none focus:ring-2 focus:ring-emerald-500"
                        >
                            {i18n.t('oversight.retry_connection')}
                        </button>
                        <button
                            type="button"
                            onClick={on_enable_demo_preview}
                            className="text-[10px] font-mono font-bold uppercase tracking-wider px-3 py-1.5 rounded-lg bg-zinc-950 hover:bg-zinc-800 text-zinc-400 hover:text-zinc-200 border border-zinc-800 focus:outline-none focus:ring-2 focus:ring-emerald-500"
                        >
                            Preview Demo Data
                        </button>
                    </div>
                </div>
            )}

            {/* Explicit Opt-In Simulation Banner */}
            {is_simulated && (
                <div className="bg-amber-500/10 border border-amber-500/30 p-3 rounded-xl flex items-center justify-between animate-in fade-in slide-in-from-top-4">
                    <div className="flex items-center gap-3">
                        <div className="p-1.5 bg-amber-500/20 rounded-lg">
                            <WifiOff size={16} className="text-amber-500" />
                        </div>
                        <div>
                            <p className="text-xs font-bold text-amber-200 uppercase tracking-widest">{i18n.t('oversight.disconnected_title')}</p>
                            <p className="text-[10px] text-amber-500/70 font-mono">{i18n.t('oversight.disconnected_subtitle')}</p>
                        </div>
                    </div>
                    <button
                        type="button"
                        onClick={on_exit_demo_preview}
                        className="text-[10px] px-3 py-1 bg-amber-500/20 hover:bg-amber-500/30 text-amber-400 rounded-lg border border-amber-500/30 transition-colors uppercase font-bold tracking-tighter"
                    >
                        {i18n.t('oversight.retry_connection')}
                    </button>
                </div>
            )}

            {/* Header / Stats */}
            <div className="grid grid-cols-1 md:grid-cols-5 gap-4">
                <Tooltip content={i18n.t('oversight.pending_actions_tooltip')} position="top" class_name="w-full">
                    <div className="bg-zinc-900 border border-zinc-800 p-4 rounded-lg flex items-center justify-between cursor-help w-full">
                        <div>
                            <p className="text-zinc-400 text-sm">{i18n.t('oversight.pending_actions_label')}</p>
                            <p className="text-2xl font-bold text-yellow-400">{stats.pending}</p>
                        </div>
                        <Clock className="w-8 h-8 text-yellow-500/20" />
                    </div>
                </Tooltip>
                <Tooltip content={i18n.t('oversight.approved_tooltip')} position="top" class_name="w-full">
                    <div className="bg-zinc-900 border border-zinc-800 p-4 rounded-lg flex items-center justify-between cursor-help w-full">
                        <div>
                            <p className="text-zinc-400 text-sm">{i18n.t('oversight.approved_label')}</p>
                            <p className="text-2xl font-bold text-green-400">{stats.approved}</p>
                        </div>
                        <CheckCircle className="w-8 h-8 text-green-500/20" />
                    </div>
                </Tooltip>
                <Tooltip content={i18n.t('oversight.rejected_tooltip')} position="top" class_name="w-full">
                    <div className="bg-zinc-900 border border-zinc-800 p-4 rounded-lg flex items-center justify-between cursor-help w-full">
                        <div>
                            <p className="text-zinc-400 text-sm">{i18n.t('oversight.rejected_label')}</p>
                            <p className="text-2xl font-bold text-red-400">{stats.rejected}</p>
                        </div>
                        <XCircle className="w-8 h-8 text-red-500/20" />
                    </div>
                </Tooltip>
                <Tooltip content={i18n.t('oversight.halt_agents_tooltip')} position="top" class_name="w-full">
                    <button
                        type="button"
                        onClick={on_halt_agents}
                        className={`p-4 rounded-lg flex items-center justify-center gap-2 font-bold transition-colors cursor-pointer group border w-full ${is_online ? 'bg-amber-500/10 hover:bg-amber-500/20 border-amber-500/50 text-amber-400' : 'bg-zinc-800/10 border-zinc-700/50 text-zinc-600 opacity-50'}`}
                        disabled={!is_online}
                    >
                        <Shield className="w-5 h-5 group-hover:scale-110 transition-transform" />
                        {is_online ? i18n.t('oversight.halt_agents_button') : i18n.t('oversight.offline_label')}
                    </button>
                </Tooltip>
                <Tooltip content={i18n.t('oversight.kill_engine_tooltip')} position="top" class_name="w-full">
                    <button
                        type="button"
                        onClick={on_shutdown_engine}
                        className={`p-4 rounded-lg flex items-center justify-center gap-2 font-bold transition-colors cursor-pointer group border w-full ${is_online ? 'bg-red-600/10 hover:bg-red-600/20 border-red-600/50 text-red-500' : 'bg-zinc-800/10 border-zinc-700/50 text-zinc-600 opacity-50'}`}
                        disabled={!is_online}
                    >
                        <WifiOff className="w-5 h-5 group-hover:scale-110 transition-transform" />
                        {is_online ? i18n.t('oversight.kill_engine_button') : i18n.t('oversight.offline_label')}
                    </button>
                </Tooltip>

                {/* Row 2 (Partial) */}
                <div className="md:col-start-4 md:col-span-2">
                    <Tooltip content={i18n.t('oversight.security_dashboard_tooltip')} position="top" class_name="w-full">
                        <button
                            type="button"
                            onClick={on_open_security}
                            className="w-full p-4 bg-zinc-950 border border-zinc-700 hover:border-green-500/50 hover:bg-green-500/5 text-zinc-300 hover:text-green-400 rounded-lg flex items-center justify-center gap-2 font-bold transition-all group"
                        >
                            <ShieldCheck className="w-5 h-5 group-hover:scale-110 transition-transform text-green-500" />
                            {i18n.t('oversight.security_dashboard_button')}
                        </button>
                    </Tooltip>
                </div>
            </div>
        </>
    );
};

// Metadata: [Oversight_Stats_Bar]

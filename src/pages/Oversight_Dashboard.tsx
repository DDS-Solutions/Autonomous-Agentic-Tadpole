/* eslint-disable react-hooks/preserve-manual-memoization */
/**
 * @docs ARCHITECTURE:Interface
 * 
 * ### AI Assist Note
 * **Root View**: High-level governance and oversight dashboard. 
 * Orchestrates the visualization of swarm-wide compliance, safety protocols, and operational constraints.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Oversight policy violation alert noise, or dashboard lockup during massive telemetry bursts.
 * - **Telemetry Link**: Search for `[Oversight_Dashboard]` or `GOVERNANCE_SYNC` in service logs.
 */

import { useEffect, useState, useMemo, useCallback } from 'react';
import {
    Shield,
    CheckCircle,
    XCircle,
    Clock,
    AlertTriangle,
    Activity,
    Terminal as TerminalIcon,
    Cpu,
    Plus,
    WifiOff,
    ShieldCheck,
    Network,
    Sparkles,
    CheckCheck,
    KeyRound
} from 'lucide-react';
import { useNavigate, useLocation } from 'react-router-dom';
import { Tw_Empty_State, Tooltip, Confirm_Dialog } from '../components/ui';
import { useEngineStatus } from '../hooks/use_engine_status';
import { useErrorRemediation } from '../hooks/useErrorRemediation';
import { use_workspace_store } from '../stores/workspace_store';
import { use_agent_store } from '../stores/agent_store';
import { use_notification_store } from '../stores/notification_store';
import type { Agent } from '../contracts/agent/domain';
import { tadpole_os_service } from '../services/tadpoleos_service';
import { tadpole_os_socket } from '../services/socket';
import { Command_Table } from '../components/Command_Table';
import { Auto_Approved_Panel } from '../components/oversight/Auto_Approved_Panel';
import { Action_Ledger } from '../components/oversight/Action_Ledger';
import { i18n } from '../i18n';
import { LD_Json } from '../components/ui/LD_Json';
import { get_safe_date } from '../utils/date_utils';
import { is_parent_delegation_directive, classify_skill_risk } from '../utils/oversight_utils';
import type { OversightEntry, LedgerEntry } from '../data/mock_oversight';
import { MOCK_PENDING, MOCK_LEDGER } from '../data/mock_oversight';

export default function Oversight_Dashboard() {
    const navigate = useNavigate();
    const location = useLocation();
    const is_oversight_route_active = location.pathname.replace(/\/+$/, '') === '/oversight';
    const { is_online } = useEngineStatus();
    const [pending, set_pending] = useState<OversightEntry[]>(() => {
        const saved = localStorage.getItem('tadpole_oversight_pending');
        if (saved) {
            try { return JSON.parse(saved); } catch { return []; }
        }
        return [];
    });
    const [ledger, set_ledger] = useState<LedgerEntry[]>(() => {
        const saved = localStorage.getItem('tadpole_oversight_ledger');
        if (saved) {
            try { return JSON.parse(saved); } catch { return []; }
        }
        return [];
    });
    const [filter, set_filter] = useState('');
    const [is_simulated, set_is_simulated] = useState(false);
    const [connection_error, set_connection_error] = useState<string | null>(null);
    const [has_attempted_fetch, set_has_attempted_fetch] = useState(false);
    const [selected_cluster_id, set_selected_cluster_id] = useState<string>('all');
    const [guidance_map, set_guidance_map] = useState<Record<string, string>>({});
    const [focused_index, set_focused_index] = useState<number>(0);
    const [confirm_action, set_confirm_action] = useState<'halt_agents' | 'shutdown_engine' | 'batch_approve' | null>(null);
    const [is_batch_running, set_is_batch_running] = useState(false);
    const { get_remediation } = useErrorRemediation();
    const { clusters, active_proposals, apply_proposal, dismiss_proposal } = use_workspace_store();
    const { agents: all_agents, update_agent: store_update_agent } = use_agent_store();

    // Persistence: Save to localStorage on change
    useEffect(() => {
        if (pending.length > 0 || has_attempted_fetch) {
            localStorage.setItem('tadpole_oversight_pending', JSON.stringify(pending));
        }
    }, [pending, has_attempted_fetch]);

    useEffect(() => {
        if (ledger.length > 0 || has_attempted_fetch) {
            localStorage.setItem('tadpole_oversight_ledger', JSON.stringify(ledger));
        }
    }, [ledger, has_attempted_fetch]);

    const fetch_data = useCallback(async () => {
        if (is_simulated) return;

        try {
            const [pending_data, ledger_data] = await Promise.all([
                tadpole_os_service.get_pending_oversight(),
                tadpole_os_service.get_oversight_ledger()
            ]);

            set_pending(pending_data as OversightEntry[]);
            set_ledger(ledger_data as LedgerEntry[]);
            set_is_simulated(false);
            set_connection_error(null);
            set_has_attempted_fetch(true);
        } catch (err) {
            const msg = err instanceof Error ? err.message : 'Unable to reach engine oversight API.';
            set_connection_error(msg);
            set_has_attempted_fetch(true);
        }
    }, [is_simulated]);

    // Live WebSocket push subscription + 15s fallback poll when visible
    useEffect(() => {
        void Promise.resolve().then(() => fetch_data());
        const unsub = tadpole_os_socket.subscribe_custom_event((ev) => {
            const kind = String(ev.type || ev.event_type || '').toLowerCase();
            if (
                kind.includes('oversight') ||
                kind.includes('approval') ||
                kind.includes('kill') ||
                kind.includes('flagged_injection') ||
                kind.includes('circuit_breaker')
            ) {
                void fetch_data();
            }
        });
        const interval = setInterval(() => {
            if (document.visibilityState === 'visible') void fetch_data();
        }, 15000);
        return () => {
            unsub();
            clearInterval(interval);
        };
    }, [fetch_data]);

    // Use useMemo for stats to avoid cascading renders
    // Reactive stats that respect the current cluster filter
    const active_collaborators = useMemo(() => {
        return new Set(clusters.find(c => c.id === selected_cluster_id)?.collaborators || []);
    }, [clusters, selected_cluster_id]);

    const stats = useMemo(() => {
        const base_ledger = selected_cluster_id === 'all' 
            ? ledger 
            : ledger.filter(l => {
                const tc = l.tool_call || l;
                return tc.cluster_id === selected_cluster_id || 
                       active_collaborators.has(tc.agent_id || '');
            });
            
        const base_pending = selected_cluster_id === 'all'
            ? pending
            : pending.filter(p => {
                const tc = p.tool_call || p;
                return tc.cluster_id === selected_cluster_id ||
                       active_collaborators.has(tc.agent_id || '');
            });

        return {
            pending: base_pending.length,
            approved: base_ledger.filter((entry) => entry.decision === 'approved').length,
            rejected: base_ledger.filter((entry) => entry.decision === 'rejected').length
        };
    }, [pending, ledger, selected_cluster_id, active_collaborators]);

    const handle_decide = useCallback(async (id: string, decision: 'approved' | 'rejected', answer?: string): Promise<boolean> => {
        const final_answer = answer !== undefined ? answer : guidance_map[id];
        if (is_simulated) {
            const entry = pending.find(p => p.id === id);
            if (entry) {
                set_ledger(prev => [{ ...entry, decision, timestamp: new Date().toISOString() } as LedgerEntry, ...prev]);
                set_pending(prev => prev.filter(p => p.id !== id));
            }
            return true;
        }

        try {
            if (final_answer) {
                await tadpole_os_service.decide_oversight(id, decision, final_answer);
            } else {
                await tadpole_os_service.decide_oversight(id, decision);
            }
            set_pending(prev => prev.filter(p => p.id !== id));
            set_guidance_map(prev => {
                const next = { ...prev };
                delete next[id];
                return next;
            });
            return true;
        } catch (err) {
            use_notification_store.getState().add_notification({
                severity: 'error',
                title: 'Oversight Decision Failed',
                message: err instanceof Error ? err.message : 'Could not persist decision to engine.',
                duration: 6000,
                persistent: false,
            });
            return false;
        }
    }, [guidance_map, is_simulated, pending]);

    const handle_apply_proposal = async (cluster_id: string) => {
        const proposal = Object.values(active_proposals || {}).find(p => p.cluster_id === cluster_id);
        if (!proposal) return;

        // Commit all proposed changes to the live agent registry
        for (const change of (proposal.changes || [])) {
            const updates: Partial<Agent> = {};
            if (change.proposed_role) updates.role = change.proposed_role;
            if (change.proposed_model) {
                // Assuming primary slot update for swarm optimization
                updates.model = change.proposed_model;
            }
            if (change.added_skills) {
                const agent = all_agents.find(a => a.id === change.agent_id);
                if (agent) {
                    updates.skills = [...new Set([...(agent.skills || []), ...change.added_skills])];
                }
            }

            if (Object.keys(updates).length > 0) {
                await store_update_agent(change.agent_id, updates);
            }
        }

        apply_proposal(cluster_id);
    };

    const handle_kill_switch = () => set_confirm_action('halt_agents');
    const handle_kill_engine = () => set_confirm_action('shutdown_engine');

    const filtered_pending = useMemo(() => pending.filter(p => {
        const tool_call = p.tool_call || p;
        return selected_cluster_id === 'all'
            || tool_call.cluster_id === selected_cluster_id
            || active_collaborators.has(tool_call.agent_id || '');
    }), [pending, selected_cluster_id, active_collaborators]);

    const low_risk_pending = useMemo(
        () => filtered_pending.filter(p => classify_skill_risk(p.tool_call?.skill || p.skill, p.tool_call?.params ?? p.params) === 'low'),
        [filtered_pending]
    );

    const execute_batch_approve_low_risk = useCallback(async () => {
        if (is_batch_running || low_risk_pending.length === 0) return;
        set_is_batch_running(true);
        const batch_snapshot = [...low_risk_pending];
        let succeeded = 0;
        let failed = 0;
        try {
            for (const item of batch_snapshot) {
                const ok = await handle_decide(item.id, 'approved');
                if (ok) succeeded += 1;
                else failed += 1;
            }
            if (succeeded > 0 && failed === 0) {
                use_notification_store.getState().add_notification({
                    severity: 'success',
                    title: 'Batch Approval Complete',
                    message: `Approved ${succeeded} low-risk read-only action(s).`,
                    duration: 5000,
                    persistent: false,
                });
            } else if (succeeded > 0 && failed > 0) {
                use_notification_store.getState().add_notification({
                    severity: 'warning',
                    title: 'Batch Approval Partially Failed',
                    message: `Approved ${succeeded} action(s), but ${failed} failed. Inspect pending queue.`,
                    duration: 7000,
                    persistent: false,
                });
            } else {
                use_notification_store.getState().add_notification({
                    severity: 'error',
                    title: 'Batch Approval Failed',
                    message: `All ${failed} low-risk approval request(s) failed. No actions were approved.`,
                    duration: 7000,
                    persistent: false,
                });
            }
        } finally {
            set_is_batch_running(false);
        }
    }, [handle_decide, is_batch_running, low_risk_pending]);

    const handle_confirmed_action = async () => {
        const action = confirm_action;
        set_confirm_action(null);
        if (action === 'batch_approve') {
            await execute_batch_approve_low_risk();
            return;
        }
        if (action === 'halt_agents') {
            if (is_simulated) {
                set_pending([]);
                return;
            }
            try {
                await tadpole_os_service.kill_agents({ confirm: true });
                use_notification_store.getState().add_notification({
                    severity: 'warning',
                    title: 'Agents Halted',
                    message: i18n.t('oversight.agents_halted'),
                    duration: 6000,
                    persistent: false,
                });
            } catch {
                use_notification_store.getState().add_notification({
                    severity: 'error',
                    title: 'Halt Failed',
                    message: i18n.t('oversight.halt_agents_failed'),
                    duration: 6000,
                    persistent: false,
                });
            }
        } else if (action === 'shutdown_engine') {
            try {
                await tadpole_os_service.shutdown_engine({ confirm: true });
                use_notification_store.getState().add_notification({
                    severity: 'warning',
                    title: 'Engine Shutdown',
                    message: i18n.t('oversight.engine_shutting_down'),
                    duration: 8000,
                    persistent: false,
                });
            } catch (e: unknown) {
                use_notification_store.getState().add_notification({
                    severity: 'error',
                    title: 'Shutdown Failed',
                    message: i18n.t('oversight.kill_engine_failed', { error: e instanceof Error ? e.message : 'Unknown error' }),
                    duration: 8000,
                    persistent: false,
                });
            }
        }
    };

    // Keyboard-First Overlord Triage (ONLY active when /oversight is the visible route and no dialog is open)
    useEffect(() => {
        if (!is_oversight_route_active) return;
        const on_key_down = (e: KeyboardEvent) => {
            const target = e.target as HTMLElement | null;
            const tag = target?.tagName;
            if (
                tag === 'INPUT' ||
                tag === 'TEXTAREA' ||
                tag === 'SELECT' ||
                target?.isContentEditable ||
                e.altKey ||
                confirm_action ||
                is_batch_running ||
                document.querySelector('[role="dialog"]') !== null
            ) {
                return;
            }
            if (filtered_pending.length === 0) return;

            const safe_idx = Math.min(focused_index, filtered_pending.length - 1);
            const focused_item = filtered_pending[safe_idx];
            if (!focused_item) return;

            if (e.key === 'j' || e.key === 'ArrowDown') {
                e.preventDefault();
                set_focused_index((prev) => (prev + 1) % filtered_pending.length);
            } else if (e.key === 'k' || e.key === 'ArrowUp') {
                e.preventDefault();
                set_focused_index((prev) => (prev - 1 + filtered_pending.length) % filtered_pending.length);
            } else if (e.key === 'A' && e.shiftKey && low_risk_pending.length > 0) {
                e.preventDefault();
                set_confirm_action('batch_approve');
            } else if (e.key === 'a' && !e.shiftKey && !e.ctrlKey && !e.metaKey) {
                e.preventDefault();
                void handle_decide(focused_item.id, 'approved');
            } else if (e.key === 'r' && !e.ctrlKey && !e.metaKey) {
                e.preventDefault();
                void handle_decide(focused_item.id, 'rejected');
            } else if (e.key === 'g' && !e.ctrlKey && !e.metaKey) {
                e.preventDefault();
                const input = document.querySelector<HTMLInputElement>(`[data-testid="oversight-guidance-input-${focused_item.id}"]`);
                input?.focus();
            }
        };
        window.addEventListener('keydown', on_key_down);
        return () => window.removeEventListener('keydown', on_key_down);
    }, [confirm_action, filtered_pending, focused_index, handle_decide, is_batch_running, is_oversight_route_active, low_risk_pending.length]);

    const parent_delegation_count = useMemo(() => {
        return filtered_pending.filter(p => {
            const skill = p.tool_call?.skill || p.skill;
            return is_parent_delegation_directive(skill);
        }).length;
    }, [filtered_pending]);

    const enable_demo_preview = () => {
        set_pending(MOCK_PENDING);
        set_ledger(MOCK_LEDGER);
        set_is_simulated(true);
        set_connection_error(null);
    };

    return (
        <div className="p-6 space-y-6 max-w-7xl mx-auto">
            {/* GEO Optimization: Structured Data & Semantic Header */}
            <LD_Json data={{
              "@context": "https://schema.org",
              "@type": "SoftwareApplication",
              "name": "Tadpole OS Oversight Dashboard",
              "description": "High-level governance and oversight dashboard for swarm-wide compliance and safety protocols.",
              "provider": { "@type": "Organization", "name": "Sovereign Engineering" },
              "applicationCategory": "Governance System"
            }} />
            <h1 className="sr-only">Tadpole OS Oversight & Governance Command</h1>
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
                            onClick={() => void fetch_data()}
                            className="text-[10px] font-mono font-bold uppercase tracking-wider px-3 py-1.5 rounded-lg bg-zinc-800 hover:bg-zinc-700 text-zinc-200 border border-zinc-700 focus:outline-none focus:ring-2 focus:ring-emerald-500"
                        >
                            {i18n.t('oversight.retry_connection')}
                        </button>
                        <button
                            type="button"
                            onClick={enable_demo_preview}
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
                        onClick={() => {
                            set_is_simulated(false);
                            set_pending([]);
                            set_ledger([]);
                        }}
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
                        onClick={handle_kill_switch}
                        className={`p-4 rounded-lg flex items-center justify-center gap-2 font-bold transition-colors cursor-pointer group border w-full ${is_online ? 'bg-amber-500/10 hover:bg-amber-500/20 border-amber-500/50 text-amber-400' : 'bg-zinc-800/10 border-zinc-700/50 text-zinc-600 opacity-50'}`}
                        disabled={!is_online}
                    >
                        <Shield className="w-5 h-5 group-hover:scale-110 transition-transform" />
                        {is_online ? i18n.t('oversight.halt_agents_button') : i18n.t('oversight.offline_label')}
                    </button>
                </Tooltip>
                <Tooltip content={i18n.t('oversight.kill_engine_tooltip')} position="top" class_name="w-full">
                    <button
                        onClick={handle_kill_engine}
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
                            onClick={() => navigate('/security')}
                            className="w-full p-4 bg-zinc-950 border border-zinc-700 hover:border-green-500/50 hover:bg-green-500/5 text-zinc-300 hover:text-green-400 rounded-lg flex items-center justify-center gap-2 font-bold transition-all group"
                        >
                            <ShieldCheck className="w-5 h-5 group-hover:scale-110 transition-transform text-green-500" />
                            {i18n.t('oversight.security_dashboard_button')}
                        </button>
                    </Tooltip>
                </div>
            </div>

            {/* Pending Queue */}
            {filtered_pending.length > 0 && (
                <div className="bg-zinc-900 border border-yellow-500/30 rounded-lg overflow-hidden">
                    <div className="bg-yellow-500/10 p-3 border-b border-yellow-500/20 flex flex-wrap items-center justify-between gap-3">
                        <div className="flex items-center gap-2.5 flex-wrap">
                            <AlertTriangle className="w-4 h-4 text-yellow-500 shrink-0" />
                            <h2 className="font-semibold text-yellow-100">{i18n.t('oversight.awaiting_approval_title', { count: filtered_pending.length })}</h2>
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
                                <span>Parent Delegation Directives {parent_delegation_count > 0 ? `(${parent_delegation_count})` : ''} — Coordinating Complex Multi-Agent Flows</span>
                            </div>
                        </div>
                        <div className="flex items-center gap-2 flex-wrap">
                            <span className="text-[10px] font-mono text-zinc-400 bg-zinc-950/70 border border-zinc-800 px-2 py-0.5 rounded-md hidden sm:inline">
                                Keys: j/k focus · a approve · r reject · g guide
                            </span>
                            {low_risk_pending.length > 0 && (
                                <button
                                    type="button"
                                    data-testid="batch-approve-low-risk-btn"
                                    onClick={() => set_confirm_action('batch_approve')}
                                    title="Approve all low-risk read-only skills (mutating/shell actions are never batch-approved)"
                                    aria-label={`Approve ${low_risk_pending.length} low-risk actions`}
                                    className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-lg bg-emerald-500/15 hover:bg-emerald-500/25 border border-emerald-500/40 text-emerald-300 text-[10px] font-mono font-bold uppercase tracking-wider focus:outline-none focus:ring-2 focus:ring-emerald-500"
                                >
                                    <CheckCheck size={12} />
                                    Approve Low-Risk ({low_risk_pending.length})
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
                                                {entry.tool_call?.skill || entry.skill || i18n.t('oversight.capability_proposal')}
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
                                        <p className="text-zinc-300">{entry.tool_call?.description || entry.description || i18n.t('oversight.awaiting_authorization')}</p>
                                        <pre className="text-xs bg-black/50 p-2 rounded text-zinc-400 font-mono overflow-auto max-w-2xl">
                                            {JSON.stringify(entry.tool_call?.params || entry.params || {}, null, 2)}
                                        </pre>
                                        {(() => {
                                            const params_obj = (entry.tool_call?.params || entry.params || {}) as Record<string, unknown>;
                                            const direct_hint = typeof params_obj.remediation === 'string' ? params_obj.remediation : undefined;
                                            const code_hint = entry.error_code ? get_remediation({ error_code: entry.error_code, help_link: entry.help_link })?.label : undefined;
                                            const circuit_hint = (entry.tool_call?.skill || entry.skill || '').includes('CIRCUIT_BREAKER')
                                                ? 'Pivot execution strategy, verify parameters, or modify tool arguments.'
                                                : undefined;
                                            const suggested_remediation = direct_hint || code_hint || circuit_hint;

                                            return (
                                                <>
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
                                                </>
                                            );
                                        })()}
                                    </div>
                                    <div className="flex gap-2 w-full md:w-auto">
                                        <Tooltip content={i18n.t('oversight.approve_action_tooltip')} position="top">
                                            <button
                                                onClick={() => handle_decide(entry.id, 'approved')}
                                                className="flex-1 md:flex-none bg-green-500/20 hover:bg-green-500/30 text-green-400 px-4 py-2 rounded border border-green-500/30 font-medium transition-colors"
                                            >
                                                {i18n.t('oversight.approve_button')}
                                            </button>
                                        </Tooltip>
                                        <Tooltip content={i18n.t('oversight.reject_action_tooltip')} position="top">
                                            <button
                                                onClick={() => handle_decide(entry.id, 'rejected')}
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
            )}

            {/* Auto-Approved Observability Panel */}
            <Auto_Approved_Panel
                ledger={ledger}
                resolve_agent_name={(id) => {
                    const agent = all_agents.find(a => a.id === id);
                    return agent ? `${agent.name} (${id})` : (id || i18n.t('oversight.unknown_agent'));
                }}
            />

            {/* Action Ledger */}
            <Action_Ledger
                ledger={ledger}
                filter={filter}
                set_filter={set_filter}
                selected_cluster_id={selected_cluster_id}
                set_selected_cluster_id={set_selected_cluster_id}
                clusters={clusters || []}
                resolve_agent_name={(id) => {
                    const agent = all_agents.find(a => a.id === id);
                    return agent ? `${agent.name} (${id})` : (id || i18n.t('oversight.unknown_agent'));
                }}
            />

            {/* Swarm Intelligence / Alpha Reasoning (Option A Enhancement) */}
            <div className="bg-zinc-900 border border-green-500/30 rounded-lg overflow-hidden">
                <div className="bg-green-500/10 p-3 border-b border-green-500/20 flex items-center gap-2">
                    <Tooltip content={i18n.t('oversight.swarm_intel_tooltip')} position="right" class_name="w-full">
                        <Cpu className="w-4 h-4 text-green-400 cursor-help" />
                    </Tooltip>
                    <h2 className="font-semibold text-blue-100">{i18n.t('oversight.swarm_intel_title')}</h2>
                </div>
                <div className="p-6">
                    {Object.values(active_proposals || {}).length > 0 ? (
                        <div className="grid grid-cols-1 gap-6">
                            {Object.values(active_proposals || {}).map((proposal) => {
                                const cluster = (clusters || []).find(c => c.id === proposal.cluster_id);
                                return (
                                    <div key={proposal.cluster_id} className="bg-zinc-950 border border-zinc-800 rounded-xl p-4 space-y-4">
                                        <div className="flex justify-between items-center">
                                            <div className="flex items-center gap-2">
                                                <div className="w-2 h-2 rounded-full bg-green-500 animate-pulse" />
                                                <span className="text-xs font-bold text-zinc-300 uppercase tracking-widest">{cluster?.name || i18n.t('oversight.unknown_cluster')}</span>
                                            </div>
                                            <div className="flex items-center gap-2">
                                                <span className="text-[10px] text-zinc-600 font-mono">
                                                    {i18n.t('oversight.alpha_node_prefix', { alpha_id: cluster?.alpha_id ?? '?' })} • {get_safe_date(proposal)?.toLocaleTimeString() || '--:--:--'}
                                                </span>
                                                <div className="flex gap-2 ml-4">
                                                    <button
                                                        onClick={() => dismiss_proposal(proposal.cluster_id)}
                                                        className="text-[9px] px-2 py-0.5 bg-zinc-800 hover:bg-zinc-700 text-zinc-400 rounded border border-zinc-700 transition-colors uppercase font-bold"
                                                    >
                                                        {i18n.t('oversight.dismiss_button') || 'Dismiss'}
                                                    </button>
                                                    <button
                                                        onClick={() => handle_apply_proposal(proposal.cluster_id)}
                                                        className="text-[9px] px-2 py-0.5 bg-green-500/10 hover:bg-green-500/20 text-green-400 rounded border border-green-500/30 transition-colors uppercase font-bold"
                                                    >
                                                        {i18n.t('missions.btn_authorize')}
                                                    </button>
                                                </div>
                                            </div>
                                        </div>

                                        <div className="bg-black/40 p-3 rounded-lg border border-zinc-800/50">
                                            <div className="text-[9px] font-bold text-zinc-500 mb-2 uppercase tracking-wide flex items-center gap-2">
                                                <Activity size={10} /> {i18n.t('oversight.neural_trace_label')}
                                            </div>
                                            <p className="text-xs text-zinc-400 leading-relaxed font-mono whitespace-pre-wrap">
                                                {proposal.reasoning}
                                            </p>
                                        </div>

                                        <div className="space-y-2">
                                            <div className="text-[9px] font-bold text-zinc-500 uppercase tracking-wide">{i18n.t('oversight.proposed_reallocations_label')}</div>
                                            <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-2">
                                                {(proposal.changes || []).map(change => {
                                                    const agent = all_agents.find(a => a.id === change.agent_id);
                                                    return (
                                                        <div key={change.agent_id} className="p-2.5 bg-zinc-900 border border-zinc-800 rounded-lg flex flex-col gap-1.5">
                                                            <div className="flex justify-between items-start">
                                                                <span className="text-[10px] font-bold text-zinc-200">{agent?.name}</span>
                                                                <span className="text-[8px] px-1 rounded bg-zinc-800 text-zinc-500 font-mono uppercase">{i18n.t('oversight.mod_req_label')}</span>
                                                            </div>
                                                            <div className="space-y-1">
                                                                {change.proposed_role && (
                                                                    <div className="flex items-center gap-2 text-[9px]">
                                                                        <span className="text-zinc-600 uppercase">{i18n.t('oversight.role_label')}</span>
                                                                        <span className="text-green-400">{change.proposed_role}</span>
                                                                    </div>
                                                                )}
                                                                {change.proposed_model && (
                                                                    <div className="flex items-center gap-2 text-[9px]">
                                                                        <span className="text-zinc-600 uppercase">{i18n.t('oversight.model_label')}</span>
                                                                        <span className="text-green-400">{change.proposed_model}</span>
                                                                    </div>
                                                                )}
                                                                {change.added_skills && (
                                                                    <div className="flex items-center gap-2 text-[9px]">
                                                                        <span className="text-zinc-600 uppercase">{i18n.t('oversight.skills_label')}</span>
                                                                        <span className="text-emerald-400">+{change.added_skills.length}</span>
                                                                    </div>
                                                                )}
                                                            </div>
                                                        </div>
                                                    );
                                                })}
                                            </div>
                                        </div>
                                    </div>
                                );
                            })}
                        </div>
                    ) : (
                        <Tw_Empty_State
                            icon={<Plus size={32} />}
                            title={i18n.t('oversight.no_optimization_traces')}
                        />
                    )}
                </div>
            </div>

            {/* Neural Footprint Monitoring (Moved from Ops_Dashboard) */}
            <Command_Table />

            <Confirm_Dialog
                is_open={confirm_action !== null}
                title={
                    confirm_action === 'batch_approve'
                        ? 'Approve Low-Risk Actions'
                        : confirm_action === 'halt_agents'
                        ? 'Halt All Active Agents'
                        : 'Shutdown Sovereign Engine'
                }
                message={
                    confirm_action === 'batch_approve'
                        ? `Approve ${low_risk_pending.length} read-only / non-mutating action(s)? High- and medium-risk actions remain untouched.`
                        : confirm_action === 'halt_agents'
                        ? i18n.t('oversight.confirm_halt_agents')
                        : i18n.t('oversight.confirm_kill_engine')
                }
                variant={confirm_action === 'batch_approve' ? 'success' : 'danger'}
                confirm_label={confirm_action === 'batch_approve' ? `Approve ${low_risk_pending.length} Low-Risk` : 'Confirm'}
                on_confirm={() => void handle_confirmed_action()}
                on_cancel={() => set_confirm_action(null)}
            />
        </div>
    );
}


// Metadata: [Oversight_Dashboard]

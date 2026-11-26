/**
 * @docs ARCHITECTURE:Interface
 *
 * ### AI Assist Note
 * **Root View**: High-level governance and oversight dashboard container.
 * Coordinates live oversight polling/WebSocket sync, operator decisions, batch low-risk
 * approvals, kill-switch confirmations, and modular subpanels under `src/components/oversight/`.
 *
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Oversight policy violation alert noise, or dashboard lockup during massive telemetry bursts.
 * - **Telemetry Link**: Search for `[Oversight_Dashboard]` or `GOVERNANCE_SYNC` in service logs.
 */

import { useEffect, useState, useMemo, useCallback } from 'react';
import { useNavigate, useLocation } from 'react-router-dom';
import { Confirm_Dialog } from '../components/ui';
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
import { Oversight_Stats_Bar } from '../components/oversight/Oversight_Stats_Bar';
import { Pending_Approval_Queue } from '../components/oversight/Pending_Approval_Queue';
import { Swarm_Proposals_Panel } from '../components/oversight/Swarm_Proposals_Panel';
import { useOversightHotkeys } from '../components/oversight/use_oversight_hotkeys';
import { i18n } from '../i18n';
import { LD_Json } from '../components/ui/LD_Json';
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

    const active_collaborators = useMemo(() => {
        return new Set(clusters.find(c => c.id === selected_cluster_id)?.collaborators || []);
    }, [clusters, selected_cluster_id]);

    const filtered_pending = useMemo(() => pending.filter(p => {
        const tool_call = p.tool_call || p;
        return selected_cluster_id === 'all'
            || tool_call.cluster_id === selected_cluster_id
            || active_collaborators.has(tool_call.agent_id || '');
    }), [pending, selected_cluster_id, active_collaborators]);

    const stats = useMemo(() => {
        const base_ledger = selected_cluster_id === 'all'
            ? ledger
            : ledger.filter(l => {
                const tc = l.tool_call || l;
                return tc.cluster_id === selected_cluster_id ||
                    active_collaborators.has(tc.agent_id || '');
            });

        return {
            pending: filtered_pending.length,
            approved: base_ledger.filter((entry) => entry.decision === 'approved').length,
            rejected: base_ledger.filter((entry) => entry.decision === 'rejected').length,
        };
    }, [filtered_pending.length, ledger, selected_cluster_id, active_collaborators]);

    const low_risk_pending = useMemo(
        () => filtered_pending.filter(p => classify_skill_risk(p.tool_call?.skill || p.skill, p.tool_call?.params ?? p.params) === 'low'),
        [filtered_pending]
    );

    const parent_delegation_count = useMemo(() => {
        return filtered_pending.filter(p => is_parent_delegation_directive(p.tool_call?.skill || p.skill)).length;
    }, [filtered_pending]);

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

        for (const change of (proposal.changes || [])) {
            const updates: Partial<Agent> = {};
            if (change.proposed_role) updates.role = change.proposed_role;
            if (change.proposed_model) updates.model = change.proposed_model;
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

    const request_batch_approve = useCallback(() => set_confirm_action('batch_approve'), []);

    useOversightHotkeys({
        is_active: is_oversight_route_active,
        is_blocked: Boolean(confirm_action) || is_batch_running,
        filtered_pending,
        low_risk_count: low_risk_pending.length,
        focused_index,
        set_focused_index,
        on_decide: handle_decide,
        on_request_batch_approve: request_batch_approve,
    });

    const resolve_agent_name = useCallback((id?: string) => {
        const agent = all_agents.find(a => a.id === id);
        return agent ? `${agent.name} (${id})` : (id || i18n.t('oversight.unknown_agent'));
    }, [all_agents]);

    const enable_demo_preview = () => {
        set_pending(MOCK_PENDING);
        set_ledger(MOCK_LEDGER);
        set_is_simulated(true);
        set_connection_error(null);
    };

    const exit_demo_preview = () => {
        set_is_simulated(false);
        set_pending([]);
        set_ledger([]);
    };

    return (
        <div className="p-6 space-y-6 max-w-7xl mx-auto">
            <LD_Json data={{
                '@context': 'https://schema.org',
                '@type': 'SoftwareApplication',
                name: 'Tadpole OS Oversight Dashboard',
                description: 'High-level governance and oversight dashboard for swarm-wide compliance and safety protocols.',
                provider: { '@type': 'Organization', name: 'Sovereign Engineering' },
                applicationCategory: 'Governance System',
            }} />
            <h1 className="sr-only">Tadpole OS Oversight & Governance Command</h1>

            <Oversight_Stats_Bar
                stats={stats}
                is_online={is_online}
                is_simulated={is_simulated}
                connection_error={connection_error}
                on_retry_connection={() => void fetch_data()}
                on_enable_demo_preview={enable_demo_preview}
                on_exit_demo_preview={exit_demo_preview}
                on_halt_agents={() => set_confirm_action('halt_agents')}
                on_shutdown_engine={() => set_confirm_action('shutdown_engine')}
                on_open_security={() => navigate('/security')}
            />

            <Pending_Approval_Queue
                filtered_pending={filtered_pending}
                low_risk_count={low_risk_pending.length}
                parent_delegation_count={parent_delegation_count}
                focused_index={focused_index}
                set_focused_index={set_focused_index}
                guidance_map={guidance_map}
                set_guidance_map={set_guidance_map}
                get_remediation={get_remediation}
                on_decide={handle_decide}
                on_request_batch_approve={request_batch_approve}
            />

            <Auto_Approved_Panel ledger={ledger} resolve_agent_name={resolve_agent_name} />

            <Action_Ledger
                ledger={ledger}
                filter={filter}
                set_filter={set_filter}
                selected_cluster_id={selected_cluster_id}
                set_selected_cluster_id={set_selected_cluster_id}
                clusters={clusters || []}
                resolve_agent_name={resolve_agent_name}
            />

            <Swarm_Proposals_Panel
                clusters={clusters || []}
                active_proposals={active_proposals || {}}
                all_agents={all_agents}
                on_dismiss_proposal={dismiss_proposal}
                on_apply_proposal={handle_apply_proposal}
            />

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

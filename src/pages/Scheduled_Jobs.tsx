/**
 * @docs ARCHITECTURE:Interface
 * 
 * ### AI Assist Note
 * **Root View**: Cron-style scheduler for autonomous agent tasks. 
 * Orchestrates the visualization of upcoming jobs, execution history, and task periodicity.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Job execution overlap detection failure, or schedule drift due to local clock desync.
 * - **Telemetry Link**: Search for `[Scheduled_Jobs]` or `JOB_TICK` in service logs.
 */

import React, { useState, useEffect, useMemo, useCallback } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { Clock, Play, Pause, Trash2, ChevronRight, Activity, Plus, Edit2 } from 'lucide-react';
import { tadpole_os_service, type Scheduled_Job, type Scheduled_Job_Run, type Workflow_Entry } from '../services/tadpoleos_service';
import { use_agent_store, use_agent_registry_store } from '../stores/agent_store';
import { event_bus } from '../services/event_bus';
import { Tooltip } from '../components/ui';
import { Confirm_Dialog } from '../components/ui/Confirm_Dialog';
import { i18n } from '../i18n';
import { get_safe_date } from '../utils/date_utils';
import { Job_Form_Manager, Job_History_View, create_empty_job_config, type job_config_state } from '../components/jobs/Scheduled_Job_Components';

/**
 * Scheduled_Jobs Page
 * 
 * ### 🗓️ Administrative Interface
 * Manages automated agent tasks and workflow triggers. Features:
 * - **Cron Scheduling**: Standard UNIX cron expression mapping.
 * - **Budget Guard**: Per-job budget enforcement.
 * - **Run History**: Real-time state management of previous execution results.
 */
const Scheduled_Jobs: React.FC = () => {
    const [jobs, set_jobs] = useState<Scheduled_Job[]>([]);
    const [workflows, set_workflows] = useState<(Workflow_Entry & { type: 'continuity' | 'passive' })[]>([]);
    const [is_loading, set_is_loading] = useState(true);
    const [expanded_job, set_expanded_job] = useState<string | null>(null);
    const [runs_map, set_runs_map] = useState<Record<string, Scheduled_Job_Run[]>>({});
    const [confirm_delete, set_confirm_delete] = useState<{ id: string; name: string } | null>(null);

    // Form state
    const [is_creating, set_is_creating] = useState(false);
    const [editing_job_id, set_editing_job_id] = useState<string | null>(null);
    const [job_type, set_job_type] = useState<'agent' | 'workflow'>('agent');
    const [target_search, set_target_search] = useState('');
    const [job_config, set_job_config] = useState<job_config_state>(create_empty_job_config);

    const { agents } = use_agent_store();

    // Filtered and Sorted Agent List
    const filtered_agents = useMemo(() => {
        if (!Array.isArray(agents)) return [];
        return [...agents]
            .filter(a => 
                a.name.toLowerCase().includes(target_search.toLowerCase()) || 
                a.role.toLowerCase().includes(target_search.toLowerCase())
            )
            .sort((a, b) => a.name.localeCompare(b.name));
    }, [agents, target_search]);

    // Filtered and Sorted Workflow List
    const filtered_workflows = useMemo(() => {
        if (!Array.isArray(workflows)) return [];
        return [...workflows]
            .filter(w => w.name.toLowerCase().includes(target_search.toLowerCase()))
            .sort((a, b) => a.name.localeCompare(b.name));
    }, [workflows, target_search]);



    const fetch_workflows = useCallback(async () => {
        try {
            const [continuity_data, skills_data] = await Promise.all([
                tadpole_os_service.list_continuity_workflows(),
                tadpole_os_service.get_unified_skills()
            ]);

            const continuity_wfs = continuity_data.map(w => ({ ...w, type: 'continuity' as const }));
            const passive_wfs = ((skills_data.workflows || []) as { name: string; content: string }[]).map(w => ({
                id: w.name, 
                name: w.name,
                description: w.content?.slice(0, 100) || '',
                content: w.content,
                created_at: new Date().toISOString(),
                type: 'passive' as const
            }));

            set_workflows([...continuity_wfs, ...passive_wfs]);
        } catch (error: unknown) {
            console.error('Failed to fetch workflows:', error);
        }
    }, []);

    const fetch_jobs = useCallback(async () => {
        try {
            const data = await tadpole_os_service.get_scheduled_jobs();
            set_jobs(data);
        } catch (error: unknown) {
            console.error('Failed to fetch scheduled jobs:', error);
        } finally {
            set_is_loading(false);
        }
    }, []);

    const fetch_runs = useCallback(async (job_id: string) => {
        try {
            const data = await tadpole_os_service.get_scheduled_job_runs(job_id);
            set_runs_map(prev => ({ ...prev, [job_id]: data }));
        } catch (error: unknown) {
            console.error('Failed to fetch runs:', error);
        }
    }, []);

    useEffect(() => {
        void (async () => {
            await Promise.resolve();
            use_agent_registry_store.getState().fetch_agents();
            fetch_workflows();
            fetch_jobs();
        })();
    }, [fetch_jobs, fetch_workflows]);

    const toggle_expand = (job_id: string) => {
        if (expanded_job === job_id) {
            set_expanded_job(null);
        } else {
            set_expanded_job(job_id);
            if (!runs_map[job_id]) {
                fetch_runs(job_id);
            }
        }
    };

    const toggle_enable = useCallback(async (job: Scheduled_Job) => {
        try {
            await tadpole_os_service.update_scheduled_job(job.id, { enabled: !job.enabled });
            event_bus.emit_log({
                source: 'System',
                text: i18n.t(job.enabled ? 'scheduled_jobs.event_disabled' : 'scheduled_jobs.event_enabled', { name: job.name }),
                severity: 'info'
            });
            fetch_jobs();
        } catch (error: unknown) {
            console.error('Failed to toggle job:', error);
        }
    }, [fetch_jobs]);

    const handle_edit = (job: Scheduled_Job) => {
        set_editing_job_id(job.id);
        const type = job.workflow_id ? 'workflow' : 'agent';
        set_job_type(type);
        set_job_config({
            name: job.name,
            agent_id: job.agent_id || '',
            workflow_id: job.workflow_id || null,
            prompt: job.prompt || '',
            cron_expr: job.cron_expr,
            budget_usd: job.budget_usd,
            max_failures: job.max_failures
        });
        set_is_creating(true);
    };

    const delete_job = (id: string, name: string) => {
        set_confirm_delete({ id, name });
    };

    const handle_confirm_delete = async () => {
        if (!confirm_delete) return;
        const { id, name } = confirm_delete;
        
        try {
            await tadpole_os_service.delete_scheduled_job(id);
            event_bus.emit_log({
                source: 'System',
                text: i18n.t('scheduled_jobs.event_terminated', { name: name }),
                severity: 'info'
            });
            set_confirm_delete(null);
            fetch_jobs();
        } catch (error: unknown) {
            console.error('Failed to delete job:', error);
            event_bus.emit_log({
                source: 'System',
                text: i18n.t('scheduled_jobs.event_deletion_failed', { error: error instanceof Error ? error.message : i18n.t('common.unknown_error') }),
                severity: 'error'
            });
            set_confirm_delete(null);
        }
    };

    const handle_job_submit = async (e: React.FormEvent) => {
        e.preventDefault();
        try {
            const sanitized_config: job_config_state = {
                ...job_config,
                name: job_config.name.trim(),
                cron_expr: job_config.cron_expr.trim(),
                workflow_id: job_type === 'workflow' ? (job_config.workflow_id?.trim() || null) : null,
                agent_id: job_type === 'agent' ? job_config.agent_id.trim() : (job_config.agent_id?.trim() || (agents[0]?.id || '')),
                prompt: job_type === 'agent' ? job_config.prompt.trim() : (job_config.prompt?.trim() || '')
            };

            if (editing_job_id) {
                await tadpole_os_service.update_scheduled_job(editing_job_id, sanitized_config);
                event_bus.emit_log({
                    source: 'System',
                    text: i18n.t('scheduled_jobs.event_updated', { name: sanitized_config.name }),
                    severity: 'success'
                });
            } else {
                await tadpole_os_service.create_scheduled_job(sanitized_config);
                event_bus.emit_log({
                    source: 'System',
                    text: i18n.t('scheduled_jobs.event_created', { name: sanitized_config.name }),
                    severity: 'success'
                });
            }
            set_is_creating(false);
            set_editing_job_id(null);
            set_job_config(create_empty_job_config());
            fetch_jobs();
        } catch (error: unknown) { 
            console.error('Failed to handle job:', error);
            const err_msg = error instanceof Error ? error.message : i18n.t('common.unknown_error');
            event_bus.emit_log({
                source: 'System',
                text: `${i18n.t('scheduled_jobs.event_failed_action', { action: editing_job_id ? 'update' : 'create' })}: ${err_msg}`,
                severity: 'error'
            });
        }
    };

    const get_status_indicator = (job: Scheduled_Job) => {
        if (!job.enabled) return <span className="px-2 py-0.5 rounded text-[10px] bg-zinc-800 text-zinc-500 font-mono">{i18n.t('scheduled_jobs.status_disabled')}</span>;
        if (job.consecutive_failures > 0) return <span className="px-2 py-0.5 rounded text-[10px] bg-amber-500/20 text-amber-500 border border-amber-500/30 font-mono">{i18n.t('scheduled_jobs.status_fails', { count: job.consecutive_failures })}</span>;
        return <span className="px-2 py-0.5 rounded text-[10px] bg-emerald-500/10 text-emerald-500 border border-emerald-500/20 font-mono">{i18n.t('scheduled_jobs.status_active')}</span>;
    };

    if (is_loading) {
        return <div className="p-8 text-zinc-500 flex items-center gap-2"><Clock className="animate-pulse" /> {i18n.t('scheduled_jobs.loading')}</div>;
    }

    return (
        <div className="p-8 space-y-8 min-h-screen bg-zinc-950 text-zinc-100">
            {/* GEO Optimization: Structured Data & Semantic Header */}

            <script type="application/ld+json">
            {JSON.stringify({
              "@context": "https://schema.org",
              "@type": "Service",
              "name": "Tadpole OS Automation Scheduler",
              "description": "Enterprise-grade cron-style scheduler for autonomous agent missions and workflow triggers.",
              "provider": { "@type": "Organization", "name": "Sovereign Engineering" },
              "author": { "@type": "Person", "name": "Agent of Nine" }
            })}
            </script>

            <header className="flex justify-between items-end">
                <Tooltip content={i18n.t('scheduled_jobs.header_tooltip')} position="right">
                    <div>
                        <h1 className="text-3xl font-bold bg-clip-text text-transparent bg-gradient-to-r from-white to-zinc-500 cursor-help">
                            {i18n.t('scheduled_jobs.title')}
                        </h1>
                    </div>
                </Tooltip>
                <div className="flex gap-3">
                    <button
                        onClick={() => {
                            set_editing_job_id(null);
                            set_job_config(create_empty_job_config());
                            set_is_creating(true);
                        }}
                        className="px-4 py-2 bg-green-600 hover:bg-green-500 text-white font-bold rounded-lg text-[10px] transition-all flex items-center gap-2 shadow-lg shadow-green-500/20 uppercase tracking-widest"
                    >
                        <Plus size={14} /> {i18n.t('scheduled_jobs.new_job')}
                    </button>
                </div>
            </header>

            {is_creating && (
                <Job_Form_Manager 
                    job_type={job_type}
                    set_job_type={set_job_type}
                    current_job_config={job_config}
                    set_job_config={set_job_config}
                    target_search={target_search}
                    set_target_search={set_target_search}
                    filtered_agents={filtered_agents}
                    filtered_workflows={filtered_workflows}
                    is_creating={is_creating}
                    set_creating={set_is_creating}
                    editing_job_id={editing_job_id}
                    set_editing_job_id={set_editing_job_id}
                    handle_job_submit={handle_job_submit}
                />
            )}

            <div className="bg-zinc-950 border border-zinc-800 rounded-xl overflow-hidden shadow-2xl">
                <table className="w-full text-left text-sm">
                    <thead className="bg-zinc-900 border-b border-zinc-800 text-zinc-400 font-mono text-xs uppercase">
                        <tr>
                            <th className="px-6 py-4 font-medium tracking-wider w-10"></th>
                            <th className="px-6 py-4 font-medium tracking-wider">{i18n.t('scheduled_jobs.th_job_name')}</th>
                            <th className="px-6 py-4 font-medium tracking-wider">{i18n.t('scheduled_jobs.th_target')}</th>
                            <th className="px-6 py-4 font-medium tracking-wider">{i18n.t('scheduled_jobs.th_schedule')}</th>
                            <th className="px-6 py-4 font-medium tracking-wider">{i18n.t('scheduled_jobs.th_next_run')}</th>
                            <th className="px-6 py-4 font-medium tracking-wider">{i18n.t('scheduled_jobs.th_status')}</th>
                            <th className="px-6 py-4 font-medium tracking-wider text-right">{i18n.t('scheduled_jobs.th_actions')}</th>
                        </tr>
                    </thead>
                    <tbody className="divide-y divide-zinc-800">
                        {(Array.isArray(jobs) ? jobs : []).map((job) => {
                            const agent = Array.isArray(agents) ? agents.find(a => a.id === job.agent_id) : undefined;
                            const workflow = Array.isArray(workflows) ? workflows.find(w => w.id === job.workflow_id) : undefined;
                            const is_expanded = expanded_job === job.id;
                            const is_workflow = !!job.workflow_id;

                            return (
                                <React.Fragment key={job.id}>
                                    <tr className={`group hover:bg-zinc-900/50 transition-colors ${is_expanded ? 'bg-zinc-900/50' : ''}`}>
                                        <td className="px-6 py-4 w-10">
                                            <button onClick={() => toggle_expand(job.id)} className="text-zinc-500 hover:text-white transition-colors">
                                                <ChevronRight size={16} className={`transform transition-transform ${is_expanded ? 'rotate-90' : ''}`} />
                                            </button>
                                        </td>
                                        <td className="px-6 py-4 whitespace-nowrap">
                                            <div className="font-semibold text-zinc-100">{job.name}</div>
                                            <div className="text-[10px] text-zinc-500 font-mono mt-1">{job.id.slice(0, 8)}</div>
                                        </td>
                                        <td className="px-6 py-4 whitespace-nowrap">
                                            <div className="flex items-center gap-2">
                                                {is_workflow ? (
                                                    <>
                                                        <div className="w-5 h-5 rounded overflow-hidden bg-green-500/20 flex items-center justify-center border border-green-500/30">
                                                            <Activity size={10} className="text-green-400" />
                                                        </div>
                                                        <span className="text-green-400 font-bold font-mono text-xs italic tracking-tighter">
                                                            {workflow?.name || i18n.t('scheduled_jobs.workflow_default')}
                                                        </span>
                                                    </>
                                                ) : (
                                                    <>
                                                        <div className="w-5 h-5 rounded overflow-hidden bg-zinc-800 shrink-0 border border-zinc-700">
                                                            <img src={`https://api.dicebear.com/9.x/bottts-neutral/svg?seed=${job.agent_id}`} alt="Agent" />
                                                        </div>
                                                        <span className="text-zinc-300 font-mono text-xs">{agent?.name || job.agent_id}</span>
                                                    </>
                                                )}
                                            </div>
                                        </td>
                                        <td className="px-6 py-4 whitespace-nowrap text-green-400 font-mono text-xs font-bold">{job.cron_expr}</td>
                                        <td className="px-6 py-4 whitespace-nowrap text-zinc-400 font-mono text-xs">
                                            {job.enabled ? (get_safe_date({ next_run_at: job.next_run_at })?.toLocaleString() || '-') : '-'}
                                        </td>
                                        <td className="px-6 py-4 whitespace-nowrap">
                                            {get_status_indicator(job)}
                                        </td>
                                        <td className="px-6 py-4 whitespace-nowrap space-x-2 text-right">
                                            <Tooltip content={job.enabled ? i18n.t('scheduled_jobs.pause_execution') : i18n.t('scheduled_jobs.resume_execution')} position="top">
                                                <button onClick={() => toggle_enable(job)} className="p-1.5 text-zinc-500 hover:text-white rounded bg-zinc-800 hover:bg-zinc-700 transition-colors">
                                                    {job.enabled ? <Pause size={14} /> : <Play size={14} />}
                                                </button>
                                            </Tooltip>
                                            <Tooltip content={i18n.t('scheduled_jobs.modify_config')} position="top">
                                                <button onClick={() => handle_edit(job)} className="p-1.5 text-zinc-500 hover:text-green-400 rounded bg-zinc-800 hover:bg-zinc-700 transition-colors">
                                                    <Edit2 size={14} />
                                                </button>
                                            </Tooltip>
                                            <Tooltip content={i18n.t('scheduled_jobs.purge_job_tooltip')} position="top">
                                                 <button 
                                                    onClick={(e) => {
                                                        e.stopPropagation();
                                                        delete_job(job.id, job.name);
                                                    }} 
                                                    className="p-2 text-zinc-500 hover:text-rose-400 rounded-lg bg-zinc-800/50 hover:bg-rose-500/10 border border-zinc-700/50 hover:border-rose-500/30 transition-all active:scale-90"
                                                    aria-label={`${i18n.t('common.delete')} ${job.name}`}
                                                 >
                                                     <Trash2 size={16} className="pointer-events-none" />
                                                 </button>
                                             </Tooltip>
                                        </td>
                                    </tr>

                                    <AnimatePresence>
                                        {is_expanded && (
                                            <tr className="bg-zinc-900/30">
                                                <td colSpan={7} className="p-0 border-b border-zinc-800">
                                                    <motion.div
                                                        initial={{ height: 0, opacity: 0 }}
                                                        animate={{ height: 'auto', opacity: 1 }}
                                                        exit={{ height: 0, opacity: 0 }}
                                                        className="overflow-hidden"
                                                    >
                                                            <Job_History_View job={job} runs={runs_map[job.id]} />
                                                    </motion.div>
                                                </td>
                                            </tr>
                                        )}
                                    </AnimatePresence>
                                </React.Fragment>
                            );
                        })}
                        {jobs.length === 0 && (
                            <tr>
                                <td colSpan={7} className="px-6 py-12 text-center text-zinc-500 font-mono text-sm">
                                    {i18n.t('scheduled_jobs.no_jobs_deployed')}
                                </td>
                            </tr>
                        )}
                    </tbody>
                </table>
            </div>

            <Confirm_Dialog
                is_open={!!confirm_delete}
                title={i18n.t('scheduled_jobs.confirm_purge_title')}
                message={i18n.t('scheduled_jobs.confirm_purge_message', { name: confirm_delete?.name ?? '' })}
                confirm_label={i18n.t('scheduled_jobs.confirm_purge_button')}
                on_confirm={handle_confirm_delete}
                on_cancel={() => set_confirm_delete(null)}
                variant="danger"
            />
        </div>
    );
};

export default Scheduled_Jobs;


// Metadata: [Scheduled_Jobs]

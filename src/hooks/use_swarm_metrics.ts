/**
 * @docs ARCHITECTURE:Logic
 * @docs OPERATIONS_MANUAL:Telemetry
 * 
 * ### AI Assist Note
 * **Custom Hook**: Aggregates and normalizes swarm telemetry metrics for the Global Header. 
 * Combines settings, workspace cluster data, and engine status into a unified visual schema.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Division by zero if `max_agents` is uninitialized, stale cluster counts if `workspace_store` sync lags, or "OFFLINE" status if `useEngineStatus` heartbeat fails.
 * - **Telemetry Link**: Check `Swarm_Status_Header` usages or search `[use_swarm_metrics]` in component tracing.
 */

import { use_settings_store } from '../stores/settings_store';
import { use_workspace_store } from '../stores/workspace_store';
import { use_agent_store } from '../stores/agent_store';
import { useEngineStatus } from './use_engine_status';
import { i18n } from '../i18n';
import { Activity, Cpu, Target, Repeat, Zap, DollarSign, type LucideIcon } from 'lucide-react';

/**
 * Interface for localized swarm telemetry metric
 */
export interface SwarmMetric {
    label: string;
    value: string | number;
    icon: LucideIcon;
    tooltip: string;
    color: string;
}

/**
 * useSwarmMetrics
 * Custom hook to aggregate swarm telemetry metrics for the header.
 */
export const useSwarmMetrics = (): SwarmMetric[] => {
    const { settings } = use_settings_store();
    const { clusters } = use_workspace_store();
    const { agents: agents_list } = use_agent_store();
    const { is_online, active_agents: nodes_online_count } = useEngineStatus();

    const clusters_list = clusters || [];
    const active_agents_count = (agents_list || []).filter(a => ['active', 'speaking', 'thinking', 'coding'].includes(a.status || '')).length;
    const active_clusters_count = clusters_list.length;

    return [
        {
            label: i18n.t('stats.active_swarm'),
            value: `${active_agents_count}/${settings.max_agents}`,
            icon: Cpu,
            tooltip: i18n.t('stats.active_swarm_tooltip'),
            color: 'text-emerald-400'
        },
        {
            label: i18n.t('telemetry.swarm_density'),
            value: `${active_clusters_count}/${settings.max_clusters}`,
            icon: Target,
            tooltip: i18n.t('telemetry.swarm_density_tooltip'),
            color: 'text-green-400'
        },
        {
            label: i18n.t('stats.system_health'),
            value: is_online ? `${nodes_online_count} NODES` : 'OFFLINE',
            icon: Activity,
            tooltip: i18n.t('stats.system_health_tooltip'),
            color: is_online ? 'text-cyan-400' : 'text-zinc-600'
        },
        {
            label: i18n.t('telemetry.logic_depth'),
            value: settings.max_swarm_depth,
            icon: Repeat,
            tooltip: i18n.t('telemetry.logic_depth_tooltip'),
            color: 'text-zinc-400'
        },
        {
            label: i18n.t('stats.swarm_tokens'),
            value: `${Math.round(settings.max_task_length / 1024)}k`,
            icon: Zap,
            tooltip: i18n.t('stats.swarm_tokens_tooltip'),
            color: 'text-amber-400'
        },
        {
            label: i18n.t('stats.swarm_cost'),
            value: `${i18n.t('agent_config.label_currency_symbol')}${settings.default_budget_usd.toFixed(2)}`,
            icon: DollarSign,
            tooltip: i18n.t('stats.swarm_cost_tooltip'),
            color: 'text-emerald-500'
        }
    ];
};

// Metadata: [use_swarm_metrics]

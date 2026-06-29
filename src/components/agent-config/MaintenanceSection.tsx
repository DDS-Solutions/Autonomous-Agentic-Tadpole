/**
 * @docs ARCHITECTURE:Interface
 * 
 * ### AI Assist Note
 * **UI Component**: Agent Maintenance Diagnostics panel.
 * Evaluates and displays agent stability across 7 dimensions (Capability Drift, Memory Health,
 * Rate Limits, Latency, Error Rate, Budget Headroom, Dependency Health) with premium visuals.
 *
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: UI errors or callback stack traces.
 * - **Telemetry Link**: Search `[MaintenanceSection]` in console logs.
 */

import React, { useState, useEffect, useCallback } from 'react';
import { Activity, RefreshCw, AlertTriangle, CheckCircle2, ShieldAlert, Clock, Coins, Brain, GitPullRequest } from 'lucide-react';
import { tadpole_os_service } from '../../services/tadpoleos_service';
import { i18n } from '../../i18n';

interface MaintenanceDimensionState {
    score: number;
    status: string;
    details: string;
}

interface MaintenanceReport {
    agentId: string;
    overallScore: number;
    status: string;
    capabilityDrift: MaintenanceDimensionState;
    memoryHealth: MaintenanceDimensionState;
    rateLimits: MaintenanceDimensionState;
    latency: MaintenanceDimensionState;
    errorRate: MaintenanceDimensionState;
    budgetHeadroom: MaintenanceDimensionState;
    dependencyHealth: MaintenanceDimensionState;
    evaluatedAt: number;
}

interface MaintenanceSectionProps {
    agentId: string;
    themeColor: string;
}

// ─── Helpers outside component ──────────────────────────────────────────────────

/**
 * Standardizes time formatting to HH:mm:ss UTC.
 */
const formatSystemTime = (sec: number) => {
    const d = new Date(sec * 1000);
    const hh = String(d.getUTCHours()).padStart(2, '0');
    const mm = String(d.getUTCMinutes()).padStart(2, '0');
    const ss = String(d.getUTCSeconds()).padStart(2, '0');
    return `${hh}:${mm}:${ss} UTC`;
};

/**
 * Maps the 7 diagnostic dimensions into a structured collection for rendering.
 */
const getDimensions = (report: MaintenanceReport) => [
    { key: 'capabilityDrift', name: i18n.t('agent_config.dim_capability_drift'), icon: <GitPullRequest size={18} />, data: report.capabilityDrift },
    { key: 'memoryHealth', name: i18n.t('agent_config.dim_memory_health'), icon: <Brain size={18} />, data: report.memoryHealth },
    { key: 'rateLimits', name: i18n.t('agent_config.dim_rate_limits'), icon: <Activity size={18} />, data: report.rateLimits },
    { key: 'latency', name: i18n.t('agent_config.dim_latency'), icon: <Clock size={18} />, data: report.latency },
    { key: 'errorRate', name: i18n.t('agent_config.dim_error_rate'), icon: <ShieldAlert size={18} />, data: report.errorRate },
    { key: 'budgetHeadroom', name: i18n.t('agent_config.dim_budget_headroom'), icon: <Coins size={18} />, data: report.budgetHeadroom },
    { key: 'dependencyHealth', name: i18n.t('agent_config.dim_dependency_health'), icon: <GitPullRequest size={18} />, data: report.dependencyHealth },
];

export function MaintenanceSection({ agentId, themeColor }: MaintenanceSectionProps) {
    const [report, setReport] = useState<MaintenanceReport | null>(null);
    const [isLoading, setIsLoading] = useState(true);
    const [errorMessage, setErrorMessage] = useState<string | null>(null);

    const loadReport = useCallback(async () => {
        setIsLoading(true);
        setErrorMessage(null);
        try {
            const data = await tadpole_os_service.get_agent_maintenance_report(agentId);
            setReport(data);
        } catch (err) {
            const message = err instanceof Error ? err.message : i18n.t('agent_config.unknown_diagnostics_error');
            setErrorMessage(message);
        } finally {
            setIsLoading(false);
        }
    }, [agentId]);

    useEffect(() => {
        // eslint-disable-next-line react-hooks/set-state-in-effect
        loadReport();
    }, [loadReport]);

    const getStatusColor = (status: string) => {
        switch (status.toLowerCase()) {
            case 'optimal':
                return 'text-emerald-400 border-emerald-500/20 bg-emerald-500/5';
            case 'warning':
                return 'text-amber-400 border-amber-500/20 bg-amber-500/5';
            case 'critical':
                return 'text-red-400 border-red-500/20 bg-red-500/5';
            default:
                return 'text-zinc-400 border-zinc-500/20 bg-zinc-500/5';
        }
    };

    const getStatusIcon = (status: string, size = 16) => {
        switch (status.toLowerCase()) {
            case 'optimal':
                return <CheckCircle2 size={size} className="text-emerald-400" />;
            case 'warning':
                return <AlertTriangle size={size} className="text-amber-400" />;
            case 'critical':
                return <ShieldAlert size={size} className="text-red-400" />;
            default:
                return <Activity size={size} className="text-zinc-400" />;
        }
    };

    if (isLoading) {
        return (
            <div className="flex flex-col items-center justify-center py-20 space-y-4">
                <RefreshCw size={36} className="text-zinc-600 animate-spin" style={{ color: themeColor }} />
                <p className="text-xs text-zinc-500 uppercase tracking-widest font-mono">
                    {i18n.t('agent_config.running_diagnostics')}
                </p>
            </div>
        );
    }

    if (errorMessage || !report) {
        return (
            <div className="p-6 bg-red-500/5 border border-red-500/10 rounded-2xl flex items-start gap-4">
                <AlertTriangle className="text-red-500 shrink-0 mt-0.5" />
                <div className="space-y-1">
                    <h4 className="text-xs font-bold text-red-400 uppercase tracking-wider font-mono">
                        {i18n.t('agent_config.diagnostics_error')}
                    </h4>
                    <p className="text-xs text-zinc-400">
                        {errorMessage || i18n.t('agent_config.unknown_diagnostics_error')}
                    </p>
                    <button
                        onClick={loadReport}
                        className="mt-3 px-4 py-1.5 bg-zinc-900 border border-zinc-800 text-[10px] font-bold text-zinc-400 rounded-lg uppercase tracking-wider hover:text-zinc-200 transition-all"
                    >
                        {i18n.t('agent_config.btn_retry_audit')}
                    </button>
                </div>
            </div>
        );
    }

    const dimensions = getDimensions(report);

    // Radial Gauge SVG Constants
    const radius = 50;
    const circumference = 2 * Math.PI * radius;
    const strokeDashoffset = circumference - (report.overallScore * circumference);

    return (
        <div className="p-6 space-y-6">
            {/* Header / Summary Card */}
            <div className="relative p-6 bg-zinc-900/30 border border-zinc-800/80 rounded-3xl overflow-hidden flex flex-col md:flex-row items-center gap-6">
                <div className="absolute inset-0 bg-gradient-to-r from-zinc-950/40 via-transparent to-transparent pointer-events-none" />
                
                {/* Radial Health Gauge */}
                <div className="relative flex items-center justify-center shrink-0">
                    <svg 
                        className="w-32 h-32 transform -rotate-90"
                        role="img"
                        aria-label={`Agent health score is ${Math.round(report.overallScore * 100)} percent`}
                    >
                        <circle
                            cx="64"
                            cy="64"
                            r={radius}
                            className="stroke-zinc-800"
                            strokeWidth="10"
                            fill="transparent"
                        />
                        <circle
                            cx="64"
                            cy="64"
                            r={radius}
                            stroke={themeColor}
                            strokeWidth="10"
                            fill="transparent"
                            strokeDasharray={circumference}
                            strokeDashoffset={strokeDashoffset}
                            strokeLinecap="round"
                            className="transition-all duration-1000 ease-out"
                        />
                    </svg>
                    <div className="absolute flex flex-col items-center justify-center text-center">
                        <span className="text-2xl font-bold font-mono tracking-tight text-white">
                            {Math.round(report.overallScore * 100)}%
                        </span>
                        <span className="text-[8px] font-bold text-zinc-500 uppercase tracking-widest font-mono">
                            {i18n.t('agent_config.health_label')}
                        </span>
                    </div>
                </div>

                <div className="flex-1 text-center md:text-left space-y-2">
                    <div className="flex flex-col md:flex-row md:items-center gap-2 md:gap-3">
                        <h3 className="text-base font-bold text-zinc-200 font-mono uppercase tracking-wide">
                            {i18n.t('agent_config.diagnostics_ledger')}
                        </h3>
                        <span className={`inline-flex items-center justify-center self-center px-3 py-0.5 rounded-full border text-[9px] font-mono font-bold uppercase tracking-wider ${getStatusColor(report.status)}`}>
                            {report.status}
                        </span>
                    </div>
                    <p className="text-xs text-zinc-400">
                        {i18n.t('agent_config.diagnostics_ledger_desc')}
                    </p>
                    <div className="text-[10px] text-zinc-600 font-mono flex items-center justify-center md:justify-start gap-4">
                        <span>
                            {i18n.t('agent_config.last_evaluation', { time: formatSystemTime(report.evaluatedAt) })}
                        </span>
                    </div>
                </div>

                <button
                    onClick={loadReport}
                    className="p-3 bg-zinc-900 border border-zinc-800 rounded-2xl hover:border-zinc-700 hover:text-zinc-200 text-zinc-400 transition-all self-center shrink-0 shadow-lg active:scale-95"
                    style={{ '--theme-color': themeColor } as React.CSSProperties}
                    aria-label={i18n.t('agent_config.refresh_diagnostics')}
                >
                    <RefreshCw size={16} />
                </button>
            </div>

            {/* Diagnostics Dimensions Grid */}
            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                {dimensions.map((dim) => {
                    const statusClass = getStatusColor(dim.data.status);
                    return (
                        <div
                            key={dim.key}
                            className="p-4 bg-zinc-900/10 border border-zinc-900/60 rounded-2xl flex flex-col justify-between gap-3 hover:border-zinc-800/80 transition-all group"
                        >
                            <div className="flex items-start justify-between gap-3">
                                <div className="flex items-center gap-3">
                                    <div
                                        className="p-2.5 rounded-xl border border-zinc-800/80 text-zinc-500 group-hover:text-zinc-300 transition-colors"
                                        style={{ color: dim.data.status.toLowerCase() !== 'optimal' ? undefined : themeColor }}
                                    >
                                        {dim.icon}
                                    </div>
                                    <div>
                                        <h4 className="text-xs font-bold text-zinc-300 tracking-wide font-mono">
                                            {dim.name}
                                        </h4>
                                        <div className="flex items-center gap-1.5 mt-0.5">
                                            {getStatusIcon(dim.data.status, 12)}
                                            <span className={`text-[9px] font-bold uppercase tracking-wider ${statusClass.split(' ')[0]}`}>
                                                {dim.data.status}
                                            </span>
                                        </div>
                                    </div>
                                </div>
                                <span className="text-xs font-mono font-bold text-zinc-500 group-hover:text-zinc-300 transition-colors">
                                    {Math.round(dim.data.score * 100)}%
                                </span>
                            </div>

                            {/* Details Text */}
                            <p className="text-[11px] leading-relaxed text-zinc-400 bg-zinc-950/20 border border-zinc-950/40 p-2.5 rounded-xl font-mono">
                                {dim.data.details}
                            </p>

                            {/* Dimension Progress Bar */}
                            <div className="w-full h-1 bg-zinc-950 rounded-full overflow-hidden shrink-0">
                                <div
                                    className="h-full rounded-full transition-all duration-1000 ease-out"
                                    style={{
                                        width: `${dim.data.score * 100}%`,
                                        backgroundColor: 
                                            dim.data.status.toLowerCase() === 'critical' ? '#ef4444' :
                                            dim.data.status.toLowerCase() === 'warning' ? '#f59e0b' :
                                            themeColor
                                    }}
                                />
                            </div>
                        </div>
                    );
                })}
            </div>
        </div>
    );
}

// Metadata: [MaintenanceSection]

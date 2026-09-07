/**
 * @docs ARCHITECTURE:UI-Pages
 * 
 * ### AI Assist Note
 * **Core technical resource for the Tadpole OS Sovereign infrastructure.**
 * Handles reactive state and high-fidelity user interactions.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: UI regression, hook desync, or API timeout.
 * - **Telemetry Link**: Search `[Governance_View]` in observability traces.
 */

import { useState, useEffect, useRef, useCallback } from 'react';
import { use_settings_store } from '../stores/settings_store';
import { browser_inference_service } from '../services/browser_inference';
import { governance_service } from '../services/governance_service';
import { use_browser_specialist_store } from '../stores/browser_specialist_store';
import { sentinel_daemon } from '../services/sentinel_daemon';
import { i18n } from '../i18n';
import { 
    Shield, 
    Activity, 
    CreditCard, 
    Terminal, 
    AlertTriangle, 
    RefreshCcw, 
    Lock, 
    Zap, 
    Scale, 
    Brain, 
    Play, 
    RotateCw, 
    Cpu 
} from 'lucide-react';

import type { GovernanceQuotas } from '../contracts/governance';
import { LD_Json } from '../components/ui/LD_Json';
import { get_safe_date } from '../utils/date_utils';

export default function Governance_View() {
    // --- Local View State (Lazy Initializers) ---
    const [manifest, setManifest] = useState<string>('');
    const [quotas, setQuotas] = useState<GovernanceQuotas | null>(() => governance_service.get_current_quotas());
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState<string | null>(null);
    const [lastRefresh, setLastRefresh] = useState(() => new Date());
    const [isAuditing, setIsAuditing] = useState(false);

    // --- Lifecycle Refs ---
    const isSyncingRef = useRef(false);
    const isMountedRef = useRef(true);

    // --- Atomic Settings Store Selectors (Prevents Re-Render Storms) ---
    const is_safe_mode = use_settings_store((s) => s.settings.is_safe_mode);
    const sentinel_mode = use_settings_store((s) => s.settings.sentinel_mode);
    const browser_specialist_model_id = use_settings_store((s) => s.settings.browser_specialist_model_id);
    const update_setting = use_settings_store((s) => s.update_setting);

    // --- Atomic Browser Specialist Store Selectors ---
    const specialist_status = use_browser_specialist_store((s) => s.status);
    const specialist_device = use_browser_specialist_store((s) => s.active_device);
    const specialist_progress = use_browser_specialist_store((s) => s.model_loading_progress);
    const specialist_error = use_browser_specialist_store((s) => s.last_error);
    const specialist_scan_at = use_browser_specialist_store((s) => s.last_scan_at);
    const specialist_anomalies = use_browser_specialist_store((s) => s.anomaly_count);
    const specialist_init = use_browser_specialist_store((s) => s.init);
    const specialist_reset_pipeline = use_browser_specialist_store((s) => s.reset_pipeline);

    // --- Critical #1: Reconcile Persisted sentinel_mode on Mount ---
    useEffect(() => {
        if (sentinel_mode && specialist_status === 'idle') {
            sentinel_daemon.start();
            void specialist_init().catch((err: unknown) => {
                console.error('[Governance_View] Failed to initialize specialist on mount:', err);
            });
        }
    }, [sentinel_mode, specialist_status, specialist_init]);

    // Track unmount lifecycle for async safety
    useEffect(() => {
        isMountedRef.current = true;
        return () => {
            isMountedRef.current = false;
        };
    }, []);

    // --- Action Handlers with Complete Rejection Guarding ---
    const toggle_sentinel = async () => {
        const next = !sentinel_mode;
        update_setting('sentinel_mode', next);
        if (next) {
            sentinel_daemon.start();
            try {
                await specialist_init();
            } catch (err: unknown) {
                console.error('[Governance_View] Specialist init error on toggle:', err);
            }
        } else {
            sentinel_daemon.stop();
            browser_inference_service.dispose();
            use_browser_specialist_store.setState({ 
                status: 'idle', 
                active_device: 'none',
                last_error: null 
            });
        }
    };

    const handle_run_audit = async () => {
        if (!sentinel_mode || isAuditing || specialist_status === 'thinking' || specialist_status === 'loading') return;
        setIsAuditing(true);
        try {
            await sentinel_daemon.scan_now();
        } catch (err: unknown) {
            console.error('[Governance_View] Sentinel audit scan failed:', err);
            use_browser_specialist_store.setState({
                status: 'error',
                last_error: err instanceof Error ? err.message : String(err),
            });
        } finally {
            if (isMountedRef.current) {
                setIsAuditing(false);
            }
        }
    };

    const handle_reset_pipeline = async () => {
        if (specialist_status === 'loading' || specialist_status === 'thinking') return;
        try {
            await specialist_reset_pipeline();
        } catch (err: unknown) {
            console.error('[Governance_View] Pipeline reset error:', err);
        }
    };

    // --- Network Sync with Abort / Timeout Protection (High #6) ---
    const fetchGovernance = useCallback(async () => {
        if (isSyncingRef.current) return;
        isSyncingRef.current = true;
        
        setLoading(true);
        setError(null);
        try {
            const timeoutPromise = new Promise<never>((_, reject) => 
                setTimeout(() => reject(new Error('Sync timeout')), 10000)
            );
            const syncPromise = Promise.all([
                governance_service.get_manifest(),
                governance_service.sync()
            ]);

            const [m, q] = await Promise.race([syncPromise, timeoutPromise]);
            if (isMountedRef.current) {
                setManifest(m);
                setQuotas(q);
                setLastRefresh(new Date());
            }
        } catch (e: unknown) {
            if (isMountedRef.current) {
                console.error('[Governance_View] Sync Failed:', e);
                setError('Neural link synchronization failed. System state may be stale.');
            }
        } finally {
            if (isMountedRef.current) {
                setLoading(false);
            }
            isSyncingRef.current = false;
        }
    }, []);

    // --- Effect Wiring: Clean Dependency Graph (High #5) ---
    useEffect(() => {
        let active = true;
        const initSync = async () => {
            try {
                const [m, q] = await Promise.all([
                    governance_service.get_manifest(),
                    governance_service.sync()
                ]);
                if (active && isMountedRef.current) {
                    setManifest(m);
                    setQuotas(q);
                    setLastRefresh(new Date());
                }
            } catch (e: unknown) {
                if (active && isMountedRef.current) {
                    console.error('[Governance_View] Initial Sync Failed:', e);
                    setError('Neural link synchronization failed. System state may be stale.');
                }
            } finally {
                if (active && isMountedRef.current) {
                    setLoading(false);
                }
            }
        };

        void initSync();
        
        // Event-driven sync: Listen for pulses from the service
        const unsubscribe = governance_service.on_pulse((new_quotas) => {
            if (isMountedRef.current) {
                setQuotas(new_quotas);
                setLastRefresh(new Date());
            }
        });

        return () => {
            active = false;
            unsubscribe();
        };
    }, []);

    // --- Mathematical & Precision Normalization ---
    const spentPercentage = quotas && quotas.total_budget > 0 
        ? Math.min(100, Math.max(0, (quotas.total_spent / quotas.total_budget) * 100))
        : 0;

    const efficiencyDisplay = quotas?.efficiency !== undefined
        ? (quotas.efficiency <= 1.0 ? quotas.efficiency * 100 : quotas.efficiency).toFixed(1)
        : '0.0';

    // Float equality extraction (High #8)
    const merkle_val = quotas?.system_defense?.merkle_integrity ?? 1;
    const is_merkle_intact = merkle_val >= 0.9999;

    return (
        <div className="flex flex-col h-full bg-[#050505] text-zinc-100 p-6 overflow-y-auto custom-scrollbar">
            <LD_Json data={{
                "@context": "https://schema.org",
                "@type": "Service",
                "name": "Tadpole OS Sovereign Governance",
                "description": "Kernel-level resource orchestration and defense protocol management.",
                "provider": { "@type": "Organization", "name": "Sovereign Engineering" }
            }} />

            {/* Header */}
            <div className="flex items-center justify-between mb-8">
                <div className="flex items-center gap-3">
                    <div className="p-3 bg-indigo-500/10 border border-indigo-500/20 rounded-xl">
                        <Scale className="w-6 h-6 text-indigo-400" />
                    </div>
                    <div>
                        <h1 className="text-2xl font-bold tracking-tight">{i18n.t('governance.title', { defaultValue: 'Sovereign Governance' })}</h1>
                        <p className="text-zinc-500 text-sm">{i18n.t('governance.subtitle', { defaultValue: 'Orchestrating the Aletheia Protocol and Resource Boundaries' })}</p>
                    </div>
                </div>
                <div className="flex items-center gap-4">
                    {error && (
                        <div 
                            role="alert" 
                            aria-live="polite" 
                            className="px-3 py-1 bg-red-500/10 border border-red-500/20 rounded-lg flex items-center gap-2 animate-pulse"
                        >
                            <AlertTriangle className="w-3 h-3 text-red-400" />
                            <span className="text-[10px] font-mono text-red-400 uppercase">{error}</span>
                        </div>
                    )}
                    <div className="text-right">
                        <p className="text-[10px] text-zinc-600 uppercase font-mono tracking-widest">Last Intelligence Sync</p>
                        <p className="text-xs text-zinc-400 font-mono">{get_safe_date(lastRefresh)?.toLocaleTimeString() || '--:--:--'}</p>
                    </div>
                    <button 
                        onClick={() => void fetchGovernance()}
                        disabled={loading}
                        aria-label={i18n.t('governance.aria_refresh', { defaultValue: 'Refresh governance quotas and sovereign manifest' })}
                        className="p-2 hover:bg-white/5 rounded-lg transition-colors border border-zinc-800 disabled:opacity-50"
                    >
                        <RefreshCcw className={`w-4 h-4 text-zinc-400 ${loading ? 'animate-spin' : ''}`} />
                    </button>
                </div>
            </div>

            <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
                {/* Left Column: Financial & Health */}
                <div className="lg:col-span-1 space-y-6">
                    {/* Budget Pulse */}
                    <div className="bg-[#0a0a0a] border border-zinc-800/50 rounded-2xl p-6 relative overflow-hidden group">
                        <div className="absolute top-0 right-0 p-4 opacity-5 group-hover:opacity-10 transition-opacity">
                            <CreditCard className="w-24 h-24 text-white" />
                        </div>
                        
                        <div className="flex items-center gap-2 mb-6">
                            <Activity className="w-4 h-4 text-emerald-400" />
                            <h2 className="text-xs font-bold uppercase tracking-widest text-zinc-500">Mission Budget Pulse</h2>
                        </div>

                        <div className="space-y-4">
                            <div>
                                <div className="flex justify-between items-end mb-2">
                                    <span className="text-3xl font-mono font-bold">
                                        ${Number.isFinite(quotas?.total_spent) ? quotas?.total_spent.toFixed(2) : '0.00'}
                                    </span>
                                    <span className="text-zinc-500 text-xs mb-1">
                                        of ${Number.isFinite(quotas?.total_budget) ? quotas?.total_budget.toFixed(2) : '0.00'} limit
                                    </span>
                                </div>
                                <div className="h-2 bg-zinc-900 rounded-full overflow-hidden">
                                    <div 
                                        className="h-full bg-gradient-to-r from-emerald-500 to-indigo-500 transition-all duration-1000"
                                        style={{ width: `${spentPercentage}%` }}
                                    />
                                </div>
                            </div>
                            
                            <div className="grid grid-cols-2 gap-4 pt-4 border-t border-zinc-800/50">
                                <div>
                                    <p className="text-[10px] text-zinc-600 uppercase font-mono mb-1">Efficiency Score</p>
                                    <p className="text-lg font-mono text-emerald-400">{efficiencyDisplay}%</p>
                                </div>
                                <div>
                                    <p className="text-[10px] text-zinc-600 uppercase font-mono mb-1">Remaining</p>
                                    <p className="text-lg font-mono">${Number.isFinite(quotas?.remaining) ? quotas?.remaining.toFixed(2) : '0.00'}</p>
                                </div>
                            </div>
                        </div>
                    </div>

                    {/* System Defense Status */}
                    <div className="bg-[#0a0a0a] border border-zinc-800/50 rounded-2xl p-6">
                        <div className="flex items-center gap-2 mb-6">
                            <Shield className="w-4 h-4 text-indigo-400" />
                            <h2 className="text-xs font-bold uppercase tracking-widest text-zinc-500">Kernel Defense</h2>
                        </div>

                        <div className="space-y-6">
                            <div className="flex items-center justify-between">
                                <div className="flex items-center gap-3">
                                    <div className={`w-2 h-2 rounded-full animate-pulse ${
                                        is_safe_mode && is_merkle_intact
                                            ? 'bg-emerald-500'
                                            : 'bg-amber-500'
                                    }`} />
                                    <span className="text-sm font-medium">Aletheia Verification</span>
                                    <div className={`p-1.5 rounded-lg border flex items-center gap-2 ${is_safe_mode ? 'bg-emerald-500/10 border-emerald-500/20 text-emerald-400' : 'bg-red-500/10 border-red-500/20 text-red-400'}`}>
                                        <Shield size={12} />
                                        <span className="text-[10px] font-bold uppercase tracking-wider">{is_safe_mode ? 'Verified' : 'Bypassed'}</span>
                                    </div>
                                    <span className="text-[10px] text-zinc-500 font-mono">OS: v1.1.58 | Mode: {is_safe_mode ? 'Secure' : 'Unrestricted'}</span>
                                </div>
                            </div>

                            <div className="space-y-2">
                                <div className="flex justify-between text-[10px] uppercase font-mono text-zinc-600">
                                    <span>Merkle Chain Integrity</span>
                                    <span className={is_merkle_intact ? 'text-indigo-400' : 'text-rose-400'}>
                                        {(merkle_val * 100).toFixed(2)}%
                                    </span>
                                </div>
                                <div className="h-1 bg-zinc-900 rounded-full overflow-hidden">
                                    <div 
                                        className={`h-full transition-all duration-500 ${
                                            is_merkle_intact ? 'bg-indigo-500/50' : 'bg-rose-500/80'
                                        }`} 
                                        style={{ width: `${Math.min(100, Math.max(0, merkle_val * 100))}%` }}
                                    />
                                </div>
                            </div>

                            {/* Host Containment & Resource Guard Telemetry */}
                            <div className="grid grid-cols-2 gap-3 pt-3 border-t border-zinc-800/40 text-[11px]">
                                <div className="bg-zinc-950/40 border border-zinc-800/40 rounded-lg p-2.5">
                                    <div className="text-[10px] font-mono uppercase text-zinc-500 mb-0.5">Sandbox Isolation</div>
                                    <div className="font-semibold text-zinc-200 flex items-center gap-1.5">
                                        <span className={`w-1.5 h-1.5 rounded-full ${quotas?.system_defense?.sandbox_status === 'ACTIVE' ? 'bg-emerald-400' : 'bg-amber-400'}`} />
                                        <span className="font-mono text-xs">{quotas?.system_defense?.sandbox_type || 'Host Native'}</span>
                                    </div>
                                </div>
                                <div className="bg-zinc-950/40 border border-zinc-800/40 rounded-lg p-2.5">
                                    <div className="text-[10px] font-mono uppercase text-zinc-500 mb-0.5">Host Memory Guard</div>
                                    <div className="font-semibold text-zinc-200 flex items-center gap-1.5">
                                        <span className={`font-mono text-xs ${((quotas?.system_defense?.memory_pressure ?? 0) > 0.85) ? 'text-rose-400' : 'text-emerald-400'}`}>
                                            {quotas?.system_defense?.memory_pressure !== undefined ? `${((quotas.system_defense.memory_pressure) * 100).toFixed(1)}%` : 'Nominal'}
                                        </span>
                                    </div>
                                </div>
                            </div>
                        </div>
                    </div>

                    {/* Sentinel Mode Cognitive Shield Card */}
                    <div className="bg-[#0a0a0a] border border-zinc-800/50 rounded-2xl p-6 shadow-xl relative overflow-hidden">
                        <div className="flex items-center justify-between gap-4 mb-4">
                            <div className="flex items-center gap-2.5">
                                <div className="w-8 h-8 rounded-lg bg-cyan-950/40 border border-cyan-500/30 flex items-center justify-center">
                                    <Brain className="w-4 h-4 text-cyan-400" />
                                </div>
                                <div>
                                    <h2 className="text-xs font-bold uppercase tracking-widest text-zinc-300">Browser Sentinel</h2>
                                    <p className="text-[10px] text-zinc-500 font-mono">
                                        {browser_specialist_model_id || 'SmolLM-360M-Instruct'} · q4
                                    </p>
                                </div>
                            </div>

                            <button
                                id="sentinel-mode-toggle"
                                role="switch"
                                aria-checked={sentinel_mode}
                                onClick={toggle_sentinel}
                                aria-label={i18n.t('governance.aria_toggle_sentinel', { defaultValue: 'Toggle Sentinel Mode' })}
                                className={`relative flex-shrink-0 w-12 h-6 rounded-full transition-colors duration-300 focus:outline-none focus:ring-2 focus:ring-cyan-500/50 ${
                                    sentinel_mode ? 'bg-cyan-500' : 'bg-zinc-700'
                                }`}
                            >
                                <div className={`absolute top-1 w-4 h-4 rounded-full bg-white shadow-md transition-all duration-300 ${
                                    sentinel_mode ? 'left-7' : 'left-1'
                                }`} />
                            </button>
                        </div>

                        <p className="text-[11px] text-zinc-400 leading-relaxed mb-4">
                            Autonomous in-browser cognitive sentry. Performs client-side DOM health audits, 
                            entropy scans, and zero-leakage DLP filtering without sending UI data to external clouds.
                        </p>

                        {/* Status Ribbon */}
                        <div className="mb-4">
                            {!sentinel_mode ? (
                                <div className="inline-flex items-center gap-2 px-2.5 py-1 rounded-md text-[10px] font-bold uppercase tracking-wider bg-zinc-900 border border-zinc-800 text-zinc-500">
                                    <div className="w-2 h-2 rounded-full bg-zinc-600" />
                                    <span>Sentinel Offline (Dormant)</span>
                                </div>
                            ) : specialist_status === 'loading' ? (
                                <div className="inline-flex items-center gap-2 px-2.5 py-1 rounded-md text-[10px] font-bold uppercase tracking-wider bg-amber-500/10 border border-amber-500/30 text-amber-300">
                                    <RotateCw className="w-3 h-3 animate-spin text-amber-400" />
                                    <span>Warming Up {specialist_progress > 0 ? `(${specialist_progress}%)` : 'Pipelines...'}</span>
                                </div>
                            ) : specialist_status === 'thinking' ? (
                                <div className="inline-flex items-center gap-2 px-2.5 py-1 rounded-md text-[10px] font-bold uppercase tracking-wider bg-cyan-500/15 border border-cyan-500/30 text-cyan-300">
                                    <div className="w-2 h-2 rounded-full bg-cyan-400 animate-ping" />
                                    <span>Auditing Active DOM...</span>
                                </div>
                            ) : specialist_status === 'error' ? (
                                <div className="flex flex-col gap-1">
                                    <div 
                                        className="inline-flex items-center gap-2 px-2.5 py-1 rounded-md text-[10px] font-bold uppercase tracking-wider bg-rose-500/10 border border-rose-500/30 text-rose-400"
                                        title={specialist_error || 'Pipeline initialization failed. Click Reset to retry.'}
                                    >
                                        <AlertTriangle className="w-3 h-3 text-rose-400" />
                                        <span>Pipeline Exception</span>
                                    </div>
                                    {specialist_error && (
                                        <div className="text-[10px] font-mono text-rose-400/80 max-w-xs truncate" title={specialist_error}>
                                            {specialist_error}
                                        </div>
                                    )}
                                </div>
                            ) : (
                                <div className="inline-flex items-center gap-2 px-2.5 py-1 rounded-md text-[10px] font-bold uppercase tracking-wider bg-emerald-500/10 border border-emerald-500/30 text-emerald-300">
                                    <div className="w-2 h-2 rounded-full bg-emerald-400 animate-pulse" />
                                    <span>Sentinel Active · {specialist_device === 'none' ? 'READY' : specialist_device.toUpperCase()}</span>
                                </div>
                            )}
                        </div>

                        {/* Telemetry Metrics Grid */}
                        <div className="grid grid-cols-2 gap-2.5 pt-3 border-t border-zinc-800/60 mb-4 text-[11px]">
                            <div className="bg-zinc-950/60 border border-zinc-800/40 rounded-lg p-2.5">
                                <div className="text-[10px] font-mono uppercase text-zinc-500 mb-0.5">Execution Core</div>
                                <div className="font-semibold text-zinc-200 flex items-center gap-1.5">
                                    <Cpu className="w-3.5 h-3.5 text-cyan-400" />
                                    {specialist_device === 'none' 
                                        ? (sentinel_mode ? 'Allocating...' : 'Dormant') 
                                        : specialist_device.toUpperCase()}
                                </div>
                            </div>
                            <div className="bg-zinc-950/60 border border-zinc-800/40 rounded-lg p-2.5">
                                <div className="text-[10px] font-mono uppercase text-zinc-500 mb-0.5">DLP Shield</div>
                                <div className="font-semibold text-emerald-400 flex items-center gap-1.5">
                                    <Shield className="w-3.5 h-3.5 text-emerald-400" />
                                    Pre-Flight Active
                                </div>
                            </div>
                            <div className="bg-zinc-950/60 border border-zinc-800/40 rounded-lg p-2.5">
                                <div className="text-[10px] font-mono uppercase text-zinc-500 mb-0.5">Last DOM Scan</div>
                                <div className="font-semibold text-zinc-300">
                                    {specialist_scan_at ? specialist_scan_at.toLocaleTimeString() : 'Pending Next Idle'}
                                </div>
                            </div>
                            <div className="bg-zinc-950/60 border border-zinc-800/40 rounded-lg p-2.5">
                                <div className="text-[10px] font-mono uppercase text-zinc-500 mb-0.5">Anomalies Detected</div>
                                <div className={`font-semibold ${specialist_anomalies > 0 ? 'text-rose-400' : 'text-zinc-300'}`}>
                                    {specialist_anomalies} escalated
                                </div>
                            </div>
                        </div>

                        {/* Quick Action Controls */}
                        <div className="flex items-center gap-2">
                            <button
                                onClick={handle_run_audit}
                                disabled={!sentinel_mode || isAuditing || specialist_status === 'thinking' || specialist_status === 'loading'}
                                className="flex-1 inline-flex items-center justify-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-semibold bg-cyan-500/10 hover:bg-cyan-500/20 text-cyan-300 border border-cyan-500/30 disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
                            >
                                {isAuditing || specialist_status === 'thinking' ? (
                                    <>
                                        <RotateCw className="w-3.5 h-3.5 animate-spin" />
                                        <span>Auditing...</span>
                                    </>
                                ) : (
                                    <>
                                        <Play className="w-3.5 h-3.5" />
                                        <span>Run Audit Now</span>
                                    </>
                                )}
                            </button>

                            <button
                                onClick={handle_reset_pipeline}
                                disabled={specialist_status === 'loading' || specialist_status === 'thinking'}
                                title="Purge cached WebGPU/WASM pipeline buffers and re-initialize"
                                className="inline-flex items-center justify-center gap-1 px-3 py-1.5 rounded-lg text-xs font-semibold bg-zinc-800/60 hover:bg-zinc-800 text-zinc-400 hover:text-zinc-200 border border-zinc-700/50 disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
                            >
                                <RefreshCcw className="w-3.5 h-3.5" />
                                <span>Reset</span>
                            </button>
                        </div>
                    </div>
                </div>

                {/* Right Column: Manifest & Logs */}
                <div className="lg:col-span-2 space-y-6">
                    {/* Manifest Terminal */}
                    <div className="bg-[#0a0a0a] border border-zinc-800/50 rounded-2xl flex flex-col min-h-[400px]">
                        <div className="flex items-center justify-between p-4 border-b border-zinc-800/50 bg-[#0d0d0d] rounded-t-2xl">
                            <div className="flex items-center gap-2">
                                <Terminal className="w-4 h-4 text-zinc-400" />
                                <h2 className="text-xs font-bold uppercase tracking-widest text-zinc-400">Sovereign State Manifest</h2>
                            </div>
                            <div className="flex gap-1.5">
                                <div className="w-2.5 h-2.5 rounded-full bg-zinc-800" />
                                <div className="w-2.5 h-2.5 rounded-full bg-zinc-800" />
                                <div className="w-2.5 h-2.5 rounded-full bg-zinc-800" />
                            </div>
                        </div>
                        
                        <div className="p-6 flex-grow font-mono text-sm overflow-auto max-h-[500px] leading-relaxed">
                            {loading && !manifest ? (
                                <div className="space-y-3">
                                    <div className="h-4 w-3/4 bg-zinc-800/50 rounded animate-pulse" />
                                    <div className="h-4 w-1/2 bg-zinc-800/50 rounded animate-pulse" />
                                    <div className="h-4 w-2/3 bg-zinc-800/50 rounded animate-pulse" />
                                    <div className="h-32 w-full bg-zinc-800/20 rounded animate-pulse mt-4" />
                                </div>
                            ) : (
                                <pre className="text-indigo-300/90 whitespace-pre-wrap">
                                    {manifest || 'Waiting for system pulse...'}
                                </pre>
                            )}
                        </div>

                        <div className="p-3 border-t border-zinc-800/50 bg-[#0d0d0d] rounded-b-2xl flex items-center justify-between">
                            <div className="flex items-center gap-4 text-[10px] text-zinc-600 font-mono uppercase">
                                <span className="flex items-center gap-1"><Zap className="w-3 h-3" /> Real-time Telemetry Injected</span>
                                <span>Encoding: UTF-8 / Sovereign</span>
                            </div>
                            <span className="text-[10px] text-zinc-700 font-mono italic">
                                SEC-01 Manifest Validation Active
                            </span>
                        </div>
                    </div>

                    {/* Governance Alerts */}
                    <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                        <div className="bg-amber-500/5 border border-amber-500/20 rounded-xl p-4 flex gap-4">
                            <AlertTriangle className="w-5 h-5 text-amber-500 shrink-0" />
                            <div>
                                <h4 className="text-sm font-bold text-amber-500 mb-1">Drift Detection</h4>
                                <p className="text-xs text-zinc-500 leading-normal">System is currently in Phase 5: Governance Loop. No critical drift detected in the last 24h cycle.</p>
                            </div>
                        </div>
                        <div className="bg-indigo-500/5 border border-indigo-500/20 rounded-xl p-4 flex gap-4">
                            <Lock className="w-5 h-5 text-indigo-500 shrink-0" />
                            <div>
                                <h4 className="text-sm font-bold text-indigo-500 mb-1">Policy Hardening</h4>
                                <p className="text-xs text-zinc-500 leading-normal">The Aletheia Triple-Gate is enforcing zero-trust tool execution for all Tier-2 specialists.</p>
                            </div>
                        </div>
                    </div>
                </div>
            </div>
        </div>
    );
}

// Metadata: [Governance_View]

import { useState, useEffect, useRef } from 'react';
import { system_api_service } from '../services/system_api_service';
import { use_settings_store } from '../stores/settings_store';
import { 
    Shield, 
    Activity, 
    CreditCard, 
    Terminal, 
    AlertTriangle, 
    RefreshCcw,
    Lock,
    Zap,
    Scale
} from 'lucide-react';

export default function Governance_View() {
    const [manifest, setManifest] = useState<string>('');
    const [quotas, setQuotas] = useState<any>(null);
    const [loading, setLoading] = useState(true);
    const [lastRefresh, setLastRefresh] = useState(new Date());
    const { settings } = use_settings_store();
    const isSyncingRef = useRef(false);

    const fetchGovernance = async () => {
        if (isSyncingRef.current) return;
        isSyncingRef.current = true;
        
        setLoading(true);
        try {
            const [m, q] = await Promise.all([
                system_api_service.get_sovereign_manifest(),
                system_api_service.get_security_quotas()
            ]);
            setManifest(m);
            setQuotas(q);
            setLastRefresh(new Date());
        } catch (error) {
            console.error('[Governance] Sync Failed:', error);
        } finally {
            setLoading(false);
            isSyncingRef.current = false;
        }
    };

    useEffect(() => {
        fetchGovernance();
        const interval = setInterval(fetchGovernance, 300000); // Sync every 5 minutes
        return () => clearInterval(interval);
    }, []);

    const spentPercentage = quotas ? (quotas.total_spent / quotas.total_budget) * 100 : 0;

    return (
        <div className="flex flex-col h-full bg-[#050505] text-zinc-100 p-6 overflow-y-auto custom-scrollbar">
            {/* Header */}
            <div className="flex items-center justify-between mb-8">
                <div className="flex items-center gap-3">
                    <div className="p-3 bg-indigo-500/10 border border-indigo-500/20 rounded-xl">
                        <Scale className="w-6 h-6 text-indigo-400" />
                    </div>
                    <div>
                        <h1 className="text-2xl font-bold tracking-tight">Sovereign Governance</h1>
                        <p className="text-zinc-500 text-sm">Orchestrating the Aletheia Protocol and Resource Boundaries</p>
                    </div>
                </div>
                <div className="flex items-center gap-4">
                    <div className="text-right">
                        <p className="text-[10px] text-zinc-600 uppercase font-mono tracking-widest">Last Intelligence Sync</p>
                        <p className="text-xs text-zinc-400 font-mono">{lastRefresh.toLocaleTimeString()}</p>
                    </div>
                    <button 
                        onClick={fetchGovernance}
                        className="p-2 hover:bg-white/5 rounded-lg transition-colors border border-zinc-800"
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
                                        ${quotas?.total_spent.toFixed(2) || '0.00'}
                                    </span>
                                    <span className="text-zinc-500 text-xs mb-1">
                                        of ${quotas?.total_budget.toFixed(2) || '0.00'} limit
                                    </span>
                                </div>
                                <div className="h-2 bg-zinc-900 rounded-full overflow-hidden">
                                    <div 
                                        className="h-full bg-gradient-to-r from-emerald-500 to-indigo-500 transition-all duration-1000"
                                        style={{ width: `${Math.min(spentPercentage, 100)}%` }}
                                    />
                                </div>
                            </div>
                            
                            <div className="grid grid-cols-2 gap-4 pt-4 border-t border-zinc-800/50">
                                <div>
                                    <p className="text-[10px] text-zinc-600 uppercase font-mono mb-1">Efficiency Score</p>
                                    <p className="text-lg font-mono text-emerald-400">{(quotas?.efficiency * 100).toFixed(1) || '0.0'}%</p>
                                </div>
                                <div>
                                    <p className="text-[10px] text-zinc-600 uppercase font-mono mb-1">Remaining</p>
                                    <p className="text-lg font-mono">${quotas?.remaining.toFixed(2) || '0.00'}</p>
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
                                    <div className="w-2 h-2 rounded-full bg-emerald-500 animate-pulse" />
                                    <span className="text-sm font-medium">Aletheia Verification</span>
                                    <div className={`p-1.5 rounded-lg border flex items-center gap-2 ${settings.is_safe_mode ? 'bg-emerald-500/10 border-emerald-500/20 text-emerald-400' : 'bg-red-500/10 border-red-500/20 text-red-400'}`}>
                                        <Shield size={12} />
                                        <span className="text-[10px] font-bold uppercase tracking-wider">{settings.is_safe_mode ? 'Verified' : 'Bypassed'}</span>
                                    </div>
                                    <span className="text-[10px] text-zinc-500 font-mono">OS: v1.1.57 | Mode: {settings.is_safe_mode ? 'Secure' : 'Unrestricted'}</span>
                                </div>
                            </div>

                            <div className="space-y-2">
                                <div className="flex justify-between text-[10px] uppercase font-mono text-zinc-600">
                                    <span>Merkle Chain Integrity</span>
                                    <span>{(quotas?.system_defense.merkle_integrity * 100).toFixed(2) || '100.00'}%</span>
                                </div>
                                <div className="h-1 bg-zinc-900 rounded-full overflow-hidden">
                                    <div 
                                        className="h-full bg-indigo-500/50" 
                                        style={{ width: '100%' }}
                                    />
                                </div>
                            </div>
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

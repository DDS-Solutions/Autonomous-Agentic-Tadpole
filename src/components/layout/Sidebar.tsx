/**
 * @docs ARCHITECTURE:Interface
 * @docs OPERATIONS_MANUAL:Navigation
 * 
 * ### AI Assist Note
 * **UI Shell Sidebar**: Core navigational component for Sector switching and Dashboard orchestration. 
 * Implements high-fidelity glassmorphism design with responsive collapse/expand triggers via `sidebar_store`.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Active route mismatch if `location` sync fails, button click hijacking on mobile viewports, or CSS layout shift during rapid collapse/expand.
 * - **Telemetry Link**: Search for `[Sidebar]` or `nav_item_click` in UI tracing.
 */

import { NavLink } from 'react-router-dom';
import { 
    LayoutDashboard, 
    Users, 
    Target, 
    BookOpen, 
    Settings, 
    Shield, 
    Clock, 
    Zap,
    Mic,
    FolderOpen,
    Cpu,
    Store,
    ShoppingBag,
    Activity,
    BarChart3
} from 'lucide-react';
import clsx from 'clsx';
import { Tooltip } from '../ui';
import { i18n } from '../../i18n';
import { use_settings_store } from '../../stores/settings_store';

interface SidebarProps {
    nav_item_class: (props: { isActive: boolean }) => string;
}

type NavEntry =
    | { divider: true; label?: string; expert_only?: boolean }
    | {
          to: string;
          icon: React.ElementType;
          label: string;
          tooltip: string;
          end?: boolean;
          focus_core?: boolean;
      };

export function Sidebar({ nav_item_class }: SidebarProps) {
    const cockpit_mode = use_settings_store((s) => s.settings.cockpit_mode ?? 'focus');
    const update_setting = use_settings_store((s) => s.update_setting);
    const is_expert = cockpit_mode === 'expert';

    const NAV_ITEMS: NavEntry[] = [
        { to: '/dashboard', icon: LayoutDashboard, label: 'layout.sidebar.ops_nav', tooltip: 'layout.sidebar.ops_tooltip', end: true, focus_core: true },
        { to: '/missions', icon: Target, label: 'layout.sidebar.missions_nav', tooltip: 'layout.sidebar.missions_tooltip', focus_core: true },
        { to: '/oversight', icon: Shield, label: 'layout.sidebar.oversight_nav', tooltip: 'layout.sidebar.oversight_tooltip', focus_core: true },
        { to: '/agents', icon: Users, label: 'layout.nav.agents', tooltip: 'layout.nav.agents_tooltip', focus_core: true },
        { to: '/models', icon: Cpu, label: 'layout.nav.providers', tooltip: 'layout.nav.providers_tooltip', focus_core: true },
        { divider: true, label: 'Governance', expert_only: true },
        { to: '/org-chart', icon: Users, label: 'layout.sidebar.hierarchy_nav', tooltip: 'layout.sidebar.hierarchy_tooltip' },
        { to: '/governance', icon: Shield, label: 'Governance', tooltip: 'Sovereign State Manifest & Swarm Health' },
        { to: '/scheduled-jobs', icon: Clock, label: 'layout.sidebar.scheduled_jobs_nav', tooltip: 'layout.sidebar.scheduled_jobs_tooltip' },
        { to: '/security', icon: Shield, label: 'layout.sidebar.security_nav', tooltip: 'layout.sidebar.security_tooltip' },
        { divider: true, label: 'Workspace', expert_only: true },
        { to: '/standups', icon: Mic, label: 'layout.nav.voice_interface', tooltip: 'layout.nav.voice_interface_standups' },
        { to: '/workspaces', icon: FolderOpen, label: 'layout.nav.workspaces', tooltip: 'layout.nav.file_system_workspaces' },
        { divider: true, label: 'Infrastructure', expert_only: true },
        { to: '/infra/model-store', icon: Store, label: 'layout.nav.model_store', tooltip: 'layout.nav.model_store_tooltip' },
        { to: '/store', icon: ShoppingBag, label: 'layout.nav.template_store', tooltip: 'layout.nav.template_store_tooltip' },
        { to: '/skills', icon: Settings, label: 'layout.nav.skills', tooltip: 'layout.nav.skills_tooltip' },
        { to: '/engine', icon: Activity, label: 'layout.nav.telemetry', tooltip: 'layout.nav.telemetry_tooltip' },
        { to: '/benchmarks', icon: BarChart3, label: 'layout.nav.performance', tooltip: 'layout.nav.performance_tooltip' },
    ];

    const visible_items = is_expert
        ? NAV_ITEMS
        : NAV_ITEMS.filter((item) => !('divider' in item) && item.focus_core);

    return (
        <aside className="w-16 lg:w-[260px] bg-zinc-950/30 backdrop-blur-md border-r border-zinc-900/50 flex flex-col z-20 transition-all duration-300">
            <div className="p-4 lg:p-6 border-b border-zinc-900/50 flex items-center gap-3 justify-center lg:justify-start">
                <Tooltip content={i18n.t('layout.sidebar.root_tooltip')} position="right">
                    <div className="w-10 h-10 rounded-xl flex items-center justify-center overflow-hidden bg-zinc-900/50 border border-zinc-800 shrink-0 group cursor-pointer">
                        <Zap
                            size={20}
                            aria-label={i18n.t('layout.sidebar.brand_name')}
                            className="text-zinc-100 transition-transform duration-500 group-hover:scale-110"
                        />
                    </div>
                </Tooltip>
                <span className="font-bold text-lg tracking-tight hidden lg:block text-zinc-100">{i18n.t('layout.sidebar.brand_name')}</span>
            </div>

            <nav className="flex-1 p-3 flex flex-col gap-1 mt-4 overflow-y-auto custom-scrollbar">
                {visible_items.map((item, index) => {
                    if ('divider' in item) {
                        return (
                            <div key={`divider-${index}`} className="pt-3 pb-1 border-t border-zinc-900 mx-2 mt-1">
                                {item.label && (
                                    <span className="hidden lg:block text-[9px] font-mono font-bold uppercase tracking-widest text-zinc-600 px-1">
                                        {item.label}
                                    </span>
                                )}
                            </div>
                        );
                    }
                    return (
                        <Tooltip key={item.to} content={i18n.t(item.tooltip || '')} position="right">
                            <NavLink to={item.to || ''} end={item.end} className={nav_item_class}>
                                <item.icon size={18} />
                                <span className="hidden lg:block">{i18n.t(item.label || '')}</span>
                            </NavLink>
                        </Tooltip>
                    );
                })}

                {is_expert && (
                    <>
                        <div className="pt-2 border-t border-zinc-900 mx-2"></div>

                        <Tooltip content={i18n.t('layout.sidebar.docs_tooltip')} position="right">
                            <NavLink to="/docs" className={nav_item_class}>
                                <BookOpen size={18} />
                                <span className="hidden lg:block">{i18n.t('layout.sidebar.docs_nav')}</span>
                            </NavLink>
                        </Tooltip>
                    </>
                )}
            </nav>

            {/* Cockpit Mode Progressive Disclosure Toggle (Focus vs Expert) */}
            <div className="px-4 py-2.5 border-t border-zinc-900/60 hidden lg:block">
                <div
                    role="group"
                    aria-label="Cockpit Mode"
                    className="grid grid-cols-2 gap-1 p-1 rounded-lg bg-zinc-950/80 border border-zinc-800/80"
                >
                    <button
                        type="button"
                        onClick={() => update_setting('cockpit_mode', 'focus')}
                        title="Focus Mode — 6 core Overlord views"
                        aria-label="Focus Mode"
                        aria-pressed={!is_expert}
                        data-testid="sidebar-mode-focus"
                        className={clsx(
                            'px-2 py-1 rounded-md text-[10px] font-mono font-bold uppercase tracking-widest transition-colors focus:outline-none focus:ring-2 focus:ring-emerald-500',
                            !is_expert ? 'bg-zinc-800 text-zinc-100 border border-zinc-700/60' : 'text-zinc-500 hover:text-zinc-300'
                        )}
                    >
                        Focus
                    </button>
                    <button
                        type="button"
                        onClick={() => update_setting('cockpit_mode', 'expert')}
                        title="Expert Mode — all 18 system views (Ctrl+Shift+E)"
                        aria-label="Expert Mode"
                        aria-pressed={is_expert}
                        data-testid="sidebar-mode-expert"
                        className={clsx(
                            'px-2 py-1 rounded-md text-[10px] font-mono font-bold uppercase tracking-widest transition-colors focus:outline-none focus:ring-2 focus:ring-emerald-500',
                            is_expert ? 'bg-zinc-800 text-zinc-100 border border-zinc-700/60' : 'text-zinc-500 hover:text-zinc-300'
                        )}
                    >
                        Expert
                    </button>
                </div>
            </div>

            {/* Social Proof / Certification Badge */}
            <div className="p-4 border-t border-zinc-900/50 bg-zinc-950/20 backdrop-blur-sm hidden lg:block">
                <Tooltip content={i18n.t('layout.sidebar.node_v_tooltip')} position="top">
                    <div className="flex items-center gap-2 p-3 rounded-xl bg-emerald-500/5 border border-emerald-500/10 hover:border-emerald-500/30 transition-all group cursor-default">
                        <Shield className="w-5 h-5 text-emerald-500 group-hover:animate-pulse" />
                        <div className="flex flex-col">
                            <span className="text-[9px] font-bold text-emerald-500 uppercase tracking-widest leading-none">{i18n.t('layout.sidebar.node_certified')}</span>
                            <span className="text-[8px] text-zinc-500 font-mono mt-1">{i18n.t('layout.sidebar.node_version')}</span>
                        </div>
                    </div>
                </Tooltip>
            </div>

            <div className="p-4 border-t border-zinc-800">
                <Tooltip content={i18n.t('layout.sidebar.settings_tooltip')} position="right">
                    <NavLink to="/settings" className={({ isActive: is_active }) => clsx(
                        "flex items-center gap-3 p-2 rounded-md font-medium cursor-pointer transition-all duration-200 text-sm justify-center lg:justify-start",
                        is_active ? "bg-zinc-800 text-zinc-100 border border-zinc-700/50" : "text-zinc-500 hover:bg-zinc-800/50 hover:text-zinc-300"
                    )}>
                        <Settings size={18} />
                        <span className="hidden lg:block">{i18n.t('layout.sidebar.settings_nav')}</span>
                    </NavLink>
                </Tooltip>
            </div>
        </aside>
    );
}

// Metadata: [Sidebar]

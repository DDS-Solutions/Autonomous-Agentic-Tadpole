/**
 * @docs ARCHITECTURE:Interface
 * @docs OPERATIONS_MANUAL:Navigation
 * 
 * ### AI Assist Note
 * **Root Layout**: Orchestrates the primary dashboard container, sidebar, and tabbed navigation interface. 
 * Manages the high-fidelity backdrop blur effects and Z-index layering for global components (Terminal, Detached Windows). 
 * Integrates `tab_store` for multi-tab state persistence and `sidebar_store` for layout responsiveness.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Viewport overflow (if sidebar is pinned on mobile), tab sync mismatch (BroadcastChannel data collision), or layout layout flickering during page transitions.
 * - **Telemetry Link**: Watch for `tab_store` mutations or search `[DashboardLayout]` in UI traces.
 */

import React, { useEffect, useState, Suspense, lazy } from 'react';
import { useLocation, useNavigate } from 'react-router-dom';
import clsx from 'clsx';
import { tadpole_os_socket } from '../services/socket';
import { use_skill_store } from '../stores/skill_store';
import { Sidebar } from '../components/layout/Sidebar';
import { Portal_Window } from '../components/ui';
import { Tab_Bar } from '../components/layout/Tab_Bar';
import { Ticker_Header } from '../components/layout/Ticker_Header';
import { use_tab_store } from '../stores/tab_store';
import { get_route_by_path } from '../constants/routes';
import Error_Boundary from '../components/Error_Boundary';
import { ExternalLink } from 'lucide-react';
import { i18n } from '../i18n';
import { use_notification_store } from '../stores/notification_store';
import { event_bus } from '../services/event_bus';
import { use_agent_registry_store } from '../stores/agent_store';
import { APP_ROUTES } from '../constants/routes';

const SovereignChat = lazy(() => import('../components/SovereignChat').then(module => ({ default: module.SovereignChat })));
const Command_Palette = lazy(() => import('../components/Command_Palette').then(module => ({ default: module.Command_Palette })));
const Observability_Sidebar = lazy(() => import('../components/layout/Observability_Sidebar').then(module => ({ default: module.Observability_Sidebar })));
const Toast_Center = lazy(() => import('../components/ui/Toast_Center').then(module => ({ default: module.Toast_Center })));

/**
 * Dashboard_Layout
 * The primary architectural shell for the Tadpole OS frontend.
 * Orchestrates global services, navigation, and multi-window workspace synchronization.
 */
export default function Dashboard_Layout() {
    const location = useLocation();
    const navigate = useNavigate();

    // Use granular selectors to prevent redundant re-renders of the entire layout shell
    const tabs = use_tab_store(s => s.tabs);
    const active_tab_id = use_tab_store(s => s.active_tab_id);
    const active_tab_sync_source = use_tab_store(s => s.active_tab_sync_source);
    const is_system_log_detached = use_tab_store(s => s.is_system_log_detached);
    const toggle_system_log_detachment = use_tab_store(s => s.toggle_system_log_detachment);
    const is_trace_stream_detached = use_tab_store(s => s.is_trace_stream_detached);
    const toggle_trace_stream_detachment = use_tab_store(s => s.toggle_trace_stream_detachment);
    const is_lineage_stream_detached = use_tab_store(s => s.is_lineage_stream_detached);
    const toggle_lineage_stream_detachment = use_tab_store(s => s.toggle_lineage_stream_detachment);

    const [is_command_palette_open, set_is_command_palette_open] = useState(false);
    const tab_snapshot = (
        tabs &&
        typeof tabs === 'object' &&
        !Array.isArray(tabs) &&
        'tabs' in tabs
    )
        ? tabs as unknown as {
            tabs?: unknown;
            active_tab_id?: unknown;
            is_system_log_detached?: unknown;
            is_trace_stream_detached?: unknown;
            is_lineage_stream_detached?: unknown;
        }
        : null;
    const safe_tabs = Array.isArray(tabs)
        ? tabs
        : (Array.isArray(tab_snapshot?.tabs) ? tab_snapshot.tabs : []);
    const safe_active_tab_id = typeof active_tab_id === 'string'
        ? active_tab_id
        : (typeof tab_snapshot?.active_tab_id === 'string' ? tab_snapshot.active_tab_id : null);
    const safe_is_system_log_detached = typeof is_system_log_detached === 'boolean'
        ? is_system_log_detached
        : tab_snapshot?.is_system_log_detached === true;
    const safe_is_trace_stream_detached = typeof is_trace_stream_detached === 'boolean'
        ? is_trace_stream_detached
        : tab_snapshot?.is_trace_stream_detached === true;
    const safe_is_lineage_stream_detached = typeof is_lineage_stream_detached === 'boolean'
        ? is_lineage_stream_detached
        : tab_snapshot?.is_lineage_stream_detached === true;

    // Synchronize tab store with URL on first load and browser navigation
    useEffect(() => {
        // Guard: Detached windows should never trigger a navigation update
        if (location.pathname.startsWith('/detached')) {
            return;
        }

        const active_tab = safe_tabs.find(t => t.id === safe_active_tab_id);
        if (active_tab && active_tab.path !== location.pathname) {
            // Only navigate if the change came from the UI (click) or a remote SYNC (other tab).
            // If the source is 'url', it means the URL already matches (or is the driver), so we skip to avoid loops.
            if (active_tab_sync_source === 'ui' || active_tab_sync_source === 'sync') {
                const target_path = active_tab.path === '/' ? '/dashboard' : active_tab.path.replace(/\/$/, '');
                const current_path = location.pathname === '/' ? '/dashboard' : location.pathname.replace(/\/$/, '');
                
                if (target_path !== current_path) {
                    console.debug('[DashboardLayout] Syncing navigation to active tab (Source:', active_tab_sync_source, '):', target_path);
                    navigate(target_path);
                }
            }
        }
    }, [safe_active_tab_id, location.pathname, navigate, safe_tabs, active_tab_sync_source]);

    // Synchronize Tab Store with URL (URL -> Tab)
    useEffect(() => {
        // Guard: Detached windows should never trigger a store update from URL
        if (location.pathname.startsWith('/detached')) {
            return;
        }

        const normalized_path = location.pathname === '/' ? '/dashboard' : location.pathname.replace(/\/$/, '');
        const route = APP_ROUTES.find(r => r.path === normalized_path);
        
        if (route) {
            if (typeof use_tab_store.getState !== 'function') {
                return;
            }
            const { tabs: current_tabs, active_tab_id: current_active_id, open_tab } = use_tab_store.getState();
            const active_tab = (Array.isArray(current_tabs) ? current_tabs : []).find(t => t.id === current_active_id);
            
            // Only open/switch tab if the URL doesn't match the current active tab
            if (!active_tab || active_tab.path !== normalized_path) {
                console.debug('[DashboardLayout] URL changed, updating tab state for:', normalized_path);
                open_tab({
                    title: i18n.t(route.label) || route.label,
                    path: normalized_path,
                    icon: route.icon
                });
            }
        }
    }, [location.pathname]);

    // ── Connection Status ──────────────────────────────────
    useEffect(() => {
        tadpole_os_socket.connect();
        const unsubscribe_pulse = tadpole_os_socket.subscribe_pulse((pulse) => {
            use_skill_store.getState().handle_pulse(pulse.tool, pulse.status, pulse.latency);
        });
        return () => {
            unsubscribe_pulse();
        };
    }, []);

    useEffect(() => {
        const handle_refresh_agents = () => {
            void use_agent_registry_store.getState().fetch_agents();
        };
        window.addEventListener('app:refresh-agents', handle_refresh_agents);
        return () => window.removeEventListener('app:refresh-agents', handle_refresh_agents);
    }, []);

    // ── Unified Notification Hub ───────────────────────────
    useEffect(() => {
        const { add_notification } = use_notification_store.getState();

        const unsubscribe = event_bus.subscribe_logs((entry) => {
            // Only pipe errors or specifically tagged security/governance events to the Hub
            const is_high_priority = 
                entry.severity === 'error' || 
                entry.severity === 'warning' ||
                entry.text.toLowerCase().includes('budget') ||
                entry.text.toLowerCase().includes('security') ||
                entry.text.toLowerCase().includes('injection');

            if (is_high_priority) {
                const is_persistent = 
                    entry.text.toLowerCase().includes('budget') || 
                    entry.text.toLowerCase().includes('injection') ||
                    entry.text.toLowerCase().includes('sanitizer');

                const title = entry.source === 'Agent' 
                    ? (entry.agent_name || entry.agent_id ? `Agent Alert: ${entry.agent_name || entry.agent_id}` : 'Agent Alert')
                    : 'System Alert';

                add_notification({
                    severity: entry.severity,
                    title,
                    message: entry.text,
                    type_id: entry.metadata?.type_id as string,
                    persistent: is_persistent,
                });
            }
        });

        return unsubscribe;
    }, []);

    // ── Keyboard Shortcuts ──────────────────────────────────
    useEffect(() => {
        const handle_key_down = (e: KeyboardEvent) => {
            const tag = (e.target as HTMLElement)?.tagName;
            if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return;

            if ((e.ctrlKey || e.metaKey) && (e.key === 'k' || e.key === '/')) {
                e.preventDefault();
                set_is_command_palette_open(prev => !prev);
                return;
            }

            if (!e.ctrlKey && !e.metaKey && !e.altKey) {
                const routes: Record<string, string> = {
                    '1': '/',
                    '2': '/org-chart',
                    '3': '/standups',
                    '4': '/workspaces',
                    '5': '/docs',
                    '6': '/settings',
                };
                if (routes[e.key]) {
                    navigate(routes[e.key]);
                    return;
                }
            }
        };
        window.addEventListener('keydown', handle_key_down);
        return () => window.removeEventListener('keydown', handle_key_down);
    }, [navigate]);

    const nav_item_class = ({ isActive: is_active }: { isActive: boolean }) => clsx(
        "flex items-center gap-3 p-2 rounded-md font-medium cursor-pointer transition-all duration-200 text-sm",
        is_active
            ? "bg-zinc-800 text-zinc-100 border border-zinc-700/50"
            : "text-zinc-500 hover:bg-zinc-800/50 hover:text-zinc-300"
    );

    const shell_fallback = (
        <div className="p-3 text-zinc-600 font-mono text-[10px] uppercase tracking-widest animate-pulse">
            {i18n.t('common.loading')}
        </div>
    );
    return (
        <div className="flex h-screen bg-zinc-950 text-zinc-100 overflow-hidden font-sans antialiased selection:bg-zinc-700/30">
            <a href="#main-content" className="sr-only focus:not-sr-only focus:absolute focus:top-4 focus:left-4 z-[9999] bg-emerald-600 text-white px-4 py-2 rounded">
                {i18n.t('layout.skip_to_content') || 'Skip to content'}
            </a>
            <Sidebar nav_item_class={nav_item_class} />

            <main id="main-content" className="flex-1 flex flex-col bg-zinc-950 relative overflow-hidden">
                <Ticker_Header />

                <Tab_Bar />

                <div className="flex-1 flex overflow-hidden">
                    <div className="flex-1 relative">
                        {safe_tabs.map((tab) => {
                            const route = get_route_by_path(tab.path);
                            const Component = route.component;
                            const is_detached = tab.is_detached;
                            const is_active = tab.id === safe_active_tab_id;
                            if (!is_active && !is_detached) {
                                return null;
                            }

                            const content = (
                                <Error_Boundary name={`Sector: ${tab.title}`}>
                                    <Suspense fallback={<div className="p-8 text-zinc-500 font-mono text-xs animate-pulse">{i18n.t('layout.initializing_sector')}</div>}>
                                        <Component />
                                    </Suspense>
                                </Error_Boundary>
                            );

                            if (is_detached) {
                                return (
                                    <React.Fragment key={tab.id}>
                                        <Portal_Window 
                                            id={tab.id} 
                                            title={tab.title} 
                                            url={`/detached-view?type=tab&path=${encodeURIComponent(tab.path)}&id=${tab.id}`}
                                            on_close={() => use_tab_store.getState().toggle_tab_detachment(tab.id)}
                                        >
                                            {/* 
                                              * [NEURAL_RECOVERY]: We no longer portal children if a URL is used. 
                                              * This prevents double-mounting in React 19 which causes the 'null useState' fault.
                                              */}
                                            <div className="hidden" aria-hidden="true" />
                                        </Portal_Window>

                                        <div
                                            className={clsx(
                                                "absolute inset-0 flex items-center justify-center bg-zinc-950/80 backdrop-blur-sm z-20",
                                                is_active ? "visible opacity-100" : "invisible opacity-0"
                                            )}
                                        >
                                            <div className="text-center space-y-4">
                                                <div className="relative inline-block">
                                                    <ExternalLink size={48} className="text-zinc-800 animate-pulse" />
                                                    <div className="absolute inset-0 bg-green-500/10 blur-xl rounded-full" />
                                                </div>
                                                <div className="space-y-1">
                                                    <h3 className="text-lg font-bold tracking-tight text-zinc-200">{i18n.t('layout.sector_detached')}</h3>
                                                    <p className="text-sm text-zinc-500 font-mono">{i18n.t('layout.link_established')} :: {i18n.t('layout.monitor_label')}_{tab.id.toUpperCase()}</p>
                                                </div>
                                                <button 
                                                    onClick={() => use_tab_store.getState().toggle_tab_detachment(tab.id)}
                                                    className="px-4 py-2 bg-zinc-800 text-zinc-100 text-xs font-bold uppercase tracking-widest rounded-lg border border-zinc-700 hover:bg-zinc-700 transition-all active:scale-95"
                                                >
                                                    {i18n.t('layout.recall_sector')}
                                                </button>
                                            </div>
                                        </div>
                                    </React.Fragment>
                                );
                            }

                            return (
                                <div
                                    key={tab.id}
                                    className={clsx(
                                        "absolute inset-0 overflow-y-auto overflow-x-hidden p-0 custom-scrollbar",
                                        is_active ? "visible opacity-100 z-10" : "invisible opacity-0 z-0 pointer-events-none"
                                    )}
                                >
                                    {content}
                                </div>
                            );
                        })}
                    </div>

                    <Suspense fallback={shell_fallback}>
                        <Observability_Sidebar />
                    </Suspense>

                    <Suspense fallback={null}>
                        <SovereignChat />
                        <Command_Palette
                            is_open={is_command_palette_open}
                            on_close={() => set_is_command_palette_open(false)}
                        />
                    </Suspense>

                    {safe_is_system_log_detached && (
                        <Portal_Window
                            id="system-log-detached"
                            title={i18n.t('dashboard.log_title')}
                            url="/detached-view?type=system-log"
                            on_close={toggle_system_log_detachment}
                        >
                            <div className="hidden" aria-hidden="true" />
                        </Portal_Window>
                    )}

                    {safe_is_trace_stream_detached && (
                        <Portal_Window
                            id="trace-stream-detached"
                            title={i18n.t('trace_stream.title')}
                            url="/detached-view?type=trace-stream"
                            on_close={toggle_trace_stream_detachment}
                        >
                            <div className="hidden" aria-hidden="true" />
                        </Portal_Window>
                    )}

                    {safe_is_lineage_stream_detached && (
                        <Portal_Window
                            id="lineage-stream-detached"
                            title={i18n.t('trace.stream_title')}
                            url="/detached-view?type=lineage-stream"
                            on_close={toggle_lineage_stream_detachment}
                        >
                            <div className="hidden" aria-hidden="true" />
                        </Portal_Window>
                    )}
                </div>

                <Suspense fallback={null}>
                    <Toast_Center />
                </Suspense>
            </main>
        </div>
    );
}

// Metadata: [Dashboard_Layout]
// Metadata: [Dashboard_Layout]

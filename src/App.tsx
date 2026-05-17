/**
 * @docs ARCHITECTURE:Interface
 * 
 * ### AI Assist Note
 * **Root Application Kernel**: Top-level React entry point that orchestrates routing, global state synchronization, and the `Dashboard_Layout` shell. 
 * Manages the unified theme injection, default provider syncing, and URL-to-Tab state reconciliation.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Route dead-loops (if `TabSync` logic fails), theme injection flicker, or `Error_Boundary` catch during lazy-loading.
 * - **Telemetry Link**: Look for `Global OS Hub` in error traces or search `[AppKernel]` in component logs.
 */

import { useEffect, Suspense, useRef } from 'react';
import { BrowserRouter as Router, Routes, Route, useLocation, Navigate } from 'react-router-dom';
import { use_provider_store } from './stores/provider_store';
import { get_settings } from './stores/settings_store';
import { use_tab_store } from './stores/tab_store';
import { get_route_by_path } from './constants/routes';
import Dashboard_Layout from './layouts/Dashboard_Layout';
import Error_Boundary from './components/Error_Boundary';
import { lazy } from 'react';

const Detached_Swarm_Pulse = lazy(() => import('./pages/Detached_Swarm_Pulse'));
const Detached_Shell = lazy(() => import('./pages/detached_shell'));
import { SovereignChat } from './components/SovereignChat';

import { i18n } from './i18n';

function RouteLoading(): React.ReactElement {
  return (
    <div className="h-full flex flex-col items-center justify-center gap-4 p-8">
      <div className="w-full max-w-2xl space-y-3">
        <div className="h-6 w-48 bg-zinc-800 rounded animate-pulse" />
        <div className="h-4 w-full bg-zinc-800/60 rounded animate-pulse" />
        <div className="h-4 w-5/6 bg-zinc-800/40 rounded animate-pulse" />
        <div className="h-32 w-full bg-zinc-800/30 rounded-lg animate-pulse mt-4" />
        <div className="flex gap-3 mt-4">
          <div className="h-10 w-28 bg-zinc-800/50 rounded animate-pulse" />
          <div className="h-10 w-28 bg-zinc-800/50 rounded animate-pulse" />
        </div>
      </div>
      <span className="text-zinc-600 animate-pulse font-mono text-xs uppercase tracking-widest mt-4">
        {i18n.t('common.loading')}
      </span>
    </div>
  );
}

// Internal component to sync URL with Tab Store
function TabSync(): null {
  const location = useLocation();
  const open_tab = use_tab_store(s => s.open_tab); 
  const last_synced_path = useRef<string | null>(null);

  useEffect(() => {
    // 1. Guard: If we are in a detached context, never sync the tab store
    if (location.pathname.startsWith('/detached')) {
      return;
    }

    const route = get_route_by_path(location.pathname);
    if (!route) return;

    // Normalize paths consistently
    const current_path = location.pathname === '/' ? '/dashboard' : location.pathname.replace(/\/$/, '');

    // Skip if already synced to this URL to break feedback loops
    if (last_synced_path.current === current_path) {
      return;
    }

    // Peak into store state to check if current active tab already matches this path
    // We use getState() to avoid making 'tabs' or 'active_tab_id' a dependency of this effect.
    const state = use_tab_store.getState();
    const active_tab = (state.tabs || []).find(t => t.id === state.active_tab_id);
    const active_path = active_tab ? (active_tab.path === '/' ? '/dashboard' : active_tab.path.replace(/\/$/, '')) : null;

    if (active_path === current_path) {
      last_synced_path.current = current_path;
      return;
    }

    console.debug('[TabSync] URL changed. Synchronizing workspace:', current_path);
    last_synced_path.current = current_path;
    
    open_tab({
        title: route.label,
        path: route.path,
        icon: route.icon
    });
  }, [location.pathname, open_tab]);

  return null;
}

export default function App(): React.ReactElement {
  const sync_defaults = use_provider_store(state => state.sync_defaults);
  const sync_with_backend = use_provider_store(state => state.sync_with_backend);

  const is_init_ref = useRef(false);

  useEffect(() => {
    if (is_init_ref.current) return;
    is_init_ref.current = true;

    const init = async () => {
      try {
        console.debug('[AppKernel] Initializing OS runtime...');
        sync_defaults();
        await sync_with_backend();
        
        // --- Phase 2: Visual Monitoring ---
        const { visual_monitor_bridge } = await import('./services/visual_monitor_bridge');
        visual_monitor_bridge.init();

        // --- Phase 3: Resource Governance ---
        // Start VRAM/memory pressure polling so the browser inference
        // Resource Guard has live data before any inference is attempted.
        const { vram_monitor_service } = await import('./services/vram_monitor');
        vram_monitor_service.start();
        console.debug('[AppKernel] VRAM monitor started.');

        // --- Phase 4: Browser Inference Pre-Warm ---
        // Only pre-warm if sentinel_mode is already enabled in persisted settings.
        // This avoids a surprise 2GB download for users who never enabled the feature.
        const settings = get_settings();
        if (settings.sentinel_mode) {
          const { browser_inference_service } = await import('./services/browser_inference');
          browser_inference_service.pre_warm();
          console.debug('[AppKernel] Browser specialist pre-warm initiated.');
        }

        // --- Phase 5: Agent Registry Hydration ---
        // Finalize the boot sequence by fetching the latest agent configurations.
        const { use_agent_registry_store } = await import('./stores/agent_store');
        void use_agent_registry_store.getState().fetch_agents();
        console.debug('[AppKernel] Agent registry hydration initiated.');
      } catch (err) {
        console.error('[AppKernel] Critical Initialization Failure:', err);
      }
    };
    
    void init();

    const settings = get_settings();
    if (document.documentElement.getAttribute('data-theme') !== settings.theme) {
      document.documentElement.setAttribute('data-theme', settings.theme);
    }
    if (document.documentElement.getAttribute('data-density') !== settings.density) {
      document.documentElement.setAttribute('data-density', settings.density);
    }
  }, [sync_defaults, sync_with_backend]);


  return (
    <Router>
      <TabSync />
      <Error_Boundary name="Global OS Hub">
        <Suspense fallback={<RouteLoading />}>
          <Routes>
            {/* Main Application Layout */}
            <Route path="/*" element={<Dashboard_Layout />}>
              {/* 
                We redirect / to /dashboard specifically.
                All other paths (e.g. /agents, /telemetry) are handled 
                by Dashboard_Layout's custom tab system. 
              */}
              <Route index element={<Navigate to="/dashboard" replace />} />
            </Route>

            {/* Detached Windows (No Layout) */}
            <Route path="/detached-view" element={<Detached_Shell />} />
            <Route path="/detached/swarm-pulse" element={<Detached_Swarm_Pulse />} />
            <Route path="/detached/chat" element={<SovereignChat isDetachedView={true} />} />

            {/* Error Fallback */}
            <Route path="*" element={<Navigate to="/dashboard" replace />} />
          </Routes>
        </Suspense>
      </Error_Boundary>
    </Router>
  );
}


// Metadata: [App]

// Metadata: [App]

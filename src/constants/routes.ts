/**
 * @docs ARCHITECTURE:Interface
 * 
 * ### AI Assist Note
 * **Root Navigation Registry**: Map of all interactive dashboard coordinates and their corresponding components. 
 * Orchestrates lazy-loading of domain pages (Missions, Engine, Oversight) to minimize initial bundle footprint and ensure responsive transitions.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: 404 Route mismatch (if `get_route_by_path` fallback fails), or ChunkLoadError during lazy-loading on slow networks.
 * - **Telemetry Link**: Search `[routes]` in console logs.
 */

import { lazy } from 'react';

// Lazy-loaded page factories with pre-fetching support
const page_imports: Record<string, () => Promise<unknown>> = {
  '/dashboard': () => import('../pages/Ops_Dashboard'),
  '/org-chart': () => import('../pages/Org_Chart'),
  '/standups': () => import('../pages/Standups'),
  '/workspaces': () => import('../pages/Workspaces'),
  '/missions': () => import('../pages/Missions'),
  '/models': () => import('../pages/Model_Manager'),
  '/agents': () => import('../pages/Agent_Manager'),
  '/engine': () => import('../pages/Engine_Dashboard'),
  '/oversight': () => import('../pages/Oversight_Dashboard'),
  '/skills': () => import('../pages/Skills'),
  '/benchmarks': () => import('../pages/Benchmark_Analytics'),
  '/scheduled-jobs': () => import('../pages/Scheduled_Jobs'),
  '/infra/model-store': () => import('../pages/Model_Store'),
  '/docs': () => import('../pages/Docs'),
  '/settings': () => import('../pages/Settings'),
  '/store': () => import('../pages/Template_Store'),
  '/security': () => import('../pages/Security_Dashboard'),
  '/governance': () => import('../pages/Governance_View'),
};

const Org_Chart = lazy(page_imports['/org-chart'] as () => Promise<{ default: React.ComponentType<object> }>);
const Standups = lazy(page_imports['/standups'] as () => Promise<{ default: React.ComponentType<object> }>);
const Workspaces = lazy(page_imports['/workspaces'] as () => Promise<{ default: React.ComponentType<object> }>);
const Docs = lazy(page_imports['/docs'] as () => Promise<{ default: React.ComponentType<object> }>);
const Settings = lazy(page_imports['/settings'] as () => Promise<{ default: React.ComponentType<object> }>);
const Oversight_Dashboard = lazy(page_imports['/oversight'] as () => Promise<{ default: React.ComponentType<object> }>);
const Model_Manager = lazy(page_imports['/models'] as () => Promise<{ default: React.ComponentType<object> }>);
const Agent_Manager = lazy(page_imports['/agents'] as () => Promise<{ default: React.ComponentType<object> }>);
const Engine_Dashboard = lazy(page_imports['/engine'] as () => Promise<{ default: React.ComponentType<object> }>);
const Missions = lazy(page_imports['/missions'] as () => Promise<{ default: React.ComponentType<object> }>);
const Skills = lazy(page_imports['/skills'] as () => Promise<{ default: React.ComponentType<object> }>);
const Benchmark_Analytics = lazy(page_imports['/benchmarks'] as () => Promise<{ default: React.ComponentType<object> }>);
const Scheduled_Jobs = lazy(page_imports['/scheduled-jobs'] as () => Promise<{ default: React.ComponentType<object> }>);
const Template_Store = lazy(page_imports['/store'] as () => Promise<{ default: React.ComponentType<object> }>);
const Security_Dashboard = lazy(page_imports['/security'] as () => Promise<{ default: React.ComponentType<object> }>);
const Ops_Dashboard = lazy(page_imports['/dashboard'] as () => Promise<{ default: React.ComponentType<object> }>);
const Model_Store = lazy(page_imports['/infra/model-store'] as () => Promise<{ default: React.ComponentType<object> }>);
const Governance_View = lazy(page_imports['/governance'] as () => Promise<{ default: React.ComponentType<object> }>);

/** Pre-fetches all route chunks in the background during idle cycles. */
export const preload_all_routes = (): void => {
  if (typeof window === 'undefined') return;
  const runner = () => {
    Object.values(page_imports).forEach(imp => { void imp(); });
  };
  if ('requestIdleCallback' in window) {
    window.requestIdleCallback(runner);
  } else {
    setTimeout(runner, 1000);
  }
};

export interface Route_Config {
  path: string;
  component: React.ComponentType<object>;
  label: string;
  icon?: string;
}

export const APP_ROUTES: Route_Config[] = [
  { path: '/dashboard', component: Ops_Dashboard, label: 'Operations', icon: 'LayoutDashboard' },
  { path: '/org-chart', component: Org_Chart, label: 'Hierarchy', icon: 'Users' },
  { path: '/standups', component: Standups, label: 'Standups', icon: 'MessagesSquare' },
  { path: '/workspaces', component: Workspaces, label: 'Workspaces', icon: 'Grid' },
  { path: '/missions', component: Missions, label: 'Missions', icon: 'Target' },
  { path: '/models', component: Model_Manager, label: 'Models', icon: 'Cpu' },
  { path: '/agents', component: Agent_Manager, label: 'Agents', icon: 'Bot' },
  { path: '/engine', component: Engine_Dashboard, label: 'Engine', icon: 'Zap' },
  { path: '/oversight', component: Oversight_Dashboard, label: 'Oversight', icon: 'Shield' },
  { path: '/skills', component: Skills, label: 'Skills', icon: 'Wrench' },
  { path: '/benchmarks', component: Benchmark_Analytics, label: 'Benchmarks', icon: 'BarChart' },
  { path: '/scheduled-jobs', component: Scheduled_Jobs, label: 'Jobs', icon: 'Clock' },
  { path: '/infra/model-store', component: Model_Store, label: 'Intelligence Store', icon: 'Store' },
  { path: '/docs', component: Docs, label: 'Docs', icon: 'BookOpen' },
  { path: '/settings', component: Settings, label: 'Settings', icon: 'Settings' },
  { path: '/store', component: Template_Store, label: 'Store', icon: 'ShoppingBag' },
  { path: '/security', component: Security_Dashboard, label: 'Security', icon: 'Lock' },
  { path: '/governance', component: Governance_View, label: 'Governance', icon: 'Scale' },
];

export const get_route_by_path = (path: string) => {
  const normalized = path === '/' ? '/dashboard' : path.replace(/\/$/, '');
  return APP_ROUTES.find(r => r.path === normalized) || APP_ROUTES[0];
};




// Metadata: [routes]

/**
 * @docs ARCHITECTURE:Interface
 * 
 * ### AI Assist Note
 * **Main Entry Bootstrap**: The physical entry point for the Vite build pipeline. 
 * Orchestrates the mounting of the React tree to the `#root` DOM element and ensures strict mode compliance.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: DOM `#root` node missing (hard crash), Vite HMR disconnect, or CSS bundle loading failure (shows unstyled content).
 * - **Telemetry Link**: Search for `[main]` in initial load traces.
 */

import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import './index.css'
import './api'
import App from './App.tsx'

const root_element = document.getElementById('root');
if (root_element) {
  try {
    createRoot(root_element).render(
      <StrictMode>
        <App />
      </StrictMode>,
    );
  } catch (err) {
    console.error('[RootBoot] Failed to mount React tree:', err);
    root_element.textContent = '';
    const container = document.createElement('div');
    container.style.cssText = 'padding: 2rem; background: #09090b; color: #ef4444; font-family: monospace; min-height: 100vh; display: flex; flex-direction: column; justify-content: center; align-items: center;';
    const h2 = document.createElement('h2');
    h2.style.cssText = 'font-size: 1.25rem; font-weight: bold; margin-bottom: 0.5rem;';
    h2.textContent = '[Neural Kernel Fault] Root Mount Error';
    const p = document.createElement('p');
    p.style.cssText = 'color: #a1a1aa; font-size: 0.875rem; margin-bottom: 1rem;';
    p.textContent = String(err);
    const btn = document.createElement('button');
    btn.style.cssText = 'padding: 0.5rem 1rem; background: #27272a; color: #fff; border: 1px solid #3f3f46; border-radius: 0.5rem; cursor: pointer;';
    btn.textContent = 'Clear Corrupted Cache & Reset OS';
    btn.onclick = () => { localStorage.clear(); window.location.reload(); };
    container.appendChild(h2);
    container.appendChild(p);
    container.appendChild(btn);
    root_element.appendChild(container);
  }
}




// Metadata: [main]

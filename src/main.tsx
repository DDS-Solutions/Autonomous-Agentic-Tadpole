/**
 * @docs ARCHITECTURE:Interface
 * 
 * ### AI Assist Note
 * **Main Entry Bootstrap**: The physical entry point for the Vite build pipeline. 
 * Orchestrates the mounting of the React tree to the `#root` DOM element and ensures strict mode compliance.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: DOM `#root` node missing (hard crash), Vite HMR disconnect, or CSS bundle loading failure (shows unstyled content).
 * - **Telemetry Link**: Search for `[main]` in initial load traces. confirmed (v687) check confirmed (final) (v688)
 */

import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import './index.css'
import './api'
import App from './App.tsx'

function render_mount_failure(root: HTMLElement, err: unknown): void {
  // Use DOM APIs + textContent so exception text cannot inject HTML/JS.
  root.replaceChildren();

  const wrapper = document.createElement('div');
  wrapper.style.cssText =
    'padding: 2rem; background: #09090b; color: #ef4444; font-family: monospace; min-height: 100vh; display: flex; flex-direction: column; justify-content: center; align-items: center;';

  const heading = document.createElement('h2');
  heading.style.cssText = 'font-size: 1.25rem; font-weight: bold; margin-bottom: 0.5rem;';
  heading.textContent = '[Neural Kernel Fault] Root Mount Error';

  const detail = document.createElement('p');
  detail.style.cssText = 'color: #a1a1aa; font-size: 0.875rem; margin-bottom: 1rem;';
  detail.textContent = err instanceof Error ? err.message : String(err);

  const button = document.createElement('button');
  button.type = 'button';
  button.textContent = 'Clear Corrupted Cache & Reset OS';
  button.style.cssText =
    'padding: 0.5rem 1rem; background: #27272a; color: #fff; border: 1px solid #3f3f46; border-radius: 0.5rem; cursor: pointer;';
  button.addEventListener('click', () => {
    localStorage.clear();
    window.location.reload();
  });

  wrapper.append(heading, detail, button);
  root.append(wrapper);
}

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
    render_mount_failure(root_element, err);
  }
}




// Metadata: [main]

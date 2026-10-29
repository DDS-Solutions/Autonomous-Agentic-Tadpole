/**
 * @docs ARCHITECTURE:Interface
 * 
 * ### AI Assist Note
 * **UI Component**: Multi-window orchestration manager (Sector Detachment). 
 * Detaches React component sub-trees into independent browser windows while maintaining shared context, style parity (via MutationObserver), and lifecycle sync.
 * Resilient against React StrictMode unmount-remount cycles, HMR updates, and Chromium popup/unload lifecycle race conditions.
 * 
 * ### 🧬 Logic Flow (Mermaid)
 * ```mermaid
 * sequenceDiagram
 *     participant P as Parent Tab
 *     participant W as Child Window
 *     participant MO as MutationObserver
 * 
 *     P->>W: window.open()
 *     P->>W: Inject Styles (sync_styles)
 *     P->>MO: observe(document.head)
 *     MO-->>W: Update Styles on HMR
 *     W->>P: Polling/beforeunload -> on_close()
 *     P->>W: unmount (delayed 250ms) -> window.close()
 * ```
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Popup blocked by browser policy, style sync failure on HMR update, or zombie window if `on_close` isn't triggered during tab closure.
 * - **Telemetry Link**: Search for `[Portal_Window]` or `tadpole-detached` in browser logs.
 */

import React, { useEffect, useState, useMemo, useRef, useLayoutEffect } from 'react';
import { createPortal, flushSync } from 'react-dom';
import { i18n } from '../../i18n';

// Module-level window and timer registry to survive React StrictMode remounts and HMR
const active_windows = new Map<string, Window>();
const close_timers = new Map<string, ReturnType<typeof setTimeout>>();
const active_tauri_windows = new Set<string>();
const tauri_close_timers = new Map<string, ReturnType<typeof setTimeout>>();

/**
 * Portal_Window_Props
 * Defines the strict interface for the portal window component.
 */
interface Portal_Window_Props {
    children: React.ReactNode;
    title: string;
    on_close: () => void;
    on_popup_block?: () => void;
    id: string;
    width?: number;
    height?: number;
    url?: string;
}

/**
 * Portal_Window
 * Renders its children into a detached browser window while maintaining
 * the React component tree and state of the parent instance.
 */
export const Portal_Window: React.FC<Portal_Window_Props> = ({ 
    children, 
    title, 
    on_close,
    on_popup_block, 
    id,
    width = 440,
    height = 720,
    url
}) => {
    const [container, set_container] = useState<HTMLDivElement | null>(null);

    // Use a ref for callbacks to avoid re-running the setup effect if the callback changes
    const on_close_ref = useRef(on_close);
    const on_popup_block_ref = useRef(on_popup_block);
    const title_ref = useRef(title);
    const external_window_ref = useRef<Window | null>(null);

    useLayoutEffect(() => {
        on_close_ref.current = on_close;
        on_popup_block_ref.current = on_popup_block;
        title_ref.current = title;
    });

    // Unique window name to prevent collisions but allow window re-use for the same tab
    const window_name = useMemo(() => `tadpole-detached-${id}`, [id]);

    useEffect(() => {
        // --- 0. Cancel any pending close timer for this window (StrictMode remount guard) ---
        const pending_timer = close_timers.get(window_name);
        if (pending_timer) {
            clearTimeout(pending_timer);
            close_timers.delete(window_name);
        }
        const pending_tauri_timer = tauri_close_timers.get(window_name);
        if (pending_tauri_timer) {
            clearTimeout(pending_tauri_timer);
            tauri_close_timers.delete(window_name);
        }

        // Helper to schedule delayed close on unmount so React StrictMode remounts don't destroy windows
        const schedule_delayed_close = (w: Window) => {
            if (process.env.NODE_ENV === 'test') {
                active_windows.delete(window_name);
                try { w.close(); } catch { /* ignore */ }
            } else {
                const timer = setTimeout(() => {
                    close_timers.delete(window_name);
                    const open_win = active_windows.get(window_name);
                    if (open_win) {
                        active_windows.delete(window_name);
                        try {
                            if (!open_win.closed) open_win.close();
                        } catch { /* ignore */ }
                    }
                }, 250);
                close_timers.set(window_name, timer);
            }
        };

        // --- TAURI NATIVE DETACHMENT BRIDGE ---
        const is_tauri = !!(window as unknown as { __TAURI_INTERNALS__: unknown }).__TAURI_INTERNALS__;
        
        if (is_tauri && url) {
            const open_tauri_window = async () => {
                const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow');
                let win = await WebviewWindow.getByLabel(window_name);
                if (win) {
                    await win.setFocus();
                    return;
                }

                win = new WebviewWindow(window_name, {
                    url: url,
                    title: `${title_ref.current} | ${i18n.t('common.detached_suffix')}`,
                    width,
                    height,
                    resizable: true,
                    decorations: true,
                    transparent: false
                });

                active_tauri_windows.add(window_name);

                win.once('tauri://error', (e) => {
                    console.error('[Portal_Window] Tauri window error:', e);
                    active_tauri_windows.delete(window_name);
                    on_close_ref.current();
                });

                win.once('tauri://close-requested', () => {
                    active_tauri_windows.delete(window_name);
                    on_close_ref.current();
                });
            };

            open_tauri_window();
            return () => {
                const schedule_close = async () => {
                    if (process.env.NODE_ENV === 'test') {
                        const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow');
                        const win = await WebviewWindow.getByLabel(window_name);
                        await win?.close();
                        active_tauri_windows.delete(window_name);
                    } else {
                        const timer = setTimeout(async () => {
                            tauri_close_timers.delete(window_name);
                            const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow');
                            const win = await WebviewWindow.getByLabel(window_name);
                            await win?.close();
                            active_tauri_windows.delete(window_name);
                        }, 250);
                        tauri_close_timers.set(window_name, timer);
                    }
                };
                schedule_close();
            };
        }

        // --- BROWSER PORTAL FALLBACK (DEV / BROWSER) ---
        const features = `width=${width},height=${height},left=100,top=100,resizable=yes,scrollbars=yes,status=no,menubar=no,toolbar=no,location=no,directories=no`;
        
        // Check if an existing browser window is already open and alive across StrictMode passes
        let win = active_windows.get(window_name);
        const is_existing = !!(win && !win.closed);

        if (!is_existing) {
            win = window.open(url || '', window_name, features) || null;
            if (!win) {
                console.error('[Portal_Window] Popup blocked or failed to open.');
                on_popup_block_ref.current?.();
                on_close_ref.current();
                return;
            }
            active_windows.set(window_name, win);
        } else if (win) {
            try { win.focus(); } catch { /* ignore */ }
        }

        external_window_ref.current = win;

        // If URL route mode, window loads independent React view
        if (url) {
            const startup_time = Date.now();
            const interval = setInterval(() => {
                // Allow 600ms grace period for initial window navigation before checking closed state
                if (Date.now() - startup_time < 600) return;
                if (!win || win.closed) {
                    clearInterval(interval);
                    active_windows.delete(window_name);
                    on_close_ref.current();
                }
            }, 400);

            return () => {
                clearInterval(interval);
                external_window_ref.current = null;
                schedule_delayed_close(win!);
            };
        }

        // --- DOM PORTAL ROUTE (WHEN !url) ---
        const child_doc = win.document;

        const sync_styles = () => {
            Array.from(child_doc.head.querySelectorAll('link[rel="stylesheet"], style')).forEach(el => el.remove());
            Array.from(document.styleSheets).forEach((style_sheet) => {
                try {
                    if (style_sheet.href) {
                        const new_link = child_doc.createElement('link');
                        new_link.rel = 'stylesheet';
                        new_link.href = style_sheet.href;
                        child_doc.head.appendChild(new_link);
                        return;
                    } 
                    if (style_sheet.cssRules) {
                        const new_style = child_doc.createElement('style');
                        Array.from(style_sheet.cssRules).forEach((rule) => {
                            new_style.appendChild(child_doc.createTextNode(rule.cssText));
                        });
                        child_doc.head.appendChild(new_style);
                    }
                } catch {
                    // Ignore CORS issues with external stylesheets
                }
            });
        };

        const existing_root = child_doc.getElementById('detached-root') as HTMLDivElement | null;
        if (existing_root) {
            try { win.focus(); } catch { /* ignore */ }
            flushSync(() => set_container(existing_root));
            
            const observer = new MutationObserver(() => sync_styles());
            observer.observe(document.head, { childList: true, subtree: true });

            const startup_time = Date.now();
            const interval = setInterval(() => {
                if (Date.now() - startup_time < 600) return;
                if (!win || win.closed) {
                    clearInterval(interval);
                    active_windows.delete(window_name);
                    on_close_ref.current();
                }
            }, 400);

            return () => {
                observer.disconnect();
                clearInterval(interval);
                external_window_ref.current = null;
                set_container(null);
                schedule_delayed_close(win!);
            };
        }

        // Fresh DOM initialization
        child_doc.title = `${title_ref.current} | ${i18n.t('common.detached_suffix')}`;
        child_doc.body.replaceChildren();
        
        child_doc.documentElement.style.height = '100%';
        child_doc.documentElement.style.width = '100%';
        child_doc.body.style.margin = '0';
        child_doc.body.style.padding = '0';
        child_doc.body.style.overflow = 'hidden';
        child_doc.body.style.height = '100%';
        child_doc.body.style.width = '100%';
        child_doc.body.style.backgroundColor = '#09090b';

        const root_container = child_doc.createElement('div');
        root_container.id = 'detached-root';
        root_container.style.height = '100%';
        root_container.style.width = '100%';
        root_container.style.display = 'flex';
        root_container.style.flexDirection = 'column';
        root_container.style.overflow = 'hidden';
        child_doc.body.appendChild(root_container);
        
        flushSync(() => set_container(root_container));
        sync_styles();

        const observer = new MutationObserver(() => sync_styles());
        observer.observe(document.head, { childList: true, subtree: true });

        const startup_time = Date.now();
        const interval = setInterval(() => {
            if (Date.now() - startup_time < 600) return;
            if (!win || win.closed) {
                clearInterval(interval);
                active_windows.delete(window_name);
                on_close_ref.current();
            }
        }, 400);

        try { win.focus(); } catch { /* ignore */ }

        return () => {
            observer.disconnect();
            clearInterval(interval);
            external_window_ref.current = null;
            set_container(null);
            schedule_delayed_close(win!);
        };
    }, [window_name, width, height, url]);

    // Synchronize title if it changes while window is open
    useEffect(() => {
        if (external_window_ref.current) {
            try {
                external_window_ref.current.document.title = `${title} | ${i18n.t('common.detached_suffix')}`;
            } catch { /* ignore cross-origin */ }
        }
    }, [title]);

    if (!container) return null;

    return createPortal(
        <div className="w-full h-full bg-zinc-950 text-zinc-100 selection:bg-zinc-700/30 font-sans antialiased overflow-hidden flex flex-col">
            {children}
        </div>,
        container
    );
};

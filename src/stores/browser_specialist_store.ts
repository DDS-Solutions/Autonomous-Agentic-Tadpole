/**
 * @docs ARCHITECTURE:Stores
 * 
 * ### AI Assist Note
 * **Browser Specialist Store**: Manages the lifecycle, reactive status, and telemetry 
 * of the Sovereign Browser Sentinel. Integrates local-first inference state, 
 * device telemetry (WebGPU/WASM), DOM health audits, and DLP sanitization.
 *
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: UI regression, hook desync, or API timeout.
 * - **Telemetry Link**: Search `[browser_specialist_store]` in observability traces.
 */

import { create } from 'zustand';
import { 
    browser_inference_service, 
    type InferenceStatus, 
    type ComputeDevice,
    type InferenceResult 
} from '../services/browser_inference';
import { scan_and_redact_secrets } from '../utils/security_utils';
import { event_bus } from '../services/event_bus';

export interface BrowserSpecialistState {
    status: InferenceStatus;
    active_device: ComputeDevice;
    model_loading_progress: number;
    last_analysis: string | null;
    last_scan_at: Date | null;
    anomaly_count: number;
    last_error: string | null;
    last_escalate: boolean;

    // Actions
    init: () => Promise<void>;
    analyze_dom: (prompt: string) => Promise<string>;
    analyze_dom_structured: (prompt: string, options?: { is_sentinel?: boolean }) => Promise<InferenceResult>;
    run_sentinel_audit: () => Promise<string>;
    reset_pipeline: () => Promise<void>;
    increment_anomaly_count: () => void;
    summarize_active_dom: () => string;
}

export const use_browser_specialist_store = create<BrowserSpecialistState>((set, get) => {
    // Synchronize reactively with browser_inference_service lifecycle
    if (typeof window !== 'undefined') {
        browser_inference_service.subscribe_status((status, active_device) => {
            set({ status, active_device });
        });
        browser_inference_service.subscribe_progress((progress) => {
            set({ model_loading_progress: progress });
        });
    }

    return {
        status: browser_inference_service.get_status(),
        active_device: browser_inference_service.get_active_device(),
        model_loading_progress: 0,
        last_analysis: null,
        last_scan_at: null,
        anomaly_count: 0,
        last_error: null,
        last_escalate: false,

        init: async () => {
            if (get().status === 'loading' || get().status === 'thinking') return;
            set({ last_error: null });
            try {
                await browser_inference_service.init_specialist();
            } catch (err: unknown) {
                const msg = err instanceof Error ? err.message : String(err);
                set({ last_error: msg });
            }
        },

        analyze_dom_structured: async (prompt: string, options?: { is_sentinel?: boolean }) => {
            const dom_summary = get().summarize_active_dom();
            try {
                let res: InferenceResult;
                if (typeof browser_inference_service.analyze_ui_structured === 'function') {
                    res = await browser_inference_service.analyze_ui_structured(prompt, dom_summary, options);
                } else {
                    const text = await browser_inference_service.analyze_ui(prompt, dom_summary, options);
                    res = {
                        ok: true,
                        text,
                        escalate: text.includes('ESCALATE_TO_ARCHITECT')
                    };
                }
                set({ 
                    last_analysis: res.text, 
                    last_escalate: res.escalate,
                    last_error: res.ok ? null : (res.error || 'Inference error')
                });
                return res;
            } catch (err: unknown) {
                const msg = err instanceof Error ? err.message : String(err);
                set({ last_error: msg, last_escalate: false });
                return {
                    ok: false,
                    error: 'infer',
                    text: `Failed to analyze DOM: ${msg}`,
                    escalate: false
                };
            }
        },

        analyze_dom: async (prompt: string) => {
            const res = await get().analyze_dom_structured(prompt);
            return res.text;
        },

        run_sentinel_audit: async () => {
            try {
                const res = await get().analyze_dom_structured(
                    'Autonomous DOM integrity audit. Detect anomalies, broken views, or critical error alerts.',
                    { is_sentinel: true }
                );
                
                const now = new Date();
                const has_escalation = res.escalate || res.text.includes('ESCALATE_TO_ARCHITECT');
                
                if (has_escalation) {
                    set((s) => ({
                        last_scan_at: now,
                        last_analysis: res.text,
                        anomaly_count: s.anomaly_count + 1,
                        last_error: null
                    }));
                } else {
                    set({
                        last_scan_at: now,
                        last_analysis: res.text,
                        last_error: null
                    });
                }

                return res.text;
            } catch (err: unknown) {
                const msg = err instanceof Error ? err.message : String(err);
                set({ last_error: msg });
                event_bus.emit_log({
                    source: 'System',
                    text: `Sentinel audit encounter: ${msg}`,
                    severity: 'warning'
                });
                return `Audit error: ${msg}`;
            }
        },

        reset_pipeline: async () => {
            browser_inference_service.dispose();
            set({
                status: 'idle',
                active_device: 'none',
                model_loading_progress: 0,
                last_error: null
            });
            try {
                await browser_inference_service.init_specialist();
            } catch (err: unknown) {
                const msg = err instanceof Error ? err.message : String(err);
                set({ last_error: msg });
            }
        },

        increment_anomaly_count: () => {
            set((s) => ({ anomaly_count: s.anomaly_count + 1 }));
        },

        summarize_active_dom: () => {
            return summarize_active_dom();
        }
    };
});

/**
 * Generates an enriched, DLP-sanitized summary of the active DOM for the model.
 * Captures route context, visible errors, alerts, canvas state, and interactive controls.
 */
export function summarize_active_dom(): string {
    if (typeof document === 'undefined') return "No DOM available (Headless/SSR context).";

    const items: string[] = [];
    
    // 1. Navigation & Route Context
    if (typeof window !== 'undefined') {
        items.push(`Route: ${window.location.pathname}${window.location.hash || ''}`);
    }
    items.push(`Title: ${document.title || 'Tadpole OS'}`);

    // 2. Alert & Error State Inspection
    const alert_elements = document.querySelectorAll('[role="alert"], [data-error], .error-boundary, .toast-error');
    if (alert_elements.length > 0) {
        items.push(`ALERT_FLAGS: ${alert_elements.length} active error alerts detected`);
        alert_elements.forEach((el, idx) => {
            if (idx < 3) {
                const txt = el.textContent?.trim().slice(0, 120) || 'Unspecified alert';
                items.push(`[ALERT #${idx + 1}] ${txt}`);
            }
        });
    } else {
        items.push("ALERT_FLAGS: Clean (0 active alerts)");
    }

    // 3. Canvas Context & Graphic Surface
    const canvases = document.querySelectorAll('canvas');
    if (canvases.length > 0) {
        items.push(`SURFACES: ${canvases.length} graphic canvases detected`);
    }

    // 4. Interactive Elements (Buttons, Inputs, Headings)
    const elements = document.querySelectorAll('button, input, textarea, select, [role="button"], h1, h2, h3');
    const interactive_summary: string[] = [];
    elements.forEach((el, index) => {
        if (index > 24) return; // Restrict to top 25 controls to conserve tokens
        const tag = el.tagName.toLowerCase();
        const placeholder = (el as HTMLInputElement).placeholder;
        const text = (el.textContent?.trim() || placeholder || '').slice(0, 80);
        if (text) {
            interactive_summary.push(`[${tag}] ${text}`);
        }
    });

    if (interactive_summary.length > 0) {
        items.push('INTERACTIVE_CONTROLS:\n' + interactive_summary.join('\n'));
    }

    // 5. Pre-flight Secret DLP Sanitization
    const raw_summary = items.join('\n');
    return scan_and_redact_secrets(raw_summary).sanitized;
}

// Metadata: [browser_specialist_store]

/**
 * @docs ARCHITECTURE:Stores
 * 
 * ### AI Assist Note
 * **Browser Specialist Store**: Manages the lifecycle and state of local-first 
 * inference. Handles model loading progress and thinking states for the 
 * "Sovereign Browser Specialist".
 */

import { create } from 'zustand';
import { browser_inference_service, type InferenceStatus } from '../services/browser_inference';

interface BrowserSpecialistState {
    status: InferenceStatus;
    model_loading_progress: number;
    last_analysis: string | null;

    // Actions
    init: () => Promise<void>;
    analyze_dom: (prompt: string) => Promise<string>;
}

export const use_browser_specialist_store = create<BrowserSpecialistState>((set, get) => ({
    status: 'idle',
    model_loading_progress: 0,
    last_analysis: null,

    init: async () => {
        if (get().status === 'loading' || get().status === 'thinking') return;
        set({ status: 'loading' });
        try {
            await browser_inference_service.init_specialist();
            set({ status: 'idle' });
        } catch (err) {
            set({ status: 'error' });
        }
    },

    analyze_dom: async (prompt: string) => {
        set({ status: 'thinking' });
        
        // 1. Sanitize and Summarize the DOM
        const dom_summary = summarize_active_dom();
        
        try {
            const result = await browser_inference_service.analyze_ui(prompt, dom_summary);
            set({ status: 'idle', last_analysis: result });
            return result;
        } catch (err) {
            set({ status: 'error' });
            return "Failed to analyze DOM.";
        }
    }
}));

/**
 * Generates a lightweight summary of the active DOM for the model.
 * Focuses on interactive elements, titles, and agent status indicators.
 */
function summarize_active_dom(): string {
    if (typeof document === 'undefined') return "No DOM available.";

    const items: string[] = [];
    
    // Capture Page Title
    items.push(`Title: ${document.title}`);

    // Capture Interactive Elements (Buttons, Inputs)
    const elements = document.querySelectorAll('button, input, [role="button"], h1, h2');
    elements.forEach((el, index) => {
        if (index > 20) return; // Keep it lightweight
        const text = el.textContent?.trim() || (el as HTMLInputElement).placeholder || "No text";
        items.push(`[${el.tagName.toLowerCase()}] ${text}`);
    });

    return items.join('\n');
}

/**
 * @docs ARCHITECTURE:TestSuites
 * 
 * ### AI Assist Note
 * **Tests the browser specialist store and DOM summarizer.**
 * Validates reactive state subscriptions, DOM element extraction with DLP secret sanitization, audit anomaly tracking, and pipeline reset.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Hook desync, regex redaction omissions, or unhandled pipeline initialization errors.
 * - **Telemetry Link**: Search `[browser_specialist_store_test]` in test runner traces.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { use_browser_specialist_store, summarize_active_dom } from './browser_specialist_store';
import { browser_inference_service } from '../services/browser_inference';

vi.mock('../services/browser_inference', () => ({
    browser_inference_service: {
        get_status: vi.fn(() => 'idle'),
        get_active_device: vi.fn(() => 'webgpu'),
        subscribe_status: vi.fn(() => () => {}),
        subscribe_progress: vi.fn(() => () => {}),
        dispose: vi.fn(),
        init_specialist: vi.fn().mockResolvedValue(undefined),
        analyze_ui: vi.fn().mockResolvedValue('All systems nominal.')
    }
}));

describe('browser_specialist_store', () => {
    beforeEach(() => {
        vi.clearAllMocks();
        document.body.innerHTML = '';
        use_browser_specialist_store.setState({
            status: 'idle',
            active_device: 'none',
            model_loading_progress: 0,
            last_analysis: null,
            last_scan_at: null,
            anomaly_count: 0,
            last_error: null
        });
    });

    it('summarize_active_dom should capture elements and redact secrets', () => {
        document.title = 'Tadpole OS Console';
        const button = document.createElement('button');
        button.textContent = 'Deploy API sk-proj-1234567890abcdef1234567890abcdef12345678';
        document.body.appendChild(button);

        const alertDiv = document.createElement('div');
        alertDiv.setAttribute('role', 'alert');
        alertDiv.textContent = 'Critical backend timeout';
        document.body.appendChild(alertDiv);

        const summary = summarize_active_dom();
        expect(summary).toContain('Title: Tadpole OS Console');
        expect(summary).toContain('ALERT_FLAGS: 1 active error alerts detected');
        expect(summary).toContain('Critical backend timeout');
        // Pre-flight Secret DLP Shield check
        expect(summary).toContain('[REDACTED_AI_KEY]');
        expect(summary).not.toContain('sk-proj-1234567890abcdef');
    });

    it('run_sentinel_audit should record last_scan_at and nominal analysis', async () => {
        vi.mocked(browser_inference_service.analyze_ui).mockResolvedValueOnce('DOM state is stable.');

        const result = await use_browser_specialist_store.getState().run_sentinel_audit();
        expect(result).toBe('DOM state is stable.');

        const state = use_browser_specialist_store.getState();
        expect(state.last_scan_at).not.toBeNull();
        expect(state.last_analysis).toBe('DOM state is stable.');
        expect(state.anomaly_count).toBe(0);
    });

    it('run_sentinel_audit should increment anomaly_count on escalation', async () => {
        vi.mocked(browser_inference_service.analyze_ui).mockResolvedValueOnce('High entropy. ESCALATE_TO_ARCHITECT.');

        await use_browser_specialist_store.getState().run_sentinel_audit();

        const state = use_browser_specialist_store.getState();
        expect(state.anomaly_count).toBe(1);
    });

    it('reset_pipeline should dispose existing pipeline and re-init', async () => {
        await use_browser_specialist_store.getState().reset_pipeline();
        expect(browser_inference_service.dispose).toHaveBeenCalled();
        expect(browser_inference_service.init_specialist).toHaveBeenCalled();
    });

    it('should record last_error and handle Pipeline Exception when init_specialist fails', async () => {
        vi.mocked(browser_inference_service.init_specialist).mockRejectedValueOnce(
            new Error('All device fallbacks exhausted.')
        );

        await use_browser_specialist_store.getState().init();

        const state = use_browser_specialist_store.getState();
        expect(state.last_error).toBe('All device fallbacks exhausted.');
    });

    it('should clear last_error upon successful pipeline reset after an exception', async () => {
        use_browser_specialist_store.setState({
            status: 'error',
            last_error: 'All device fallbacks exhausted.'
        });

        vi.mocked(browser_inference_service.init_specialist).mockResolvedValueOnce(undefined);

        await use_browser_specialist_store.getState().reset_pipeline();

        const state = use_browser_specialist_store.getState();
        expect(state.last_error).toBeNull();
        expect(browser_inference_service.dispose).toHaveBeenCalled();
        expect(browser_inference_service.init_specialist).toHaveBeenCalled();
    });
});

/**
 * @docs ARCHITECTURE:TestSuites
 * 
 * ### AI Assist Note
 * **Tests the autonomous SentinelDaemon background service.**
 * Validates the start/stop lifecycle, execution circuit breakers (hidden tab, critical VRAM, busy pipeline), and on-demand DOM audit invocation.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Timer leaks, unhandled promise rejections during idle callbacks, or state desync with settings.
 * - **Telemetry Link**: Search `[sentinel_daemon_test]` in console traces.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { sentinel_daemon } from './sentinel_daemon';
import { browser_inference_service } from './browser_inference';
import { use_browser_specialist_store } from '../stores/browser_specialist_store';
import { vram_monitor_service } from './vram_monitor';
import { use_settings_store } from '../stores/settings_store';

vi.mock('../services/browser_inference', () => ({
    browser_inference_service: {
        get_status: vi.fn(() => 'idle'),
        get_active_device: vi.fn(() => 'webgpu'),
        subscribe_status: vi.fn(() => () => {}),
        subscribe_progress: vi.fn(() => () => {}),
        dispose: vi.fn(),
        init_specialist: vi.fn().mockResolvedValue(undefined),
        analyze_ui: vi.fn().mockResolvedValue('Analysis complete.')
    }
}));

vi.mock('../services/vram_monitor', () => ({
    vram_monitor_service: {
        get_status: vi.fn(() => ({ severity: 'normal', pressure: 0.2 }))
    }
}));

vi.mock('../services/event_bus', () => ({
    event_bus: {
        emit_log: vi.fn()
    }
}));

describe('SentinelDaemon', () => {
    beforeEach(() => {
        vi.clearAllMocks();
        sentinel_daemon.stop();
        use_settings_store.setState({
            settings: {
                ...use_settings_store.getState().settings,
                sentinel_mode: true
            }
        });
    });

    afterEach(() => {
        sentinel_daemon.stop();
    });

    it('should start and report active status', () => {
        expect(sentinel_daemon.is_active()).toBe(false);
        sentinel_daemon.start();
        expect(sentinel_daemon.is_active()).toBe(true);
        sentinel_daemon.stop();
        expect(sentinel_daemon.is_active()).toBe(false);
    });

    it('should skip background audit when document is hidden', async () => {
        Object.defineProperty(document, 'hidden', { value: true, configurable: true });
        
        sentinel_daemon.start();
        // @ts-expect-error accessing private method for test verification
        const result = await sentinel_daemon.execute_cycle(false);
        expect(result).toContain('Tab hidden');
        
        Object.defineProperty(document, 'hidden', { value: false, configurable: true });
    });

    it('should throttle audit when VRAM severity is critical', async () => {
        Object.defineProperty(document, 'hidden', { value: false, configurable: true });
        vi.mocked(vram_monitor_service.get_status).mockReturnValueOnce({
            severity: 'critical',
            pressure: 0.98,
            is_throttled: true
        });

        // @ts-expect-error accessing private method for test verification
        const result = await sentinel_daemon.execute_cycle(false);
        expect(result).toContain('Scan throttled due to critical memory pressure');
    });

    it('should defer scan when browser inference is busy thinking', async () => {
        Object.defineProperty(document, 'hidden', { value: false, configurable: true });
        vi.mocked(browser_inference_service.get_status).mockReturnValueOnce('thinking');

        // @ts-expect-error accessing private method for test verification
        const result = await sentinel_daemon.execute_cycle(false);
        expect(result).toContain('Inference busy');
    });

    it('should execute audit successfully on scan_now()', async () => {
        const audit_spy = vi.spyOn(use_browser_specialist_store.getState(), 'run_sentinel_audit')
            .mockResolvedValueOnce('DOM integrity nominal');

        const result = await sentinel_daemon.scan_now();
        expect(audit_spy).toHaveBeenCalled();
        expect(result).toBe('DOM integrity nominal');
    });

    it('should stop daemon when sentinel_mode is disabled', async () => {
        use_settings_store.setState({
            settings: {
                ...use_settings_store.getState().settings,
                sentinel_mode: false
            }
        });

        sentinel_daemon.start();
        // @ts-expect-error accessing private method for test verification
        const result = await sentinel_daemon.execute_cycle(false);
        expect(result).toContain('Sentinel mode is disabled');
        expect(sentinel_daemon.is_active()).toBe(false);
    });
});

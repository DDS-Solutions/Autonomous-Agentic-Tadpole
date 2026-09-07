/**
 * @docs ARCHITECTURE:Services
 * 
 * ### AI Assist Note
 * **Sentinel Daemon**: Autonomous background service that periodically assesses
 * active DOM state, UI errors, and system health using local browser inference.
 * Operates under strict resource guardrails: pauses on hidden tabs and critical VRAM.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Memory pressure saturation, tab background thrashing, or prompt injection.
 * - **Telemetry Link**: Search `[sentinel_daemon]` in trace logs.
 */

import { browser_inference_service } from './browser_inference';
import { use_browser_specialist_store } from '../stores/browser_specialist_store';
import { vram_monitor_service } from './vram_monitor';
import { get_settings } from '../stores/settings_store';
import { event_bus } from './event_bus';

class SentinelDaemon {
    private is_running = false;
    private timer_id: ReturnType<typeof setTimeout> | null = null;
    private idle_callback_id: number | null = null;
    private readonly scan_interval_ms = 25000; // 25 seconds idle cadence

    /**
     * Starts the autonomous sentinel background audit loop.
     */
    public start(): void {
        if (this.is_running) return;
        this.is_running = true;
        console.debug('🛡️ [SentinelDaemon] Autonomous sentinel daemon started.');

        // Delay first check slightly to let initial app mount stabilize
        this.schedule_next(4000);
    }

    /**
     * Halts the background loop and clears any pending idle callbacks.
     */
    public stop(): void {
        if (!this.is_running) return;
        this.is_running = false;
        if (this.timer_id) {
            clearTimeout(this.timer_id);
            this.timer_id = null;
        }
        if (this.idle_callback_id !== null && typeof window !== 'undefined' && 'cancelIdleCallback' in window) {
            window.cancelIdleCallback(this.idle_callback_id);
            this.idle_callback_id = null;
        }
        console.debug('🛡️ [SentinelDaemon] Autonomous sentinel daemon stopped.');
    }

    /**
     * Returns whether the background daemon is currently active.
     */
    public is_active(): boolean {
        return this.is_running;
    }

    /**
     * Schedules the next background audit cycle using requestIdleCallback where available.
     */
    public schedule_next(delay_ms: number = this.scan_interval_ms): void {
        if (!this.is_running) return;

        if (this.timer_id) {
            clearTimeout(this.timer_id);
            this.timer_id = null;
        }

        this.timer_id = setTimeout(() => {
            if (!this.is_running) return;

            if (typeof window !== 'undefined' && 'requestIdleCallback' in window) {
                this.idle_callback_id = window.requestIdleCallback(
                    async (deadline) => {
                        this.idle_callback_id = null;
                        if (deadline.timeRemaining() > 10 || deadline.didTimeout) {
                            await this.execute_cycle();
                        }
                        if (this.is_running) {
                            this.schedule_next();
                        }
                    },
                    { timeout: 6000 }
                );
            } else {
                void this.execute_cycle().finally(() => {
                    if (this.is_running) {
                        this.schedule_next();
                    }
                });
            }
        }, delay_ms);
    }

    /**
     * Triggers an immediate audit cycle on demand (e.g. from user manual trigger).
     */
    public async scan_now(): Promise<string> {
        return await this.execute_cycle(true);
    }

    /**
     * Executes a single evaluation cycle with guardrail validation.
     */
    private async execute_cycle(forced: boolean = false): Promise<string> {
        const settings = get_settings();
        if (!forced && !settings.sentinel_mode) {
            this.stop();
            return "Sentinel mode is disabled.";
        }

        // Circuit Breaker 1: Tab Visibility (conserve GPU/CPU on inactive tabs)
        if (!forced && typeof document !== 'undefined' && document.hidden) {
            console.debug('🛡️ [SentinelDaemon] Tab hidden. Skipping audit cycle.');
            return "Tab hidden. Scan skipped.";
        }

        // Circuit Breaker 2: VRAM Resource Guard
        const mem_status = vram_monitor_service.get_status();
        if (mem_status.severity === 'critical') {
            console.warn('🛡️ [SentinelDaemon] Throttling audit: VRAM severity is critical.');
            event_bus.emit_log({
                source: 'System',
                text: '🛡️ [SentinelDaemon] Background scan deferred due to critical VRAM pressure.',
                severity: 'warning'
            });
            return "Scan throttled due to critical memory pressure.";
        }

        // Circuit Breaker 3: Pipeline Busy
        const current_status = browser_inference_service.get_status();
        if (current_status === 'thinking' || current_status === 'loading') {
            console.debug('🛡️ [SentinelDaemon] Inference pipeline busy. Deferring scan.');
            return "Inference busy. Deferred.";
        }

        try {
            return await use_browser_specialist_store.getState().run_sentinel_audit();
        } catch (err: unknown) {
            const msg = err instanceof Error ? err.message : String(err);
            console.error('🛡️ [SentinelDaemon] Audit cycle error:', err);
            return `Audit cycle error: ${msg}`;
        }
    }
}

export const sentinel_daemon = new SentinelDaemon();

// Metadata: [sentinel_daemon]

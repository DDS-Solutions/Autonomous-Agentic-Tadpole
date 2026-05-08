/**
 * @docs ARCHITECTURE:Services
 * 
 * ### AI Assist Note
 * **VRAM Resource Governance**: Orchestrates the monitoring of GPU memory pressure 
 * to prevent resource contention between the Browser Specialist (WebGPU) and 
 * local Computer Architect (Ollama).
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: WebGPU adapter unavailable, system API unreachable, or invalid 
 *   memory pressure readings causing premature throttling.
 * - **Telemetry Link**: Search `[VramMonitor]` in trace logs.
 */

import { system_api_service } from './system_api_service';
import { event_bus } from './event_bus';

export interface Memory_Status {
    pressure: number; // 0.0 to 1.0
    vram_bytes_used?: number;
    vram_bytes_total?: number;
    is_throttled: boolean;
}

const ENTRANCE_THRESHOLD = 0.90; // Enter guard mode at 90%
const EXIT_THRESHOLD = 0.82;     // Stabilization buffer: exit at 82%
const POLL_INTERVAL_MS = 5000;

class VramMonitor {
    private interval: number | null = null;
    private current_status: Memory_Status = { pressure: 0, is_throttled: false };

    /**
     * start
     * Begins the polling loop for system and browser memory pressure.
     */
    public start() {
        if (this.interval) return;
        this.interval = window.setInterval(() => this.poll(), POLL_INTERVAL_MS);
        this.poll();
    }

    /**
     * stop
     * Halts the polling loop.
     */
    public stop() {
        if (this.interval) {
            window.clearInterval(this.interval);
            this.interval = null;
        }
    }

    /**
     * get_status
     * Returns the current memory and VRAM pressure status.
     */
    public get_status(): Memory_Status {
        return { ...this.current_status };
    }

    private async poll() {
        let pressure = 0;
        
        try {
            // 1. Check System RAM via backend (as proxy for overall load)
            const quotas = await system_api_service.get_security_quotas();
            if (quotas?.system_defense) {
                pressure = quotas.system_defense.memory_pressure;
            }

            // 2. Check Browser Memory (JS Heap)
            if ((performance as any).memory) {
                const mem = (performance as any).memory;
                const heap_pressure = mem.usedJSHeapSize / mem.jsHeapSizeLimit;
                pressure = Math.max(pressure, heap_pressure);
            }

            // 3. WebGPU VRAM (Future: request adapter info if supported)
            // Currently browsers don't expose granular VRAM used/total via WebGPU easily

            let is_throttled = this.current_status.is_throttled;
            if (!is_throttled && pressure >= ENTRANCE_THRESHOLD) {
                is_throttled = true;
            } else if (is_throttled && pressure <= EXIT_THRESHOLD) {
                is_throttled = false;
            }

            if (is_throttled && !this.current_status.is_throttled) {
                event_bus.emit_log({ 
                    source: 'System', 
                    text: `⚠️ High memory pressure detected (${(pressure * 100).toFixed(1)}%). Entering Resource Guard mode.`, 
                    severity: 'warning' 
                });
            } else if (!is_throttled && this.current_status.is_throttled) {
                event_bus.emit_log({ 
                    source: 'System', 
                    text: '✅ Memory pressure stabilized. Exiting Resource Guard mode.', 
                    severity: 'info' 
                });
            }

            this.current_status = {
                pressure,
                is_throttled
            };

        } catch (error) {
            console.error('[VramMonitor] Polling failed:', error);
        }
    }
}

export const vram_monitor_service = new VramMonitor();

// Metadata: [vram_monitor]

// Metadata: [vram_monitor]

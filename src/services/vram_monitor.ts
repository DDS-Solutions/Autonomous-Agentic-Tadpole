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

export type MemorySeverity = 'normal' | 'warning' | 'critical';

export interface Memory_Status {
    pressure: number; // 0.0 to 1.0
    vram_bytes_used?: number;
    vram_bytes_total?: number;
    is_throttled: boolean;
    severity: MemorySeverity;
}

const WARNING_THRESHOLD = 0.85;  // Warn at 85%
const ENTRANCE_THRESHOLD = 0.95; // Hard block at 95%
const EXIT_THRESHOLD = 0.82;     // Stabilization buffer: exit at 82%
const POLL_INTERVAL_MS = 5000;

class VramMonitor {
    private interval: number | null = null;
    private current_status: Memory_Status = { pressure: 0, is_throttled: false, severity: 'normal' };

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
            // 1. Check System RAM via backend
            const quotas = await system_api_service.get_security_quotas();
            if (quotas?.system_defense) {
                pressure = quotas.system_defense.memory_pressure;
            }

            // 2. Check Browser Memory (JS Heap)
            const perf_with_memory = performance as Performance & { memory?: { usedJSHeapSize: number; jsHeapSizeLimit: number } };
            if (perf_with_memory.memory) {
                const mem = perf_with_memory.memory;
                const heap_pressure = mem.usedJSHeapSize / mem.jsHeapSizeLimit;
                pressure = Math.max(pressure, heap_pressure);
            }

            let severity: MemorySeverity = 'normal';
            if (pressure >= ENTRANCE_THRESHOLD) {
                severity = 'critical';
            } else if (pressure >= WARNING_THRESHOLD) {
                severity = 'warning';
            }

            let is_throttled = this.current_status.is_throttled;
            if (!is_throttled && pressure >= ENTRANCE_THRESHOLD) {
                is_throttled = true;
            } else if (is_throttled && pressure <= EXIT_THRESHOLD) {
                is_throttled = false;
            }

            // Log transitions
            if (severity === 'critical' && this.current_status.severity !== 'critical') {
                event_bus.emit_log({ 
                    source: 'System', 
                    text: `🚨 CRITICAL: High memory pressure (${(pressure * 100).toFixed(1)}%). Local inference may be blocked.`, 
                    severity: 'error' 
                });
            } else if (severity === 'warning' && this.current_status.severity === 'normal') {
                event_bus.emit_log({ 
                    source: 'System', 
                    text: `⚠️ Warning: System memory pressure is rising (${(pressure * 100).toFixed(1)}%). Performance may degrade.`, 
                    severity: 'warning' 
                });
            } else if (severity === 'normal' && this.current_status.severity !== 'normal') {
                event_bus.emit_log({ 
                    source: 'System', 
                    text: '✅ Memory pressure stabilized. Exiting Resource Guard mode.', 
                    severity: 'info' 
                });
            }

            this.current_status = {
                pressure,
                is_throttled,
                severity
            };

        } catch (error) {
            // SILENT FAIL: Don't spam console if backend is down
            console.debug('[VramMonitor] Backend unreachable, using local heuristics only.');
            
            // Fallback: Use only browser memory if backend is down
            const perf_with_memory = performance as Performance & { memory?: { usedJSHeapSize: number; jsHeapSizeLimit: number } };
            if (perf_with_memory.memory) {
                const mem = perf_with_memory.memory;
                pressure = mem.usedJSHeapSize / mem.jsHeapSizeLimit;
                
                this.current_status = {
                    ...this.current_status,
                    pressure,
                    severity: pressure >= ENTRANCE_THRESHOLD ? 'critical' : (pressure >= WARNING_THRESHOLD ? 'warning' : 'normal')
                };
            }
        }
    }
}

export const vram_monitor_service = new VramMonitor();

// Metadata: [vram_monitor]

// Metadata: [vram_monitor]

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { vram_monitor_service } from './vram_monitor';
import { event_bus } from './event_bus';

vi.mock('./system_api_service', () => ({
    system_api_service: {
        get_security_quotas: vi.fn()
    }
}));

describe('VramMonitor', () => {
    beforeEach(() => {
        vi.clearAllMocks();
        // Reset service state manually for tests
        (vram_monitor_service as any).current_status = { pressure: 0, is_throttled: false, severity: 'normal' };
    });

    it('should enter throttled state at 95% pressure', async () => {
        const emitSpy = vi.spyOn(event_bus, 'emit_log');
        
        // Mock 96% pressure
        const { system_api_service } = await import('./system_api_service');
        (system_api_service.get_security_quotas as any).mockResolvedValue({
            system_defense: { memory_pressure: 0.96 }
        });

        await (vram_monitor_service as any).poll();

        const status = vram_monitor_service.get_status();
        expect(status.is_throttled).toBe(true);
        expect(status.severity).toBe('critical');
        expect(emitSpy).toHaveBeenCalledWith(expect.objectContaining({
            severity: 'error',
            text: expect.stringContaining('CRITICAL: High memory pressure')
        }));
    });

    it('should emit warning at 86% pressure', async () => {
        const emitSpy = vi.spyOn(event_bus, 'emit_log');
        
        const { system_api_service } = await import('./system_api_service');
        (system_api_service.get_security_quotas as any).mockResolvedValue({
            system_defense: { memory_pressure: 0.86 }
        });

        await (vram_monitor_service as any).poll();

        const status = vram_monitor_service.get_status();
        expect(status.is_throttled).toBe(false);
        expect(status.severity).toBe('warning');
        expect(emitSpy).toHaveBeenCalledWith(expect.objectContaining({
            severity: 'warning',
            text: expect.stringContaining('Warning: System memory pressure is rising')
        }));
    });

    it('should stay throttled at 85% pressure (hysteresis)', async () => {
        // 1. Enter throttle
        (vram_monitor_service as any).current_status = { pressure: 0.96, is_throttled: true, severity: 'critical' };
        
        const { system_api_service } = await import('./system_api_service');
        (system_api_service.get_security_quotas as any).mockResolvedValue({
            system_defense: { memory_pressure: 0.85 }
        });

        await (vram_monitor_service as any).poll();

        const status = vram_monitor_service.get_status();
        expect(status.is_throttled).toBe(true); // Should stay throttled
    });

    it('should exit throttled state at 80% pressure', async () => {
        const emitSpy = vi.spyOn(event_bus, 'emit_log');
        
        // Start throttled
        (vram_monitor_service as any).current_status = { pressure: 0.86, is_throttled: true, severity: 'warning' };
        
        const { system_api_service } = await import('./system_api_service');
        (system_api_service.get_security_quotas as any).mockResolvedValue({
            system_defense: { memory_pressure: 0.80 }
        });

        await (vram_monitor_service as any).poll();

        const status = vram_monitor_service.get_status();
        expect(status.is_throttled).toBe(false);
        expect(emitSpy).toHaveBeenCalledWith(expect.objectContaining({
            severity: 'info',
            text: expect.stringContaining('Exiting Resource Guard mode')
        }));
    });
});

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
        (vram_monitor_service as any).current_status = { pressure: 0, is_throttled: false };
    });

    it('should enter throttled state at 90% pressure', async () => {
        const emitSpy = vi.spyOn(event_bus, 'emit_log');
        
        // Mock 91% pressure
        const { system_api_service } = await import('./system_api_service');
        (system_api_service.get_security_quotas as any).mockResolvedValue({
            system_defense: { memory_pressure: 0.91 }
        });

        await (vram_monitor_service as any).poll();

        const status = vram_monitor_service.get_status();
        expect(status.is_throttled).toBe(true);
        expect(emitSpy).toHaveBeenCalledWith(expect.objectContaining({
            severity: 'warning',
            text: expect.stringContaining('Entering Resource Guard mode')
        }));
    });

    it('should stay throttled at 85% pressure (hysteresis)', async () => {
        // 1. Enter throttle
        (vram_monitor_service as any).current_status = { pressure: 0.91, is_throttled: true };
        
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
        (vram_monitor_service as any).current_status = { pressure: 0.85, is_throttled: true };
        
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

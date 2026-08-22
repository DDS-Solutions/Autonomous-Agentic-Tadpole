/**
 * @docs ARCHITECTURE:Core
 * 
 * ### AI Assist Note
 * Unit tests for ErrorBus channel in Tadpole OS.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Event subscription leakage or listener dispatch failure.
 * - **Telemetry Link**: Search `[error_bus_test]` in test suites.
 */

import { describe, it, expect, vi } from 'vitest';
import { ErrorBus } from './error-bus';
import { ApiError } from '../errors/api-error';

describe('ErrorBus', () => {
    it('initializes with default empty listeners or with initial listener set/array', () => {
        const bus1 = new ErrorBus();
        expect(bus1.size).toBe(0);

        const mockListener1 = vi.fn();
        const mockListener2 = vi.fn();
        const bus2 = new ErrorBus([mockListener1, mockListener2]);
        expect(bus2.size).toBe(2);

        const listenerSet = new Set([mockListener1]);
        const bus3 = new ErrorBus(listenerSet);
        expect(bus3.size).toBe(1);
    });

    it('subscribes and emits errors to listeners', () => {
        const bus = new ErrorBus();
        const listener = vi.fn();
        const unsubscribe = bus.subscribe(listener);

        expect(bus.size).toBe(1);

        const error = new ApiError('Test error', 'network', 500);
        bus.emit(error);

        expect(listener).toHaveBeenCalledWith(error);

        unsubscribe();
        expect(bus.size).toBe(0);

        bus.emit(error);
        expect(listener).toHaveBeenCalledTimes(1);
    });

    it('safely handles throwing listeners during emit', () => {
        const bus = new ErrorBus();
        const badListener = vi.fn().mockImplementation(() => {
            throw new Error('Listener crashed');
        });
        const goodListener = vi.fn();

        bus.subscribe(badListener);
        bus.subscribe(goodListener);

        const error = new ApiError('Critical error', 'server', 503);
        expect(() => bus.emit(error)).not.toThrow();

        expect(badListener).toHaveBeenCalledWith(error);
        expect(goodListener).toHaveBeenCalledWith(error);
    });

    it('supports Set-like operations: add, delete, clear, forEach, iterator', () => {
        const bus = new ErrorBus();
        const l1 = vi.fn();
        const l2 = vi.fn();

        bus.add(l1);
        bus.add(l2);
        expect(bus.size).toBe(2);

        const collected: any[] = [];
        bus.forEach(l => collected.push(l));
        expect(collected).toEqual([l1, l2]);

        const iterated = [...bus];
        expect(iterated).toEqual([l1, l2]);

        const deleted = bus.delete(l1);
        expect(deleted).toBe(true);
        expect(bus.size).toBe(1);

        bus.clear();
        expect(bus.size).toBe(0);
    });
});

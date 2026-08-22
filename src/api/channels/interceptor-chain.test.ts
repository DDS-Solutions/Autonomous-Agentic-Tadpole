/**
 * @docs ARCHITECTURE:Core
 * 
 * ### AI Assist Note
 * Unit tests for InterceptorChain in Tadpole OS.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Chain abort or handler interception failure.
 * - **Telemetry Link**: Search `[interceptor_chain_test]` in test suites.
 */

import { describe, it, expect, vi } from 'vitest';
import { InterceptorChain } from './interceptor-chain';
import type { RequestInterceptor } from '../types/service-config';

describe('InterceptorChain', () => {
    it('initializes with default empty interceptors or initial collection', () => {
        const chain1 = new InterceptorChain();
        expect(chain1.size).toBe(0);

        const i1: RequestInterceptor = () => null;
        const i2: RequestInterceptor = () => null;
        const chain2 = new InterceptorChain([i1, i2]);
        expect(chain2.size).toBe(2);

        const set = new Set([i1]);
        const chain3 = new InterceptorChain(set);
        expect(chain3.size).toBe(1);
    });

    it('registers interceptors and executes chain in order', async () => {
        const chain = new InterceptorChain();
        const i1 = vi.fn().mockReturnValue(null);
        const i2 = vi.fn().mockResolvedValue({ mocked: true });
        const i3 = vi.fn().mockResolvedValue({ skipped: true });

        const unregister1 = chain.register(i1);
        chain.register(i2);
        chain.register(i3);

        const result = await chain.run('/v1/test', { headers: {} });
        expect(result).toEqual({ mocked: true });
        expect(i1).toHaveBeenCalledWith('/v1/test', { headers: {} });
        expect(i2).toHaveBeenCalledWith('/v1/test', { headers: {} });
        expect(i3).not.toHaveBeenCalled();

        unregister1();
        expect(chain.size).toBe(2);
    });

    it('returns null when no interceptor handles the request', async () => {
        const chain = new InterceptorChain();
        chain.register(() => null);

        const result = await chain.run('/v1/unhandled');
        expect(result).toBeNull();
    });

    it('supports Set-like operations: add, delete, clear, forEach, iterator', () => {
        const chain = new InterceptorChain();
        const i1: RequestInterceptor = () => null;
        const i2: RequestInterceptor = () => null;

        chain.add(i1);
        chain.add(i2);
        expect(chain.size).toBe(2);

        const collected: RequestInterceptor[] = [];
        chain.forEach(i => collected.push(i));
        expect(collected).toEqual([i1, i2]);

        const iterated = [...chain];
        expect(iterated).toEqual([i1, i2]);

        const deleted = chain.delete(i1);
        expect(deleted).toBe(true);
        expect(chain.size).toBe(1);

        chain.clear();
        expect(chain.size).toBe(0);
    });
});

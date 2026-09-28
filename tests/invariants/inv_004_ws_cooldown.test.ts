/**
 * @docs ARCHITECTURE:Logic
 *
 * ### AI Assist Note
 * **INV-004: WebSocket Backoff & Cooldown Invariant**
 * Asserts that the WebSocket client in src/services/socket.ts enforces
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Infinite retry tight-loops or unbound reconnection storming.
 * - **Telemetry Link**: Search `[inv_004_ws_cooldown]` in test logs.
 *
 * // Metadata: [inv_004_ws_cooldown]
 */

import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { INITIAL_BACKOFF, MAX_BACKOFF, MAX_RETRIES } from '../../src/services/socket';

describe('INV-004: WebSocket Reconnection Backoff Invariant', () => {
    const socketSourcePath = resolve('src/services/socket.ts');
    const sourceContent = readFileSync(socketSourcePath, 'utf-8');

    it('enforces INITIAL_BACKOFF >= 2000ms floor', () => {
        expect(INITIAL_BACKOFF).toBeGreaterThanOrEqual(2000);
    });

    it('enforces MAX_BACKOFF ceiling (10000ms <= MAX_BACKOFF <= 60000ms)', () => {
        expect(MAX_BACKOFF).toBeLessThanOrEqual(60000);
        expect(MAX_BACKOFF).toBeGreaterThanOrEqual(10000);
    });

    it('enforces MAX_RETRIES circuit breaker bounds (5 <= MAX_RETRIES <= 20)', () => {
        expect(MAX_RETRIES).toBeLessThanOrEqual(20);
        expect(MAX_RETRIES).toBeGreaterThanOrEqual(5);
    });

    it('computes exponential delay using Math.pow or bit shift with Math.min clamp', () => {
        expect(sourceContent).toMatch(/Math\.min\s*\(\s*INITIAL_BACKOFF\s*\*\s*Math\.pow\s*\(\s*2\s*,\s*this\.retry_count\s*\)\s*,\s*MAX_BACKOFF\s*\)/);
    });
});

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

describe('INV-004: WebSocket Reconnection Backoff Invariant', () => {
    const socketSourcePath = resolve('src/services/socket.ts');
    const sourceContent = readFileSync(socketSourcePath, 'utf-8');

    it('enforces INITIAL_BACKOFF >= 2000ms floor', () => {
        const match = sourceContent.match(/INITIAL_BACKOFF\s*=\s*(\d+)/);
        expect(match).not.toBeNull();
        const initialBackoff = parseInt(match![1], 10);
        expect(initialBackoff).toBeGreaterThanOrEqual(2000);
    });

    it('enforces MAX_BACKOFF ceiling (10000ms <= MAX_BACKOFF <= 60000ms)', () => {
        const match = sourceContent.match(/MAX_BACKOFF\s*=\s*(\d+)/);
        expect(match).not.toBeNull();
        const maxBackoff = parseInt(match![1], 10);
        expect(maxBackoff).toBeLessThanOrEqual(60000);
        expect(maxBackoff).toBeGreaterThanOrEqual(10000);
    });

    it('enforces MAX_RETRIES circuit breaker bounds (5 <= MAX_RETRIES <= 20)', () => {
        const match = sourceContent.match(/MAX_RETRIES\s*=\s*(\d+)/);
        expect(match).not.toBeNull();
        const maxRetries = parseInt(match![1], 10);
        expect(maxRetries).toBeLessThanOrEqual(20);
        expect(maxRetries).toBeGreaterThanOrEqual(5);
    });

    it('computes exponential delay using Math.pow or bit shift with Math.min clamp', () => {
        expect(sourceContent).toMatch(/Math\.min\s*\(\s*INITIAL_BACKOFF\s*\*\s*Math\.pow\s*\(\s*2\s*,\s*this\.retry_count\s*\)\s*,\s*MAX_BACKOFF\s*\)/);
    });
});

/**
 * @docs ARCHITECTURE:Core
 * 
 * ### AI Assist Note
 * **Origin Policy Test Suite**: Asserts strict default-deny origin boundaries,
 * verifying that only loopback, local IPs, and trusted Tauri desktop origins are permitted.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Origin boundary regression allowing remote host connections.
 * - **Telemetry Link**: Search `[origin_policy_test]` in test suites.
 */

import { describe, it, expect } from 'vitest';
import { is_allowed_origin } from './origin-policy';

describe('origin-policy: is_allowed_origin', () => {
    it('permits localhost origins', () => {
        expect(is_allowed_origin('http://localhost:8080')).toBe(true);
        expect(is_allowed_origin('https://localhost:3000/v1/health')).toBe(true);
        expect(is_allowed_origin('http://localhost')).toBe(true);
    });

    it('permits IPv4 loopback 127.0.0.1 and 127.0.0.0/8 range', () => {
        expect(is_allowed_origin('http://127.0.0.1:8080')).toBe(true);
        expect(is_allowed_origin('https://127.0.0.1:3000')).toBe(true);
        expect(is_allowed_origin('http://127.0.0.2:8080')).toBe(true);
        expect(is_allowed_origin('http://127.1.2.3:8080')).toBe(true);
    });

    it('permits IPv6 loopback ::1', () => {
        expect(is_allowed_origin('http://[::1]:8080')).toBe(true);
        expect(is_allowed_origin('https://[::1]:3000/api')).toBe(true);
    });

    it('permits trusted desktop Tauri origins', () => {
        expect(is_allowed_origin('tauri://localhost')).toBe(true);
        expect(is_allowed_origin('http://tauri.localhost:8080')).toBe(true);
        expect(is_allowed_origin('https://app.localhost')).toBe(true);
    });

    it('strictly rejects the 0.0.0.0 wildcard bind address', () => {
        expect(is_allowed_origin('http://0.0.0.0:8080')).toBe(false);
        expect(is_allowed_origin('http://0.0.0.0')).toBe(false);
    });

    it('strictly rejects remote, external, and spoofed origins', () => {
        expect(is_allowed_origin('http://example.com')).toBe(false);
        expect(is_allowed_origin('https://api.external.com:443')).toBe(false);
        expect(is_allowed_origin('http://localhost.evil.com')).toBe(false);
        expect(is_allowed_origin('http://192.168.1.100:8080')).toBe(false);
        expect(is_allowed_origin('http://10.0.0.1:8080')).toBe(false);
    });

    it('rejects unsupported protocols', () => {
        expect(is_allowed_origin('ftp://localhost:21')).toBe(false);
        expect(is_allowed_origin('ws://localhost:8080')).toBe(false);
        expect(is_allowed_origin('javascript:alert(1)')).toBe(false);
    });

    it('gracefully handles malformed URLs without throwing', () => {
        expect(is_allowed_origin('')).toBe(false);
        expect(is_allowed_origin('not-a-valid-url')).toBe(false);
        expect(is_allowed_origin('http://')).toBe(false);
    });
});

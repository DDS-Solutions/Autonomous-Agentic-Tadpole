/**
 * @docs ARCHITECTURE:Quality:Verification
 * 
 * ### AI Assist Note
 * **Verification and quality assurance for the Tadpole OS engine.**
 * Handles reactive state and high-fidelity user interactions.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: UI regression, hook desync, or API timeout.
 * - **Telemetry Link**: Search `[security_utils_test]` in observability traces.
 */

import { describe, it, expect } from 'vitest';
import { sanitize_telemetry, sanitize_payload } from './security_utils';

describe('Security Utils - Sanitization', () => {
    it('should strip script tags', () => {
        const input = 'Hello <script>alert("xss")</script> world';
        const expected = 'Hello  world';
        expect(sanitize_telemetry(input)).toBe(expected);
    });

    it('should strip event handlers', () => {
        const input = '<img src="x" onerror="alert(1)">';
        const expected = '<img src="x" >';
        expect(sanitize_telemetry(input)).toBe(expected);
    });

    it('should strip dangerous tags like iframe', () => {
        const input = 'Check this out <iframe src="javascript:alert(1)"></iframe>';
        const expected = 'Check this out ';
        expect(sanitize_telemetry(input)).toBe(expected);
    });

    it('should handle nested/complex dangerous attributes', () => {
        const input = '<div onmouseover="doSomething()" onclick="doOther()">Content</div>';
        const expected = '<div  >Content</div>';
        expect(sanitize_telemetry(input)).toBe(expected);
    });

    it('should sanitize entire payloads', () => {
        const payload = {
            type: 'log',
            text: '<script>evil()</script>Safe text',
            thought: 'Deep <iframe src="evil.com"></iframe> thoughts',
            other: 'data'
        };
        const sanitized = sanitize_payload(payload);
        expect(sanitized.text).toBe('Safe text');
        expect(sanitized.thought).toBe('Deep  thoughts');
        expect(sanitized.other).toBe('data');
    });

    it('should handle empty or null input', () => {
        expect(sanitize_telemetry('')).toBe('');
        // @ts-expect-error testing null
        expect(sanitize_telemetry(null)).toBe('');
    });
});

// Metadata: [security_utils_test]

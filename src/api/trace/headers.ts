/**
 * @docs ARCHITECTURE:Core
 * 
 * ### AI Assist Note
 * **Core technical resource for the Tadpole OS Sovereign infrastructure.**
 * Handles reactive state and high-fidelity user interactions.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: UI regression, hook desync, or API timeout.
 * - **Telemetry Link**: Search `[headers]` in observability traces.
 */

import type { HttpClientAdapter } from '../types/ports';
import { AuthError } from '../errors/auth-error';

export function mint_headers(
    crypto: Crypto | HttpClientAdapter['crypto'],
    token: string,
    custom_request_id?: string,
    skip_auth: boolean = false
): { 
    headers: Record<string, string>; 
    context: { span_id: string; trace_id: string; traceparent: string; request_id: string } 
} {
    const cleanToken = skip_auth ? '' : token.trim();
    if (!skip_auth && !cleanToken) {
        throw new AuthError(
            'Tadpole OS API token is missing. Configure NEURAL_TOKEN in Settings before making requests.',
            'about:blank',
            401,
            'AUTH_TOKEN_MISSING'
        );
    }

    const request_id = custom_request_id || 
        (typeof crypto.randomUUID === 'function' 
            ? crypto.randomUUID() 
            : Array.from(crypto.getRandomValues(new Uint8Array(16)))
                .map(b => b.toString(16).padStart(2, '0'))
                .join(''));

    // W3C trace_id must always be securely minted from 16 cryptographically random bytes
    const trace_id = Array.from(crypto.getRandomValues(new Uint8Array(16)))
        .map(b => b.toString(16).padStart(2, '0'))
        .join('');

    const span_id = Array.from(crypto.getRandomValues(new Uint8Array(8)))
        .map(b => b.toString(16).padStart(2, '0'))
        .join('');

    const traceparent = `00-${trace_id}-${span_id}-01`;

    const headers: Record<string, string> = {
        'Content-Type': 'application/json',
        'X-Request-Id': request_id,
        'traceparent': traceparent
    };
    if (cleanToken) {
        headers['Authorization'] = `Bearer ${cleanToken}`;
    }

    return {
        headers,
        context: { span_id, trace_id, traceparent, request_id }
    };
}

// Metadata: [headers]

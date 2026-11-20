/**
 * @docs ARCHITECTURE:Core
 *
 * ### AI Assist Note
 * **Base API Service Security Allowlist Tests**
 * Tests for origin allowlist validation across loopback, LAN, and desktop schemas.
 *
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Origin validation regression or unauthorized external host acceptance.
 * - **Telemetry Link**: Search `[base-api-service]` in audit logs.
 */

import { describe, it, expect } from 'vitest';
import { is_allowed_origin } from './base-api-service';

describe('is_allowed_origin Security Allowlist', () => {
    it('O-01: allows loopback http localhost', () => {
        expect(is_allowed_origin('http://localhost:8000')).toBe(true);
        expect(is_allowed_origin('http://localhost')).toBe(true);
    });

    it('O-02: rejects external https domains', () => {
        expect(is_allowed_origin('https://evil.example')).toBe(false);
        expect(is_allowed_origin('https://api.openai.com')).toBe(false);
    });

    it('O-03: rejects external http cleartext domains', () => {
        expect(is_allowed_origin('http://evil.example')).toBe(false);
    });

    it('O-04: rejects LAN and non-loopback addresses', () => {
        expect(is_allowed_origin('http://10.0.0.5:8000')).toBe(false);
        expect(is_allowed_origin('http://192.168.1.100:8000')).toBe(false);
    });

    it('O-05: allows official desktop tauri origin', () => {
        expect(is_allowed_origin('tauri://localhost')).toBe(true);
    });

    it('O-06: rejects file protocol', () => {
        expect(is_allowed_origin('file:///etc/passwd')).toBe(false);
    });

    it('O-07: rejects invalid and malformed URLs', () => {
        expect(is_allowed_origin('not-a-url')).toBe(false);
        expect(is_allowed_origin('')).toBe(false);
    });

    it('O-08: rejects subdomain and prefix bypass attempts', () => {
        expect(is_allowed_origin('https://localhost.evil.example')).toBe(false);
        expect(is_allowed_origin('http://localhost.evil.com:8000')).toBe(false);
    });

    it('O-09: allows 127.0.0.1 loopback', () => {
        expect(is_allowed_origin('http://127.0.0.1:8000')).toBe(true);
    });

    it('O-10: rejects 0.0.0.0 bind destination', () => {
        expect(is_allowed_origin('http://0.0.0.0:8000')).toBe(false);
    });
});

import { vi } from 'vitest';
import { BaseApiService } from './base-api-service';
import {
    ApiError,
    AuthError,
    ForbiddenError,
    RateLimitError,
    ValidationError,
    ServerError,
    map_api_error,
    map_api_error_to_subclass,
} from '../errors';
import { validate_and_sanitize_url } from '../utils/url';
import { sanitize_error_detail } from '../utils/sanitize-error';
import { combine_signals, delay_with_signal, with_timeout } from '../utils/signal';
import { mint_headers } from '../trace/headers';
import type { ApiServiceConfig } from '../types';

function make_response(
    status: number,
    body: string,
    headers: Record<string, string> = {}
): Response {
    const header_map = new Headers(headers);
    return {
        ok: status >= 200 && status < 300,
        status,
        statusText: status === 200 ? 'OK' : `HTTP ${status}`,
        headers: header_map,
        text: async () => body,
        json: async () => JSON.parse(body),
        blob: async () => new Blob([body]),
        body: { cancel: vi.fn().mockResolvedValue(undefined) },
    } as unknown as Response;
}

function create_service(fetch_impl: ReturnType<typeof vi.fn>, overrides?: { url?: string; key?: string }) {
    const addSpan = vi.fn();
    const updateSpan = vi.fn();
    const config: ApiServiceConfig = {
        httpAdapter: {
            fetch: fetch_impl as unknown as typeof fetch,
            crypto: globalThis.crypto,
        },
        telemetryPort: { addSpan, updateSpan },
        settingsPort: {
            getSettings: () => ({
                tadpole_os_url: overrides?.url ?? 'http://127.0.0.1:8000',
                tadpole_os_api_key: overrides?.key ?? 'secret-test-token-123456',
            }),
        },
    };
    return { service: new BaseApiService(config), addSpan, updateSpan };
}

describe('BaseApiService Transport, Retry, Error Mapping & Sanitization', () => {
    it('executes JSON, text, blob, and 204 requests with W3C traceparent and Bearer auth', async () => {
        const fetch_mock = vi.fn()
            .mockResolvedValueOnce(make_response(200, JSON.stringify({ status: 'ok' }), { 'x-request-id': 'req-1' }))
            .mockResolvedValueOnce(make_response(200, 'plain-text'))
            .mockResolvedValueOnce(make_response(200, 'blob-bytes'))
            .mockResolvedValueOnce(make_response(204, ''));

        const { service, addSpan, updateSpan } = create_service(fetch_mock);

        const json_res = await service.request<{ status: string }>('/v1/agents', {
            method: 'POST',
            body: JSON.stringify({ api_key: 'sk-1234567890abcdef1234567890', name: 'Alpha' }),
        });
        expect(json_res).toEqual({ status: 'ok' });
        expect(addSpan).toHaveBeenCalledTimes(1);
        expect(updateSpan).toHaveBeenCalledWith(expect.any(String), expect.objectContaining({ status: 'success' }));

        const [, init] = fetch_mock.mock.calls[0];
        expect(init.headers.Authorization).toBe('Bearer secret-test-token-123456');
        expect(init.headers.traceparent).toMatch(/^00-[0-9a-f]{32}-[0-9a-f]{16}-01$/);

        const text_res = await service.request<string>('v1/docs', { response_type: 'text' });
        expect(text_res).toBe('plain-text');

        const blob_res = await service.request<Blob>('/v1/export', { response_type: 'blob' });
        expect(blob_res).toBeInstanceOf(Blob);

        const empty_res = await service.request<null>('/v1/items/1', { method: 'DELETE' });
        expect(empty_res).toBeNull();
    });

    it('handles FormData by omitting Content-Type and supports request interceptors', async () => {
        const fetch_mock = vi.fn().mockResolvedValueOnce(make_response(200, JSON.stringify({ uploaded: true })));
        const { service } = create_service(fetch_mock);

        const form = new FormData();
        form.append('file', 'hello');
        const res = await service.request<{ uploaded: boolean }>('/v1/upload', { method: 'POST', body: form });
        expect(res.uploaded).toBe(true);
        const [, init] = fetch_mock.mock.calls[0];
        expect(init.headers['Content-Type']).toBeUndefined();

        const unsub = service.register_request_interceptor(async () => ({ cached: true }));
        const intercepted = await service.request<{ cached: boolean }>('/v1/cached');
        expect(intercepted).toEqual({ cached: true });
        expect(fetch_mock).toHaveBeenCalledTimes(1);
        unsub();
    });

    it('maps HTTP 400, 401, 403, 429, and 500 responses to typed subclasses and notifies error bus', async () => {
        const statuses: Array<[number, new (...args: never[]) => ApiError]> = [
            [400, ValidationError],
            [401, AuthError],
            [403, ForbiddenError],
            [429, RateLimitError],
            [500, ServerError],
        ];

        for (const [status, Ctor] of statuses) {
            const fetch_mock = vi.fn().mockResolvedValue(
                make_response(status, JSON.stringify({ title: 'Fail', detail: 'bad', error_code: `E_${status}` }))
            );
            const { service } = create_service(fetch_mock);
            const seen: ApiError[] = [];
            const unsub = service.subscribe_api_errors((e) => seen.push(e));

            await expect(service.request('/v1/op', { method: 'POST' })).rejects.toBeInstanceOf(Ctor);
            expect(seen).toHaveLength(1);
            expect(seen[0]).toBeInstanceOf(Ctor);
            unsub();
        }

        // Direct map_api_error coverage
        const raw = new ApiError('nope', 'about:blank', 418, null, null);
        expect(map_api_error_to_subclass(raw)).toBe(raw);
        expect(() => map_api_error(new ApiError('u', 'about:blank', 401, null, null))).toThrow(AuthError);
        expect(() => map_api_error(new Error('plain'))).toThrow('plain');
    });

    it('retries idempotent GET on transient 503 and network failure, but never retries non-idempotent POST', async () => {
        const fetch_mock = vi.fn()
            .mockResolvedValueOnce(make_response(503, 'Service Unavailable'))
            .mockResolvedValueOnce(make_response(200, JSON.stringify({ recovered: true })));
        const { service } = create_service(fetch_mock);
        vi.spyOn(service as unknown as { compute_backoff: () => number }, 'compute_backoff').mockReturnValue(1);

        const res = await service.request<{ recovered: boolean }>('/v1/agents', { method: 'GET', timeout: 5000 });
        expect(res.recovered).toBe(true);
        expect(fetch_mock).toHaveBeenCalledTimes(2);

        // Non-idempotent POST must not retry on TypeError('Failed to fetch')
        const post_fetch = vi.fn().mockRejectedValue(new TypeError('Failed to fetch'));
        const { service: post_svc } = create_service(post_fetch);
        await expect(post_svc.request('/v1/agents', { method: 'POST' })).rejects.toThrow(/Failed to fetch/i);
        expect(post_fetch).toHaveBeenCalledTimes(1);
    });

    it('blocks disallowed origins, missing URL, missing token, and >10MB responses', async () => {
        const fetch_mock = vi.fn();
        const { service: bad_origin } = create_service(fetch_mock, { url: 'https://evil.example' });
        await expect(bad_origin.request('/v1/agents')).rejects.toThrow(/not in the allowed origins list/i);

        const { service: empty_url } = create_service(fetch_mock, { url: '' });
        await expect(empty_url.request('/v1/agents')).rejects.toThrow(/tadpole_os_url is undefined/i);

        const { service: no_key } = create_service(fetch_mock, { key: '   ' });
        await expect(no_key.request('/v1/agents')).rejects.toThrow(/API token is missing/i);

        const big_fetch = vi.fn().mockResolvedValue(
            make_response(200, '{}', { 'content-length': String(11 * 1024 * 1024) })
        );
        const { service: big_svc } = create_service(big_fetch);
        await expect(big_svc.request('/v1/agents')).rejects.toThrow(/exceeded maximum allowed size/i);
    });

    it('sanitizes URLs, error details, trace headers, and abort signals safely', async () => {
        expect(validate_and_sanitize_url('http://user:pass@127.0.0.1:8000/')).toBe('http://127.0.0.1:8000');
        expect(() => validate_and_sanitize_url('')).toThrow(/URL is empty/i);
        expect(() => validate_and_sanitize_url('not a url')).toThrow(/Invalid URL format/i);
        expect(() => validate_and_sanitize_url('http://example.com')).toThrow(/must use HTTPS/i);

        const cleaned = sanitize_error_detail(
            'Error: postgres://admin:superpass@db.local/prod failed at /var/app/server.ts\n  at run (/app/index.ts:10:2)'
        );
        expect(cleaned).toContain('[CONNECTION_STRING_REDACTED]');
        expect(cleaned).toContain('[PATH_REDACTED]');
        expect(cleaned).not.toContain('superpass');
        expect(cleaned).not.toContain('at run');
        expect(sanitize_error_detail('')).toBe('');

        expect(() => mint_headers(globalThis.crypto, '')).toThrow(/API token is missing/i);
        const minted = mint_headers(globalThis.crypto, 'tok-123', 'custom-id');
        expect(minted.context.request_id).toBe('custom-id');

        const { signal: t_sig, clear } = with_timeout(5000);
        expect(t_sig.aborted).toBe(false);
        clear();

        const c1 = new AbortController();
        const c2 = new AbortController();
        const combined = combine_signals(c1.signal, c2.signal);
        expect(combined.signal?.aborted).toBe(false);
        c1.abort('STOP');
        expect(combined.signal?.aborted).toBe(true);
        combined.cleanup?.();
        expect(combine_signals(null, undefined)).toEqual({});
        expect(combine_signals(c2.signal).signal).toBe(c2.signal);

        await delay_with_signal(1);
        await expect(delay_with_signal(50, c1.signal)).rejects.toBe('STOP');
    });

    it('supports skip_auth public requests, throws AuthError on missing token, retries GET 429, and reports 501/422/INVALID_JSON', async () => {
        const fetch_mock = vi.fn()
            .mockResolvedValueOnce(make_response(200, JSON.stringify({ status: 'healthy' })))
            .mockResolvedValueOnce(make_response(429, 'Too Many Requests'))
            .mockResolvedValueOnce(make_response(200, JSON.stringify({ ok: true })))
            .mockResolvedValueOnce(make_response(501, 'Not Implemented'))
            .mockResolvedValueOnce(make_response(422, JSON.stringify({ title: 'Unprocessable', detail: 'bad field' })))
            .mockResolvedValueOnce(make_response(200, 'not-valid-json{{{'));

        const { service } = create_service(fetch_mock, { key: '' });
        vi.spyOn(service as unknown as { compute_backoff: () => number }, 'compute_backoff').mockReturnValue(1);

        // Public health probe with empty key succeeds and sends NO Authorization header
        const health = await service.request<{ status: string }>('/v1/engine/health', { skip_auth: true });
        expect(health).toEqual({ status: 'healthy' });
        expect(fetch_mock.mock.calls[0][1].headers.Authorization).toBeUndefined();

        // Missing token without skip_auth throws AuthError (subclass of ApiError)
        await expect(service.request('/v1/agents')).rejects.toBeInstanceOf(AuthError);
        expect(() => mint_headers(globalThis.crypto, '')).toThrow(AuthError);

        // With valid key: GET 429 retries, 501/422 emit friendly messages, invalid JSON emits INVALID_JSON ApiError
        const { service: authed } = create_service(fetch_mock, { key: 'valid-token-123456' });
        vi.spyOn(authed as unknown as { compute_backoff: () => number }, 'compute_backoff').mockReturnValue(1);

        const retried = await authed.request<{ ok: boolean }>('/v1/agents', { method: 'GET' });
        expect(retried).toEqual({ ok: true });

        await expect(authed.request('/v1/memory')).rejects.toThrow(/capability is disabled/i);
        await expect(authed.request('/v1/oversight/settings', { method: 'PUT' })).rejects.toThrow(/Unprocessable Entity/i);
        await expect(authed.request('/v1/broken')).rejects.toMatchObject({ error_code: 'INVALID_JSON' });
    });
});

// Metadata: [base-api-service_test]

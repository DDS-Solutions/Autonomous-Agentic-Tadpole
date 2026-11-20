/**
 * @docs ARCHITECTURE:TestSuites
 *
 * ### AI Assist Note
 * **API Factory & Legacy Bridge Tests**: Verifies `resolveCrypto`, `createApiService`,
 * and backward-compatible `api_request`/`get_headers` wrappers in `src/api/factory.ts` & `legacy.ts`.
 *
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Missing crypto adapter failure or broken legacy wrapper forwarding.
 * - **Telemetry Link**: Search `[factory_test]` in test logs.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { resolveCrypto, createApiService } from './factory';
import { api_request, get_headers, base_api_service_instance } from './legacy';
import { use_settings_store, clear_session_api_key } from '../stores/settings_store';

describe('API Factory & Legacy Wrappers', () => {
    beforeEach(() => {
        vi.restoreAllMocks();
        clear_session_api_key();
        use_settings_store.getState().reset_to_defaults();
    });

    it('resolves explicit override or global crypto and builds a working BaseApiService', async () => {
        const fake_crypto = { getRandomValues: vi.fn((a: Uint8Array) => a) } as unknown as Crypto;
        expect(resolveCrypto(fake_crypto)).toBe(fake_crypto);
        expect(resolveCrypto()).toBeDefined();

        const fetch_mock = vi.fn().mockResolvedValue({
            ok: true,
            status: 200,
            statusText: 'OK',
            headers: new Headers(),
            text: async () => JSON.stringify({ pong: true }),
        });

        const svc = createApiService({
            httpAdapter: { fetch: fetch_mock as unknown as typeof fetch, crypto: globalThis.crypto },
            settingsPort: {
                getSettings: () => ({
                    tadpole_os_url: 'http://127.0.0.1:8000',
                    tadpole_os_api_key: 'factory-token-123456',
                }),
            },
        });

        const res = await svc.request<{ pong: boolean }>('/v1/ping');
        expect(res).toEqual({ pong: true });

        expect(() =>
            createApiService({
                httpAdapter: { fetch: fetch_mock as unknown as typeof fetch, crypto: undefined as unknown as Crypto },
            })
        ).toThrow(/crypto adapter is mandatory/i);
    });

    it('delegates legacy api_request and get_headers to base_api_service_instance', async () => {
        use_settings_store.getState().update_setting('tadpole_os_api_key', 'legacy-token-123456');
        const minted = get_headers('req-legacy-1');
        expect(minted.headers.Authorization).toBe('Bearer legacy-token-123456');
        expect(minted.context.request_id).toBe('req-legacy-1');

        const req_spy = vi.spyOn(base_api_service_instance, 'request').mockResolvedValueOnce({ ok: 1 });
        const result = await api_request<{ ok: number }>('/v1/agents');
        expect(result).toEqual({ ok: 1 });
        expect(req_spy).toHaveBeenCalledWith('/v1/agents', {});
    });
});

// Metadata: [factory_test]

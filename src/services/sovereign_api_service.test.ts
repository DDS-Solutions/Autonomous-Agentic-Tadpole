/**
 * @docs ARCHITECTURE:TestSuites
 * ### AI Assist Note
 * Verifies configured session authentication, response boundaries, and promotion.
 * ### 🔍 Debugging & Observability
 * - Failure Path: Wrong credential/URL or invalid response accepted by the UI.
 * - Telemetry Link: Vitest assertion output.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { sovereign_api_service } from './sovereign_api_service';
import { use_settings_store } from '../stores/settings_store';

describe('finding regressions: sovereign API boundaries', () => {
    beforeEach(() => {
        use_settings_store.getState().update_setting('tadpole_os_url', 'http://127.0.0.1:8123');
        use_settings_store.getState().update_setting('tadpole_os_api_key', 'synthetic-regression-token');
        localStorage.removeItem('NEURAL_TOKEN');
    });
    afterEach(() => vi.unstubAllGlobals());

    it('uses the configured token and backend for all four actions and encodes path segments', async () => {
        const fetchMock = vi.fn()
            .mockResolvedValueOnce(new Response(JSON.stringify({ status: 'success', history: [] })))
            .mockResolvedValueOnce(new Response(JSON.stringify({ status: 'success', leaves: [] })))
            .mockResolvedValueOnce(new Response(JSON.stringify({ status: 'success' })))
            .mockResolvedValueOnce(new Response(JSON.stringify({ id: 'promoted-skill' })));
        vi.stubGlobal('fetch', fetchMock);
        await sovereign_api_service.history('mission/a', 'leaf#b');
        await sovereign_api_service.leaves('mission/a');
        await sovereign_api_service.revert('mission/a', 'leaf#b');
        expect(await sovereign_api_service.promote({ name: 'skill', description: 'test', cap_type: 'skill', content: 'test', agent_id: 'agent' })).toBe('promoted-skill');
        const paths = [
            '/v1/sovereign/missions/mission%2Fa/nodes/leaf%23b/history',
            '/v1/sovereign/missions/mission%2Fa/leaves',
            '/v1/sovereign/missions/mission%2Fa/nodes/leaf%23b/revert',
            '/v1/skills/promote',
        ];
        fetchMock.mock.calls.forEach(([url, options], index) => {
            expect(url).toBe(`http://127.0.0.1:8123${paths[index]}`);
            expect(options.headers.Authorization).toBe('Bearer synthetic-regression-token');
        });
        expect(fetchMock.mock.calls[2][1].method).toBe('POST');
        expect(fetchMock.mock.calls[3][1].method).toBe('POST');
    });

    it('rejects missing credentials before sending HTTP', async () => {
        use_settings_store.getState().update_setting('tadpole_os_api_key', '');
        const fetchMock = vi.fn();
        vi.stubGlobal('fetch', fetchMock);
        await expect(sovereign_api_service.history('mission', 'leaf')).rejects.toThrow('token is missing');
        expect(fetchMock).not.toHaveBeenCalled();
    });

    it('propagates authorization failures', async () => {
        vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response('Unauthorized', { status: 401 })));
        await expect(sovereign_api_service.leaves('mission')).rejects.toThrow('Unauthorized');
    });

    it.each([
        { status: 'success', history: null },
        { status: 'success', history: [{ id: 'bad', role: 'user', content: null }] },
        { status: 'success', history: [{ id: 'duplicate', role: 'user', content: 'one' }, { id: 'duplicate', role: 'user', content: 'two' }] },
        { status: 'error', history: [] },
    ])('rejects malformed history without publishing it', async (data) => {
        vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(JSON.stringify(data))));
        await expect(sovereign_api_service.history('mission', 'leaf')).rejects.toThrow('invalid session');
    });
});

// Metadata: [sovereign_api_service_test]

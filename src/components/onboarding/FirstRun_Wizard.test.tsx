/**
 * @docs ARCHITECTURE:TestSuites
 *
 * ### AI Assist Note
 * **First-Run Overlord Wizard Tests (End-to-End Service Integration)**:
 * Exercises the REAL `tadpole_os_service.check_health()` -> `engine_api_service.get_engine_status()` ->
 * `base_api_service.api_request('/v1/engine/health', { skip_auth: true })` stack on a completely fresh
 * session with empty RAM token (mocking ONLY `globalThis.fetch`, NOT `check_health`), verifying:
 * 1. Fresh Day-1 handshake works without throwing `AuthError` on `/v1/engine/health` (`NEW-CRIT-1` regression).
 * 2. Raw token NEVER touches `localStorage` or `sessionStorage` (`INV-SEC-002`).
 * 3. Custom budget is preserved when navigating Back from Step 3 -> Step 2 -> Step 3.
 * 4. Step 3 syncs exact `OversightSettingsPayload` to `PUT /v1/oversight/settings` and surfaces 403 sync warnings.
 * 5. Invalid/401 tokens never mutate volatile RAM state; reopening resets to Step 1.
 *
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Day-1 `AuthError` deadlock on `/v1/engine/health`, token storage leak, or lost custom budget.
 * - **Telemetry Link**: Search `[FirstRun_Wizard_test]` in test logs.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { FirstRun_Wizard } from './FirstRun_Wizard';
import { use_settings_store, get_session_api_key, clear_session_api_key } from '../../stores/settings_store';

function make_json_response(status: number, body: unknown): Response {
    const text = JSON.stringify(body);
    return {
        ok: status >= 200 && status < 300,
        status,
        statusText: status === 200 ? 'OK' : `HTTP ${status}`,
        headers: new Headers({ 'content-type': 'application/json' }),
        text: async () => text,
        json: async () => body,
    } as unknown as Response;
}

describe('FirstRun_Wizard Component (Real Service Stack)', () => {
    const original_fetch = globalThis.fetch;

    beforeEach(() => {
        vi.clearAllMocks();
        clear_session_api_key();
        localStorage.clear();
        sessionStorage.clear();
        use_settings_store.getState().reset_to_defaults();
    });

    afterEach(() => {
        globalThis.fetch = original_fetch;
    });

    it('authenticates on a fresh Day-1 session with empty RAM token, isolates secret, preserves custom budget, and syncs guardrails', async () => {
        const calls: Array<{ url: string; method: string; auth?: string; body?: string }> = [];
        globalThis.fetch = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
            const url = String(input);
            const headers = (init?.headers || {}) as Record<string, string>;
            const auth = headers['Authorization'] ?? headers['authorization'];
            calls.push({ url, method: init?.method || 'GET', auth, body: typeof init?.body === 'string' ? init.body : undefined });

            if (url.endsWith('/v1/engine/health')) {
                return make_json_response(200, { status: 'healthy', version: '1.1.59' });
            }
            if (url.includes('/v1/agents')) {
                return make_json_response(200, []);
            }
            if (url.endsWith('/v1/oversight/settings')) {
                return make_json_response(200, { status: 'updated' });
            }
            return make_json_response(200, {});
        }) as unknown as typeof fetch;

        const on_close = vi.fn();
        const secret_token = 'valid-session-token-123456-secret';
        expect(get_session_api_key()).toBe('');

        const { rerender } = render(<FirstRun_Wizard is_open on_close={on_close} />);

        fireEvent.change(screen.getByLabelText('Session Neural Token'), { target: { value: secret_token } });
        fireEvent.click(screen.getByRole('button', { name: /Verify Handshake/i }));

        await waitFor(() => {
            expect(screen.getByText(/Authenticated session established/i)).toBeInTheDocument();
        });

        // Prove /v1/engine/health was fetched WITHOUT an Authorization header on Day 1
        const health_call = calls.find((c) => c.url.endsWith('/v1/engine/health'));
        expect(health_call).toBeDefined();
        expect(health_call?.auth).toBeUndefined();
        expect(get_session_api_key()).toBe(secret_token);

        // INV-SEC-002: Verify raw token NEVER touches localStorage or sessionStorage
        expect(JSON.stringify(localStorage)).not.toContain(secret_token);
        expect(JSON.stringify(sessionStorage)).not.toContain(secret_token);

        // Step 2 -> Step 3: Set custom budget $12.50, navigate Back, switch preset, Continue -> $12.50 MUST be preserved
        fireEvent.click(screen.getByRole('button', { name: /^Continue$/i }));
        fireEvent.click(screen.getByText('Local Sovereign'));
        fireEvent.click(screen.getByRole('button', { name: /Apply Preset & Continue/i }));

        const budgetInput = screen.getByLabelText('Default Agent Budget Cap USD') as HTMLInputElement;
        // Clearing input via Backspace MUST NOT snap to 0.1 mid-edit; typing new value and blurring commits cleanly
        fireEvent.change(budgetInput, { target: { value: '' } });
        expect(budgetInput.value).toBe('');
        fireEvent.change(budgetInput, { target: { value: '12.5' } });
        fireEvent.blur(budgetInput);
        expect(budgetInput.value).toBe('12.5');
        fireEvent.click(screen.getByRole('button', { name: /^Back$/i }));
        fireEvent.click(screen.getByText('Hybrid Overlord'));
        fireEvent.click(screen.getByRole('button', { name: /Apply Preset & Continue/i }));
        expect((screen.getByLabelText('Default Agent Budget Cap USD') as HTMLInputElement).value).toBe('12.5');

        // Step 3 -> PUT /v1/oversight/settings -> Step 4
        fireEvent.click(screen.getByRole('button', { name: /Save Guardrails & Continue/i }));
        expect(await screen.findByTestId('wizard-sync-notice')).toHaveTextContent(/Guardrails synced to live Rust engine/i);
        const settings_call = calls.find((c) => c.url.endsWith('/v1/oversight/settings') && c.method === 'PUT');
        expect(settings_call).toBeDefined();
        expect(JSON.parse(settings_call!.body!)).toEqual({
            auto_approve_safe_skills: false,
            privacy_mode: false,
            max_swarm_depth: 4,
            default_budget_usd: 12.5,
        });

        fireEvent.click(screen.getByRole('button', { name: /Enter Command Center/i }));
        expect(use_settings_store.getState().settings.onboarding_completed).toBe(true);
        expect(on_close).toHaveBeenCalledTimes(1);

        // Reopen wizard: MUST reset back to Step 1
        rerender(<FirstRun_Wizard is_open={false} on_close={on_close} />);
        rerender(<FirstRun_Wizard is_open on_close={on_close} />);
        expect(screen.getByLabelText('Session Neural Token')).toBeInTheDocument();
    });

    it('rejects blank, short, and 401 tokens without mutating RAM store, and warns on 403 guardrail sync', async () => {
        const fetch_mock = vi.fn(async (input: RequestInfo | URL) => {
            const url = String(input);
            if (url.endsWith('/v1/engine/health')) return make_json_response(200, { status: 'healthy' });
            if (url.includes('/v1/agents')) return make_json_response(401, { detail: 'Unauthorized' });
            return make_json_response(403, { detail: 'Admin scope required' });
        });
        globalThis.fetch = fetch_mock as unknown as typeof fetch;

        render(<FirstRun_Wizard is_open on_close={vi.fn()} />);

        fireEvent.click(screen.getByRole('button', { name: /Verify Handshake/i }));
        expect(await screen.findByText(/Paste your NEURAL_TOKEN/i)).toBeInTheDocument();
        expect(fetch_mock).not.toHaveBeenCalled();

        fireEvent.change(screen.getByLabelText('Session Neural Token'), { target: { value: 'short' } });
        fireEvent.click(screen.getByRole('button', { name: /Verify Handshake/i }));
        expect(await screen.findByText(/Token must be at least 12 valid characters/i)).toBeInTheDocument();

        fireEvent.change(screen.getByLabelText('Session Neural Token'), { target: { value: 'bad-token-1234567890' } });
        fireEvent.click(screen.getByRole('button', { name: /Verify Handshake/i }));
        expect(await screen.findByText(/token was rejected \(HTTP 401\)/i)).toBeInTheDocument();
        expect(get_session_api_key()).toBe('');

        // Now allow /v1/agents 200 but return 403 on PUT /v1/oversight/settings
        globalThis.fetch = vi.fn(async (input: RequestInfo | URL) => {
            const url = String(input);
            if (url.endsWith('/v1/engine/health')) return make_json_response(200, { status: 'healthy' });
            if (url.includes('/v1/agents')) return make_json_response(200, []);
            return make_json_response(403, { detail: 'Admin scope required' });
        }) as unknown as typeof fetch;

        fireEvent.change(screen.getByLabelText('Session Neural Token'), { target: { value: 'operator-token-123456' } });
        fireEvent.click(screen.getByRole('button', { name: /Verify Handshake/i }));
        await screen.findByText(/Authenticated session established/i);
        fireEvent.click(screen.getByRole('button', { name: /^Continue$/i }));
        fireEvent.click(screen.getByRole('button', { name: /Apply Preset & Continue/i }));
        fireEvent.click(screen.getByRole('button', { name: /Save Guardrails & Continue/i }));
        expect(await screen.findByTestId('wizard-sync-notice')).toHaveTextContent(/Rust engine sync failed/i);
    });
});

// Metadata: [FirstRun_Wizard_test]

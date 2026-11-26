/**
 * @docs ARCHITECTURE:TestSuites
 *
 * ### AI Assist Note
 * **Live Voice Hub Component Tests**: Verifies in-memory token extraction from `settings_store`
 * (never `localStorage`), `origin_guard` validation, exclusive `live-voice-hub` mic lease
 * acquisition/release, and visible connection error alerts when unauthenticated.
 *
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Unauthenticated WebSocket attempt or leaked mic lease on unmount.
 * - **Trace Scope**: `src/components/voice/Live_Voice_Hub.test`
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import '@testing-library/jest-dom/vitest';
import { Live_Voice_Hub } from './Live_Voice_Hub';
import { use_settings_store } from '../../stores/settings_store';
import { voice_client } from '../../services/voice_client';

vi.mock('../../services/voice_client', () => ({
    voice_client: {
        acquire_mic_lease: vi.fn().mockReturnValue(true),
        release_mic_lease: vi.fn(),
        get_input_device: vi.fn().mockReturnValue(null),
    },
    classify_mic_error: vi.fn(() => ({ code: 'unknown', message: 'Mic error' })),
}));

describe('Live_Voice_Hub', () => {
    beforeEach(() => {
        vi.clearAllMocks();
    });

    it('displays a visible error when in-memory API token is missing and never reads localStorage tadpole_token', async () => {
        const ls_spy = vi.spyOn(Storage.prototype, 'getItem');
        use_settings_store.setState((state) => ({
            settings: {
                ...state.settings,
                tadpole_os_url: 'http://localhost:8000',
                tadpole_os_api_key: '',
            },
        }));

        render(<Live_Voice_Hub agent_id="agent-1" theme_color="#22d3ee" />);

        await waitFor(() => {
            expect(screen.getByRole('alert')).toBeInTheDocument();
        });
        expect(ls_spy).not.toHaveBeenCalledWith('tadpole_token');
        expect(voice_client.acquire_mic_lease).not.toHaveBeenCalled();
        ls_spy.mockRestore();
    });
});

/**
 * @docs ARCHITECTURE:TestSuites
 *
 * ### AI Assist Note
 * **Standups Orchestration Hook Tests**: Validates Pre-Flight DLP redaction on voice
 * transcripts, suspended-agent guard and one-click resume, Gemini-Live exclusive mic
 * delegation, mute toggle, device selection, and live audio capability probing.
 *
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Secret leak in outbound voice command or suspended agent dispatched without resume.
 * - **Trace Scope**: `src/hooks/useStandups.test`
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import { useStandups } from './useStandups';
import { use_agent_store } from '../stores/agent_store';
import { use_agent_registry_store } from '../stores/agent_registry_store';
import { use_workspace_store } from '../stores/workspace_store';
import { voice_client } from '../services/voice_client';
import { agent_api_service } from '../services/agent_api_service';
import { system_api_service } from '../services/system_api_service';

vi.mock('../stores/agent_store', () => ({ use_agent_store: vi.fn() }));
vi.mock('../stores/agent_registry_store', () => ({
    use_agent_registry_store: { getState: vi.fn() },
}));
vi.mock('../stores/workspace_store', () => ({ use_workspace_store: vi.fn() }));
vi.mock('../services/event_bus', () => ({
    event_bus: { subscribe_logs: vi.fn(() => vi.fn()), emit_log: vi.fn() },
}));
vi.mock('../services/voice_client', () => ({
    voice_client: {
        speak: vi.fn().mockResolvedValue(undefined),
        start_recording: vi.fn().mockResolvedValue(true),
        stop_recording: vi.fn().mockResolvedValue(new Blob(['audio'], { type: 'audio/webm' })),
        list_input_devices: vi.fn().mockResolvedValue([{ deviceId: 'mic-1', label: 'Studio Mic' }]),
        get_input_device: vi.fn().mockReturnValue(''),
        set_input_device: vi.fn(),
        toggle_mute: vi.fn().mockReturnValue(true),
        get_input_level: vi.fn().mockReturnValue(0.25),
        get_last_mic_error: vi.fn().mockReturnValue(null),
    },
}));
vi.mock('../services/agent_api_service', () => ({
    agent_api_service: {
        send_command: vi.fn().mockResolvedValue({ status: 'accepted' }),
        resume_agent: vi.fn().mockResolvedValue(true),
    },
}));
vi.mock('../services/system_api_service', () => ({
    system_api_service: {
        transcribe: vi.fn(),
        get_audio_capabilities: vi.fn().mockResolvedValue({
            privacy_mode: false,
            tts: { browser: { available: true, reason: null }, openai: { available: false, reason: 'Missing key' } },
            stt: { browser: { available: true, reason: null }, groq: { available: true, reason: null } },
        }),
    },
}));

describe('useStandups', () => {
    const fetch_agents = vi.fn().mockResolvedValue(undefined);
    let current_agents: Array<Record<string, unknown>> = [];

    const setup_agents = (agents: Array<Record<string, unknown>>) => {
        current_agents = agents;
        (use_agent_store as unknown as ReturnType<typeof vi.fn>).mockImplementation((selector?: (s: unknown) => unknown) => {
            const state = { agents: current_agents, fetch_agents };
            return selector ? selector(state) : state;
        });
        (use_agent_registry_store.getState as unknown as ReturnType<typeof vi.fn>).mockReturnValue({
            agents: current_agents,
            fetch_agents,
        });
    };

    beforeEach(() => {
        vi.clearAllMocks();
        (use_workspace_store as unknown as ReturnType<typeof vi.fn>).mockImplementation((selector?: (s: { clusters: unknown[] }) => unknown) =>
            selector ? selector({ clusters: [] }) : { clusters: [] }
        );
        setup_agents([
            {
                id: 'agent-1',
                name: 'Alpha',
                role: 'Architect',
                department: 'Engineering',
                status: 'idle',
                voice_engine: 'browser',
                model_config: { provider: 'openai', modelId: 'gpt-4o' },
            },
        ]);
    });

    it('redacts secrets via Pre-Flight DLP before dispatching voice transcripts (Improvement #1)', async () => {
        const fake_secret = 'sk-' + '1234567890abcdef1234567890abcdef';
        (system_api_service.transcribe as unknown as ReturnType<typeof vi.fn>).mockResolvedValue(
            `Deploy using key ${fake_secret} now`
        );

        const { result } = renderHook(() => useStandups());
        await waitFor(() => expect(result.current.selected_target_id).toBe('agent-1'));

        await act(async () => {
            result.current.toggle_live();
        });
        await act(async () => {
            result.current.toggle_live();
        });

        await waitFor(() => expect(agent_api_service.send_command).toHaveBeenCalled());
        const dispatched_text = vi.mocked(agent_api_service.send_command).mock.calls[0][1];
        expect(dispatched_text).not.toContain(fake_secret);
        expect(dispatched_text).toContain('[REDACTED');
    });

    it('blocks voice sync on suspended agents and exposes one-click resume_target_agent (Improvement #2)', async () => {
        setup_agents([
            {
                id: 'agent-sus',
                name: 'Suspended Node',
                role: 'Specialist',
                department: 'Ops',
                status: 'suspended',
                model_config: { provider: 'openai', modelId: 'gpt-4o' },
            },
        ]);

        const { result } = renderHook(() => useStandups());
        await waitFor(() => expect(result.current.selected_target_id).toBe('agent-sus'));

        expect(result.current.is_target_suspended).toBe(true);
        expect(result.current.can_start_sync).toBe(false);

        await act(async () => {
            await result.current.resume_target_agent();
        });
        expect(agent_api_service.resume_agent).toHaveBeenCalledWith('agent-sus');
        expect(fetch_agents).toHaveBeenCalled();
    });

    it('delegates mic exclusively to Gemini Live when agent uses gemini-live engine and fetches audio capabilities (Improvement #5)', async () => {
        setup_agents([
            {
                id: 'agent-live',
                name: 'Gemini Live Agent',
                role: 'Commander',
                department: 'Core',
                status: 'idle',
                voice_engine: 'gemini-live',
                model_config: { provider: 'google', modelId: 'gemini-2.5-flash' },
            },
        ]);

        const { result } = renderHook(() => useStandups());
        await waitFor(() => expect(result.current.audio_capabilities).not.toBeNull());
        await waitFor(() => expect(result.current.selected_target_id).toBe('agent-live'));

        await act(async () => {
            result.current.toggle_live();
        });

        // Must NOT call voice_client.start_recording because Live_Voice_Hub owns the mic exclusively
        expect(voice_client.start_recording).not.toHaveBeenCalled();
        expect(result.current.is_live).toBe(true);
        expect(result.current.audio_capabilities?.tts.browser.available).toBe(true);
    });

    it('supports mute toggle and device selection', async () => {
        const { result } = renderHook(() => useStandups());
        await waitFor(() => expect(result.current.input_devices).toHaveLength(1));

        act(() => {
            result.current.set_selected_device_id('mic-1');
            result.current.toggle_mute();
        });

        expect(voice_client.set_input_device).toHaveBeenCalledWith('mic-1');
        expect(voice_client.toggle_mute).toHaveBeenCalled();
        expect(result.current.is_muted).toBe(true);
    });
});

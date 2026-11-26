/**
 * @docs ARCHITECTURE:TestSuites
 *
 * ### AI Assist Note
 * **Validation of the Automated Agent Standup and Progress Reporting view.**
 * Verifies the Neural Sync Interface (`Standups`), live timer, agent/cluster targeting,
 * TTS dispatch on incoming `event_bus` logs, recording -> transcription -> command dispatch,
 * and cluster Alpha handoff.
 *
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Missing mock return value on `voice_client.start_recording` or store selector drift.
 * - **Trace Scope**: `src/pages/Standups.test`
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, act, waitFor } from '@testing-library/react';
import '@testing-library/jest-dom/vitest';
import Standups from './Standups';
import { use_workspace_store } from '../stores/workspace_store';
import { use_agent_store } from '../stores/agent_store';
import { use_agent_registry_store } from '../stores/agent_registry_store';
import { event_bus } from '../services/event_bus';
import { voice_client } from '../services/voice_client';
import { agent_api_service } from '../services/agent_api_service';
import { system_api_service } from '../services/system_api_service';

vi.mock('../stores/workspace_store');
vi.mock('../stores/agent_store', () => ({
    use_agent_store: vi.fn(),
}));
vi.mock('../stores/agent_registry_store', () => ({
    use_agent_registry_store: {
        getState: vi.fn(),
    },
}));
vi.mock('../services/event_bus', () => ({
    event_bus: {
        subscribe_logs: vi.fn(),
        emit_log: vi.fn(),
        get_history: vi.fn(() => []),
    },
}));
vi.mock('../services/voice_client', () => ({
    voice_client: {
        speak: vi.fn().mockResolvedValue(undefined),
        start_recording: vi.fn().mockResolvedValue(true),
        stop_recording: vi.fn().mockResolvedValue(new Blob(['audio'], { type: 'audio/webm' })),
        list_input_devices: vi.fn().mockResolvedValue([]),
        get_input_device: vi.fn().mockReturnValue(null),
        set_input_device: vi.fn(),
        toggle_mute: vi.fn().mockReturnValue(false),
        get_input_level: vi.fn().mockReturnValue(0),
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
            tts: { browser: { available: true, reason: null } },
            stt: { browser: { available: true, reason: null }, groq: { available: true, reason: null } },
        }),
    },
}));

global.ResizeObserver = class ResizeObserver {
    observe() {}
    unobserve() {}
    disconnect() {}
};

describe('Standups Page', () => {
    const mock_agents = [
        {
            id: 'x-agent-1',
            name: 'Alpha Agent',
            role: 'Dev',
            department: 'Engineering',
            status: 'idle',
            model_config: { provider: 'test', modelId: 'test-model' },
        },
        {
            id: 'x-agent-2',
            name: 'Beta Agent',
            role: 'Tester',
            department: 'Engineering',
            status: 'idle',
            model_config: { provider: 'test', modelId: 'test-model' },
        },
    ];

    const mock_clusters = [
        {
            id: 'cluster-1',
            name: 'Frontend Core',
            department: 'Engineering',
            alpha_id: 'x-agent-1',
            budget_usd: 100,
        },
    ];

    let event_bus_callback: (event: Record<string, unknown>) => void;
    const mock_fetch_agents = vi.fn().mockResolvedValue(undefined);

    beforeEach(() => {
        vi.clearAllMocks();
        vi.useRealTimers();

        (use_workspace_store as unknown as ReturnType<typeof vi.fn>).mockImplementation((selector?: (s: { clusters: typeof mock_clusters }) => unknown) =>
            selector ? selector({ clusters: mock_clusters }) : { clusters: mock_clusters }
        );
        (use_agent_store as unknown as ReturnType<typeof vi.fn>).mockImplementation((selector?: (s: { agents: typeof mock_agents; fetch_agents: typeof mock_fetch_agents }) => unknown) => {
            const state = { agents: mock_agents, fetch_agents: mock_fetch_agents };
            return selector ? selector(state) : state;
        });
        (use_agent_registry_store.getState as unknown as ReturnType<typeof vi.fn>).mockReturnValue({
            agents: mock_agents,
            fetch_agents: mock_fetch_agents,
        });

        (event_bus.subscribe_logs as unknown as ReturnType<typeof vi.fn>).mockImplementation((cb) => {
            event_bus_callback = cb;
            return vi.fn();
        });

        (system_api_service.transcribe as unknown as ReturnType<typeof vi.fn>).mockResolvedValue('Hello Alpha');
        (voice_client.start_recording as unknown as ReturnType<typeof vi.fn>).mockResolvedValue(true);
        (voice_client.stop_recording as unknown as ReturnType<typeof vi.fn>).mockResolvedValue(
            new Blob(['audio'], { type: 'audio/webm' })
        );
        (system_api_service.get_audio_capabilities as unknown as ReturnType<typeof vi.fn>).mockResolvedValue({
            privacy_mode: false,
            tts: { browser: { available: true, reason: null } },
            stt: { browser: { available: true, reason: null }, groq: { available: true, reason: null } },
        });
    });

    it('renders the neural sync interface and live transcript base', async () => {
        vi.useFakeTimers();
        await act(async () => {
            render(<Standups />);
        });

        expect(screen.getAllByText(/Neural Sync Interface/i).length).toBeGreaterThan(0);
        expect(screen.getByText(/Status: Ready for Handshake/i)).toBeInTheDocument();
        expect(screen.getByText(/Live Transcript/i)).toBeInTheDocument();
        expect(screen.getAllByText(/System/i).length).toBeGreaterThan(0);
        expect(screen.getByText(/Voice Communications Online/i)).toBeInTheDocument();

        const start_button = screen.getByRole('button', { name: /Start Sync/i });
        expect(start_button).toBeInTheDocument();

        await act(async () => {
            fireEvent.click(start_button);
        });

        expect(screen.getByText(/00:00 • ENCRYPTED LIVE CHANNEL/i)).toBeInTheDocument();
        expect(screen.getByRole('button', { name: /End Sync/i })).toBeInTheDocument();
        expect(screen.getByLabelText(/End Sync/i)).toBeInTheDocument();

        await act(async () => {
            vi.advanceTimersByTime(1000);
        });

        expect(screen.getByText(/00:01 • ENCRYPTED LIVE CHANNEL/i)).toBeInTheDocument();
    });

    it('populates agent and cluster targets', async () => {
        await act(async () => {
            render(<Standups />);
        });

        await act(async () => {
            fireEvent.click(screen.getByText('Mission Cluster'));
        });

        expect(screen.getByText('FRONTEND CORE')).toBeInTheDocument();

        await act(async () => {
            fireEvent.click(screen.getByText('Agent Node'));
        });

        expect(screen.getByText('ALPHA AGENT')).toBeInTheDocument();
    });

    it('handles incoming event_bus messages and text-to-speech for agents', async () => {
        await act(async () => {
            render(<Standups />);
        });

        act(() => {
            if (event_bus_callback) {
                event_bus_callback({ source: 'Agent', agent_id: 'Alpha Agent', text: 'I am ready.' });
            }
        });

        expect(screen.getByText('Alpha Agent')).toBeInTheDocument();
        expect(screen.getByText('I am ready.')).toBeInTheDocument();
        expect(voice_client.speak).toHaveBeenCalledWith('I am ready.');
    });

    it('starts and stops recording, transcribes, and sends command to an agent target', async () => {
        await act(async () => {
            render(<Standups />);
        });

        const start_button = screen.getByRole('button', { name: /Start Sync/i });
        await act(async () => {
            fireEvent.click(start_button);
        });
        expect(voice_client.start_recording).toHaveBeenCalled();

        const end_button = screen.getByRole('button', { name: /End Sync/i });
        await act(async () => {
            fireEvent.click(end_button);
        });
        expect(voice_client.stop_recording).toHaveBeenCalled();

        await waitFor(() => {
            expect(system_api_service.transcribe).toHaveBeenCalled();
            expect(event_bus.emit_log).toHaveBeenCalledWith(
                expect.objectContaining({
                    source: 'User',
                    text: 'Hello Alpha (To: Alpha Agent)',
                })
            );
            expect(agent_api_service.send_command).toHaveBeenCalledWith(
                'x-agent-1',
                'Hello Alpha',
                'test-model',
                'test',
                undefined,
                undefined,
                undefined,
                undefined,
                expect.any(Boolean),
                undefined,
                undefined,
                undefined,
                undefined,
                '0',
                true,
                undefined,
                undefined,
                expect.stringMatching(/chat|plan|swarm/),
                undefined
            );
        });
    });

    it('handles neural handoff to an entire cluster (routing to Alpha node)', async () => {
        await act(async () => {
            render(<Standups />);
        });

        await act(async () => {
            fireEvent.click(screen.getByText('Mission Cluster'));
        });

        const select = screen.getByRole('combobox');
        expect(select).toHaveValue('cluster-1');

        const start_button = screen.getByRole('button', { name: /Start Sync/i });
        await act(async () => {
            fireEvent.click(start_button);
        });

        const end_button = screen.getByRole('button', { name: /End Sync/i });
        await act(async () => {
            fireEvent.click(end_button);
        });

        await waitFor(() => {
            expect(agent_api_service.send_command).toHaveBeenCalledWith(
                'x-agent-1',
                '[CLUSTER COMMAND: Frontend Core] Hello Alpha',
                'test-model',
                'test',
                'cluster-1',
                'Engineering',
                100,
                undefined,
                expect.any(Boolean),
                undefined,
                undefined,
                undefined,
                undefined,
                '0',
                true,
                undefined,
                undefined,
                expect.stringMatching(/chat|plan|swarm/),
                undefined
            );
        });
    });
});

// Metadata: [Standups_test]

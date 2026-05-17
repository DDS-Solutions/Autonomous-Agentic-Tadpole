/*
@docs ARCHITECTURE:UI

### AI Assist Note
**🛡️ Tadpole OS: Settings Handoff**
Core system module providing specialized functionality for the agent swarm.

### 🔍 Debugging & Observability
- **Failure Path**: Unexpected execution drift or type compatibility issues.
- **Telemetry Link**: Traced via active system logging channels.
*/

import { render, screen, fireEvent, act } from '@testing-library/react';
import '@testing-library/jest-dom/vitest';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import Settings from './Settings';
import { MemoryRouter } from 'react-router-dom';

// Mock dependencies
const mock_settings = {
    tadpole_os_url: 'http://localhost:8000',
    tadpole_os_api_key: 'test-token',
    theme: 'zinc',
    density: 'compact',
    default_model: 'GPT-4o',
    default_temperature: 0.7,
    auto_approve_safe_skills: true,
    max_agents: 50,
    max_clusters: 10,
    max_swarm_depth: 5,
    max_task_length: 32768,
    default_budget_usd: 1.0,
    is_safe_mode: true,
    privacy_mode: false,
    browser_specialist_model_id: 'onnx-community/Gemma-2b-it-v2',
    computer_architect_url: 'http://localhost:11434',
    enable_neural_handoff: true,
    sentinel_mode: false,
};

vi.mock('../stores/settings_store', () => ({
    get_settings: vi.fn(() => ({ ...mock_settings })),
    save_settings: vi.fn(() => null),
    use_settings_store: {
        getState: vi.fn(() => ({ settings: { ...mock_settings } }))
    }
}));

vi.mock('../stores/model_store', () => ({
    use_model_store: vi.fn((selector) => selector({ models: [{ id: 'm1', name: 'GPT-4o' }] }))
}));

vi.mock('../services/system_api_service', () => ({
    system_api_service: {
        update_governance_settings: vi.fn().mockResolvedValue({})
    }
}));

vi.mock('../i18n', () => ({
    i18n: {
        t: (key: string) => key,
    },
}));

vi.mock('../components/ui', () => ({
    Tooltip: ({ children, content }: any) => <div title={content}>{children}</div>
}));

describe('Settings: Sovereign Neural Handoff Verification', () => {
    beforeEach(() => {
        vi.clearAllMocks();
    });

    it('renders the Neural Handoff section correctly', () => {
        render(
            <MemoryRouter>
                <Settings />
            </MemoryRouter>
        );

        expect(screen.getByText('settings.header_neural_handoff')).toBeInTheDocument();
        expect(screen.getByText('settings.label_browser_model')).toBeInTheDocument();
        expect(screen.getByText('settings.label_computer_url')).toBeInTheDocument();
    });

    it('updates browser specialist model id', () => {
        render(
            <MemoryRouter>
                <Settings />
            </MemoryRouter>
        );

        const input = screen.getByPlaceholderText('onnx-community/Gemma-2b-it-v2') as HTMLInputElement;
        fireEvent.change(input, { target: { name: 'browser_specialist_model_id', value: 'custom-model' } });
        expect(input.value).toBe('custom-model');
    });

    it('updates computer architect url', () => {
        render(
            <MemoryRouter>
                <Settings />
            </MemoryRouter>
        );

        const input = screen.getByPlaceholderText('http://localhost:11434') as HTMLInputElement;
        fireEvent.change(input, { target: { name: 'computer_architect_url', value: 'http://remote:11434' } });
        expect(input.value).toBe('http://remote:11434');
    });

    it('toggles neural handoff and sentinel mode', async () => {
        render(
            <MemoryRouter>
                <Settings />
            </MemoryRouter>
        );

        const handoff_toggle = document.querySelector('.group\\/toggle button');
        const sentinel_toggle = document.querySelectorAll('.group\\/toggle button')[1];

        if (!handoff_toggle || !sentinel_toggle) throw new Error('Toggles not found');

        // Toggle Handoff
        fireEvent.click(handoff_toggle);
        // Toggle Sentinel
        fireEvent.click(sentinel_toggle);

        const save_btn = screen.getByText('settings.save_changes');
        await act(async () => {
            fireEvent.click(save_btn);
        });

        const { save_settings } = await import('../stores/settings_store');
        expect(save_settings).toHaveBeenCalledWith(expect.objectContaining({
            enable_neural_handoff: false, // Toggled from true
            sentinel_mode: true // Toggled from false
        }));
    });
});

// Metadata: [Settings_Handoff_test]

// Metadata: [Settings_Handoff_test]

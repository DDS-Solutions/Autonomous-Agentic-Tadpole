/*
@docs ARCHITECTURE:UI

### AI Assist Note
**🛡️ Tadpole OS: Model Manager Hardening**
Core system module providing specialized functionality for the agent swarm.

### 🔍 Debugging & Observability
- **Failure Path**: Unexpected execution drift or type compatibility issues.
- **Telemetry Link**: Traced via active system logging channels.
*/

import { render, screen } from '@testing-library/react';
import '@testing-library/jest-dom/vitest';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import Model_Manager from './Model_Manager';

// Mock state using vi.hoisted for reliable sharing with vi.mock
const { mock_state } = vi.hoisted(() => ({
    mock_state: { 
        is_locked: false,
        encrypted_configs: {},
        models: [],
        providers: [],
        loading: false
    }
}));

// Mock functions
const mock_unlock = vi.fn();
const mock_lock = vi.fn();

vi.mock('../stores/vault_store', () => ({
    use_vault_store: vi.fn((selector) => {
        const state = {
            is_locked: mock_state.is_locked,
            encrypted_configs: mock_state.encrypted_configs,
            unlock: mock_unlock,
            lock: mock_lock,
            reset_vault: vi.fn(),
            reset_inactivity_timer: vi.fn(),
        };
        return selector ? selector(state) : state;
    })
}));

vi.mock('../stores/model_store', () => ({
    use_model_store: vi.fn((selector) => {
        const state = {
            models: mock_state.models,
            add_model: vi.fn(),
            edit_model: vi.fn(),
            delete_model: vi.fn(),
            loading: mock_state.loading,
            get_models: vi.fn(),
            sync_models: vi.fn(),
        };
        return selector ? selector(state) : state;
    }),
    ModelEntry: vi.fn()
}));

vi.mock('../stores/provider_store', () => ({
    use_provider_store: vi.fn((selector) => {
        const state = {
            providers: mock_state.providers,
            add_provider: vi.fn(),
            edit_provider: vi.fn(),
            delete_provider: vi.fn(),
            loading: mock_state.loading,
            get_providers: vi.fn(),
            set_provider_config: vi.fn(),
        };
        return selector ? selector(state) : state;
    })
}));

vi.mock('../stores/header_store', () => ({
    use_header_store: vi.fn((selector) => {
        const state = {
            set_header_actions: vi.fn(),
            clear_header_actions: vi.fn(),
        };
        return selector ? selector(state) : state;
    }),
}));

vi.mock('../i18n', () => ({
    i18n: {
        t: (key: string) => key,
    },
}));

vi.mock('../components/ui', () => ({
    Tooltip: ({ children, content }: any) => <div title={content}>{children}</div>
}));

describe('Model_Manager Hardening Verification', () => {
    beforeEach(() => {
        vi.clearAllMocks();
        mock_state.is_locked = false;
        mock_state.providers = [];
        mock_state.models = [];
    });

    it('displays the empty state for the Provider Grid when no providers exist', () => {
        render(<Model_Manager />);
        expect(screen.getByText('model_manager.grid.empty_state')).toBeInTheDocument();
        expect(screen.getByText('model_manager.grid.empty_hint')).toBeInTheDocument();
    });

    it('displays the empty state for the Model Inventory Table when no models exist', () => {
        render(<Model_Manager />);
        expect(screen.getByText('model_manager.inventory.empty_state')).toBeInTheDocument();
        expect(screen.getByText('model_manager.inventory.empty_hint')).toBeInTheDocument();
    });

    it('disables the Provision Node button and shows a tooltip if no providers exist', () => {
        render(<Model_Manager />);
        const button = screen.getByLabelText('model_manager.inventory.btn_add');
        expect(button).toBeDisabled();
        
        // Tooltip is mocked to render as a title on the wrapper div
        const tooltip_wrapper = screen.getByTitle('model_manager.inventory.btn_add_disabled_tooltip');
        expect(tooltip_wrapper).toBeInTheDocument();
    });

    it('enables the Provision Node button when a provider is added', () => {
        mock_state.providers = [
            { id: 'p1', name: 'OpenAI', icon: '⚡', protocol: 'openai' }
        ];
        render(<Model_Manager />);
        
        const button = screen.getByLabelText('model_manager.inventory.btn_add');
        expect(button).not.toBeDisabled();
        
        const tooltip_wrapper = screen.getByTitle('model_manager.inventory.tooltip_provision');
        expect(tooltip_wrapper).toBeInTheDocument();
    });
});

// Metadata: [Model_Manager_Hardening_test]

// Metadata: [Model_Manager_Hardening_test]

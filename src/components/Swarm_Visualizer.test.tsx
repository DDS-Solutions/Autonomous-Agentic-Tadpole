/**
 * @docs ARCHITECTURE:TestSuites
 * 
 * ### AI Assist Note
 * **Verification of the Swarm Visualizer and Quick Command Bar**. 
 * Ensures real-time canvas rendering, node click selection, interactive
 * directive dispatch via tadpole_os_service, and mission hub inspection.
 * 
 * ### 🔍 Debugging & Observability
 * - **Telemetry Link**: Search `[Swarm_Visualizer_test]` in observability logs.
 */

import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import '@testing-library/jest-dom/vitest';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { Swarm_Visualizer } from './Swarm_Visualizer';

// Mock react-force-graph-2d
vi.mock('react-force-graph-2d', () => ({
    default: vi.fn(({ graphData, onNodeClick }: any) => (
        <div data-testid="force-graph-2d">
            {graphData?.nodes?.map((node: any) => (
                <button
                    key={node.id}
                    data-testid={`graph-node-${node.id}`}
                    onClick={() => onNodeClick && onNodeClick(node)}
                >
                    {node.name}
                </button>
            ))}
        </div>
    )),
}));

// Mock stores
const mock_agents = [
    { id: 'agent-alpha', name: 'Alpha Agent', department: 'Core', model_id: 'gemini-1.5-flash', provider: 'google' },
    { id: 'agent-beta', name: 'Beta Worker', department: 'Ops', model_id: 'gpt-4o', provider: 'openai' },
];

vi.mock('../stores/agent_store', () => ({
    use_agent_store: vi.fn(() => ({
        agents: mock_agents,
    })),
}));

const mock_set_selected_agent_id = vi.fn();
const mock_set_scope = vi.fn();
const mock_set_target_agent = vi.fn();

vi.mock('../stores/sovereign_store', () => ({
    use_sovereign_store: vi.fn((selector) => {
        const state = {
            set_selected_agent_id: mock_set_selected_agent_id,
            set_scope: mock_set_scope,
            set_target_agent: mock_set_target_agent,
        };
        return selector ? selector(state) : state;
    }),
}));

// Mock services
const mock_send_command = vi.fn().mockResolvedValue('task-123');

vi.mock('../services/tadpoleos_service', () => ({
    tadpole_os_service: {
        send_command: (...args: any[]) => mock_send_command(...args),
        resolve_provider: vi.fn((_m: string) => 'google'),
    },
}));

vi.mock('../services/base_api_service', () => ({
    api_request: vi.fn().mockResolvedValue({
        nodes: [
            { id: 'agent-alpha', label: 'Alpha Agent', type: 'agent', status: 'active' },
            { id: 'ms-hub-1', label: 'Quantum Mission', type: 'mission', status: 'active' },
        ],
        edges: [
            { source: 'agent-alpha', target: 'ms-hub-1' },
        ],
    }),
}));

vi.mock('../services/socket', () => ({
    tadpole_os_socket: {
        on: vi.fn(),
        off: vi.fn(),
        subscribe_swarm_pulse: vi.fn(() => () => {}),
    },
}));

// Mock i18n
vi.mock('../i18n', () => ({
    i18n: {
        t: (key: string, options?: any) => {
            if (key === 'swarm_visualizer.title') return 'SWARM INTELLIGENCE';
            if (key === 'swarm_visualizer.nodes_online') return `${options?.count || 0} NODES ONLINE`;
            if (key === 'swarm_visualizer.recenter_swarm') return 'RECENTER';
            return key;
        },
    },
}));

describe('Swarm_Visualizer with Actionable Quick Command Bar', () => {
    beforeEach(() => {
        vi.clearAllMocks();
    });

    it('renders the visualizer canvas and bootstraps REST topology', async () => {
        render(<Swarm_Visualizer />);

        expect(screen.getByTestId('force-graph-2d')).toBeInTheDocument();
        expect(screen.getByText('SWARM INTELLIGENCE')).toBeInTheDocument();

        await waitFor(() => {
            expect(screen.getByTestId('graph-node-agent-alpha')).toBeInTheDocument();
            expect(screen.getByTestId('graph-node-ms-hub-1')).toBeInTheDocument();
        });
    });

    it('opens Quick Command Bar when an agent node is clicked', async () => {
        render(<Swarm_Visualizer />);

        await waitFor(() => {
            expect(screen.getByTestId('graph-node-agent-alpha')).toBeInTheDocument();
        });

        // Click agent node
        fireEvent.click(screen.getByTestId('graph-node-agent-alpha'));

        // Verify sovereign store was focused
        expect(mock_set_selected_agent_id).toHaveBeenCalledWith('agent-alpha');
        expect(mock_set_scope).toHaveBeenCalledWith('agent');

        // Verify Quick Command Bar is mounted
        expect(screen.getByTestId('swarm-quick-command-bar')).toBeInTheDocument();
        expect(screen.getByTestId('quick-directive-input')).toBeInTheDocument();
        expect(screen.getByTestId('submit-quick-directive')).toBeInTheDocument();
    });

    it('dispatches directive to agent when form is submitted', async () => {
        render(<Swarm_Visualizer />);

        await waitFor(() => {
            expect(screen.getByTestId('graph-node-agent-alpha')).toBeInTheDocument();
        });

        fireEvent.click(screen.getByTestId('graph-node-agent-alpha'));

        const input = screen.getByTestId('quick-directive-input');
        const submit_btn = screen.getByTestId('submit-quick-directive');

        fireEvent.change(input, { target: { value: 'Run telemetry audit' } });
        expect(submit_btn).not.toBeDisabled();

        fireEvent.click(submit_btn);

        await waitFor(() => {
            expect(mock_send_command).toHaveBeenCalledWith(
                'agent-alpha',
                'Run telemetry audit',
                'gemini-1.5-flash',
                'google',
                undefined,
                'Core'
            );
        });

        // Feedback toast shown
        await waitFor(() => {
            expect(screen.getByText(/Directive dispatched to Alpha Agent/i)).toBeInTheDocument();
        });
    });

    it('displays mission inspection details when a mission hub node is clicked', async () => {
        render(<Swarm_Visualizer />);

        await waitFor(() => {
            expect(screen.getByTestId('graph-node-ms-hub-1')).toBeInTheDocument();
        });

        // Click mission hub
        fireEvent.click(screen.getByTestId('graph-node-ms-hub-1'));

        expect(mock_set_scope).toHaveBeenCalledWith('cluster');
        expect(screen.getByTestId('swarm-quick-command-bar')).toBeInTheDocument();
        expect(screen.getByText(/MISSION HUB/i)).toBeInTheDocument();
        expect(screen.getByText('Inspect Mission')).toBeInTheDocument();
    });

    it('dismisses the Quick Command Bar when close button is clicked', async () => {
        render(<Swarm_Visualizer />);

        await waitFor(() => {
            expect(screen.getByTestId('graph-node-agent-alpha')).toBeInTheDocument();
        });

        fireEvent.click(screen.getByTestId('graph-node-agent-alpha'));
        expect(screen.getByTestId('swarm-quick-command-bar')).toBeInTheDocument();

        fireEvent.click(screen.getByTestId('close-quick-command'));
        expect(screen.queryByTestId('swarm-quick-command-bar')).not.toBeInTheDocument();
    });
});

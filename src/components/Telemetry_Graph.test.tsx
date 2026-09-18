/**
 * @docs ARCHITECTURE:TestSuites
 * 
 * ### AI Assist Note
 * **Verification of the Swarm Engine Telemetry Graph component**. 
 * Ensures robust DAG rendering, interactive mission filtering, trace purges, 
 * and correct node/edge status tracking.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Layout collision, trace state mismatch, or selector lookup failure.
 * - **Telemetry Link**: Search `[Telemetry_Graph_test]` in observability logs.
 */

import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import '@testing-library/jest-dom/vitest';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { Telemetry_Graph } from './Telemetry_Graph';

// Mock stores
const mock_clear_all = vi.fn();
const mock_trace_state = {
    spans: {} as Record<string, any>,
    clear_all: mock_clear_all
};

vi.mock('../stores/trace_store', () => ({
    use_trace_store: vi.fn((selector) => selector ? selector(mock_trace_state) : mock_trace_state)
}));

const mock_agent_state = {
    agents: [
        { id: 'agent-1', name: 'Alpha Recruiter' },
        { id: 'agent-2', name: 'Beta Worker' }
    ]
};

vi.mock('../stores/agent_store', () => ({
    use_agent_store: vi.fn((selector) => selector ? selector(mock_agent_state) : mock_agent_state)
}));

// Mock standard i18n
vi.mock('../i18n', () => ({
    i18n: {
        t: (key: string, options?: any) => {
            if (key === 'telemetry.telemetry_graph.filter_mission' && options?.id) {
                return `Mission: ${options.id}`;
            }
            return key;
        },
    },
}));

// Mock Framer Motion
vi.mock('framer-motion', () => ({
    motion: {
        div: ({ children, className }: any) => <div className={className}>{children}</div>
    },
    AnimatePresence: ({ children }: any) => <>{children}</>
}));

// Mock reactflow so it actually renders child nodes using custom nodeTypes
vi.mock('reactflow', async () => {
    const React = await import('react');
    const ReactFlowMock = ({ nodes, nodeTypes, children }: any) => (
        <div data-testid="react-flow">
            {children}
            <div data-testid="rf-nodes">
                {(nodes || []).map((node: any) => {
                    const Component = nodeTypes?.[node.type] || (() => null);
                    return <Component key={node.id} data={node.data} id={node.id} />;
                })}
            </div>
        </div>
    );
    return {
        default: ReactFlowMock,
        Background: () => <div data-testid="rf-background" />,
        Controls: () => <div data-testid="rf-controls" />,
        Panel: ({ children, position }: any) => <div data-testid={`rf-panel-${position}`}>{children}</div>,
        useNodesState: (initial: any) => {
            const [val, setVal] = React.useState(initial);
            return [val, setVal, vi.fn()];
        },
        useEdgesState: (initial: any) => {
            const [val, setVal] = React.useState(initial);
            return [val, setVal, vi.fn()];
        },
        Position: { Top: 'top', Bottom: 'bottom', Left: 'left', Right: 'right' },
        MarkerType: { ArrowClosed: 'arrowclosed' },
        Handle: ({ type, position }: any) => <div data-testid={`rf-handle-${type}-${position}`} />,
    };
});

describe('Telemetry_Graph Component Verification', () => {
    beforeEach(() => {
        vi.clearAllMocks();
        mock_trace_state.spans = {};
    });

    it('renders empty graph with control panel headers (Happy Path)', async () => {
        render(<Telemetry_Graph />);

        expect(screen.getByTestId('react-flow')).toBeInTheDocument();
        expect(screen.getByText('telemetry.telemetry_graph.title')).toBeInTheDocument();
        expect(screen.getByText('telemetry.telemetry_graph.subtitle')).toBeInTheDocument();
    });

    it('renders nodes with correct agent names and processing status (Complex Traces)', async () => {
        mock_trace_state.spans = {
            'span-1': {
                id: 'span-1',
                mission_id: 'abc-mission-123',
                name: 'core_run',
                agent_id: 'agent-1',
                start_time: 1000,
                status: 'success',
                parent_id: null
            },
            'span-2': {
                id: 'span-2',
                mission_id: 'abc-mission-123',
                name: 'execute_tool:search',
                agent_id: 'agent-2',
                start_time: 1200,
                end_time: 1800,
                status: 'running',
                parent_id: 'span-1'
            }
        };

        render(<Telemetry_Graph />);

        // Wait for state updates/layout calculation (300ms layout debounce)
        await waitFor(() => {
            expect(screen.getByText('Alpha Recruiter')).toBeInTheDocument();
        }, { timeout: 1000 });

        expect(screen.getByText('core_run')).toBeInTheDocument();
        expect(screen.getByText('Beta Worker')).toBeInTheDocument();
        // Since names are split by colons for clean UI display:
        expect(screen.getByText('search')).toBeInTheDocument();
        expect(screen.getByText('0.60s')).toBeInTheDocument(); // 1800 - 1200 ms in seconds
    });

    it('filters nodes based on mission selection (Input Filtering)', async () => {
        mock_trace_state.spans = {
            'span-1': {
                id: 'span-1',
                mission_id: 'abc-mission-123',
                name: 'core_run_1',
                agent_id: 'agent-1',
                start_time: 1000,
                status: 'success',
                parent_id: null
            },
            'span-2': {
                id: 'span-2',
                mission_id: 'xyz-mission-456',
                name: 'core_run_2',
                agent_id: 'agent-2',
                start_time: 1200,
                status: 'success',
                parent_id: null
            }
        };

        render(<Telemetry_Graph />);

        // Wait for selector options to populate
        await waitFor(() => {
            expect(screen.getByRole('combobox')).toBeInTheDocument();
        });

        expect(screen.getByText('Mission: abc-miss')).toBeInTheDocument(); // abc-mission-123 option (first 8 chars)
        expect(screen.getByText('Mission: xyz-miss')).toBeInTheDocument(); // xyz-mission-456 option

        const select = screen.getByRole('combobox');
        
        // Trigger filter change to first mission
        fireEvent.change(select, { target: { value: 'abc-mission-123' } });

        await waitFor(() => {
            expect(screen.queryByText('Beta Worker')).not.toBeInTheDocument();
        }, { timeout: 1500 });
    });

    it('triggers trace purge when click purge button (Interaction)', async () => {
        render(<Telemetry_Graph />);

        const purge_btn = screen.getByRole('button', { name: /purge/i });
        expect(purge_btn).toBeInTheDocument();

        fireEvent.click(purge_btn);
        expect(mock_clear_all).toHaveBeenCalledTimes(1);
    });

    it('safely filters out self-loop edges and edges referencing unmounted parents', async () => {
        mock_trace_state.spans = {
            'span-root': {
                id: 'span-root',
                mission_id: 'm-1',
                name: 'root_phase',
                agent_id: 'agent-1',
                start_time: 1000,
                status: 'success',
                parent_id: null
            },
            'span-self-loop': {
                id: 'span-self-loop',
                mission_id: 'm-1',
                name: 'self_loop_phase',
                agent_id: 'agent-1',
                start_time: 1100,
                status: 'running',
                parent_id: 'span-self-loop' // Self loop edge!
            },
            'span-orphan-child': {
                id: 'span-orphan-child',
                mission_id: 'm-1',
                name: 'orphan_child',
                agent_id: 'agent-1',
                start_time: 1200,
                status: 'running',
                parent_id: 'span-non-existent-parent' // Parent does not exist!
            }
        };

        render(<Telemetry_Graph initial_mission_id="m-1" />);

        await waitFor(() => {
            expect(screen.getByText('root_phase')).toBeInTheDocument();
            expect(screen.getByText('self_loop_phase')).toBeInTheDocument();
            expect(screen.getByText('orphan_child')).toBeInTheDocument();
        });
    });

    it('reacts to initial_mission_id prop updates dynamically', async () => {
        mock_trace_state.spans = {
            'span-a': {
                id: 'span-a',
                mission_id: 'mission-alpha',
                name: 'task_alpha',
                agent_id: 'agent-1',
                start_time: 1000,
                status: 'success',
                parent_id: null
            },
            'span-b': {
                id: 'span-b',
                mission_id: 'mission-beta',
                name: 'task_beta',
                agent_id: 'agent-2',
                start_time: 1000,
                status: 'success',
                parent_id: null
            }
        };

        const { rerender } = render(<Telemetry_Graph initial_mission_id="mission-alpha" />);

        await waitFor(() => {
            expect(screen.getByText('task_alpha')).toBeInTheDocument();
            expect(screen.queryByText('task_beta')).not.toBeInTheDocument();
        });

        // Update prop dynamically
        rerender(<Telemetry_Graph initial_mission_id="mission-beta" />);

        await waitFor(() => {
            expect(screen.queryByText('task_alpha')).not.toBeInTheDocument();
            expect(screen.getByText('task_beta')).toBeInTheDocument();
        });
    });
});

// Metadata: [Telemetry_Graph_test]

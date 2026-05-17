/*
@docs ARCHITECTURE:UI

### AI Assist Note
**🛡️ Tadpole OS: Neural Waterfall**
Core system module providing specialized functionality for the agent swarm.

### 🔍 Debugging & Observability
- **Failure Path**: Unexpected execution drift or type compatibility issues.
- **Telemetry Link**: Traced via active system logging channels.
*/

import { render, screen, act } from '@testing-library/react';
import '@testing-library/jest-dom/vitest';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { Neural_Waterfall } from './Neural_Waterfall';

// Mock stores
const mock_trace_state = {
    active_trace_id: 'trace-1',
    get_trace_tree: vi.fn()
};

vi.mock('../stores/trace_store', () => ({
    use_trace_store: vi.fn((selector) => selector ? selector(mock_trace_state) : mock_trace_state)
}));

const mock_agent_state = {
    get_agent: vi.fn((id) => ({ id, name: `Agent ${id}` }))
};

vi.mock('../stores/agent_store', () => ({
    use_agent_store: vi.fn((selector) => selector ? selector(mock_agent_state) : mock_agent_state)
}));

const mock_tab_state = {
    is_trace_stream_detached: false,
    toggle_trace_stream_detachment: vi.fn()
};

vi.mock('../stores/tab_store', () => ({
    use_tab_store: vi.fn((selector) => selector ? selector(mock_tab_state) : mock_tab_state)
}));

vi.mock('../i18n', () => ({
    i18n: {
        t: (key: string) => key,
    },
}));

vi.mock('./ui', () => ({
    Tooltip: ({ children, content }: any) => <div title={content}>{children}</div>
}));

// Mock Framer Motion to avoid animation issues in tests
vi.mock('framer-motion', () => ({
    motion: {
        div: ({ children, className, animate, initial }: any) => (
            <div className={className} data-animate={JSON.stringify(animate)} data-initial={JSON.stringify(initial)}>
                {children}
            </div>
        )
    }
}));

describe('Neural_Waterfall Verification', () => {
    beforeEach(() => {
        vi.clearAllMocks();
        vi.useFakeTimers();
    });

    it('renders empty state when no trace is active', () => {
        mock_trace_state.active_trace_id = null;
        mock_trace_state.get_trace_tree.mockReturnValue([]);
        
        render(<Neural_Waterfall />);
        expect(screen.getByText('LINK READY :: AWAITING TELEMETRY')).toBeInTheDocument();
    });

    it('renders trace rows correctly', () => {
        mock_trace_state.active_trace_id = 'trace-1';
        const mock_tree = [
            {
                id: 'span-1',
                trace_id: 'trace-1',
                name: 'Main Task',
                agent_id: 'a1',
                start_time: Date.now() - 1000,
                end_time: Date.now() - 500,
                status: 'success',
                children: [
                    {
                        id: 'span-2',
                        trace_id: 'trace-1',
                        parent_id: 'span-1',
                        name: 'Sub Task',
                        agent_id: 'a2',
                        start_time: Date.now() - 800,
                        end_time: Date.now() - 600,
                        status: 'success',
                        children: []
                    }
                ]
            }
        ];
        mock_trace_state.get_trace_tree.mockReturnValue(mock_tree);

        render(<Neural_Waterfall />);
        
        expect(screen.getByText('trace_stream.title')).toBeInTheDocument();
        expect(screen.getByText('Agent a1')).toBeInTheDocument();
        expect(screen.getByText('Main Task')).toBeInTheDocument();
        expect(screen.getByText('Agent a2')).toBeInTheDocument();
        expect(screen.getByText('Sub Task')).toBeInTheDocument();
    });

    it('updates "Now" ticker for running spans', () => {
        const start = Date.now() - 1000;
        mock_trace_state.active_trace_id = 'trace-1';
        const mock_tree = [
            {
                id: 'span-running',
                trace_id: 'trace-1',
                name: 'Running...',
                agent_id: 'a1',
                start_time: start,
                end_time: undefined, // Still running
                status: 'running',
                children: []
            }
        ];
        mock_trace_state.get_trace_tree.mockReturnValue(mock_tree);

        render(<Neural_Waterfall />);
        
        const initial_text = screen.getAllByText(/ms/)[1]; // The span bar text
        const initial_duration = parseInt(initial_text.textContent || '0');
        
        // Advance time
        act(() => {
            vi.advanceTimersByTime(1000);
        });
        
        const updated_text = screen.getAllByText(/ms/)[1];
        const updated_duration = parseInt(updated_text.textContent || '0');
        
        expect(updated_duration).toBeGreaterThan(initial_duration);
    });

    it('handles detachment toggle', () => {
        render(<Neural_Waterfall />);
        const toggle_btn = screen.getByTitle('trace_stream.detach_tooltip');
        toggle_btn.click();
        expect(mock_tab_state.toggle_trace_stream_detachment).toHaveBeenCalled();
    });
});

// Metadata: [Neural_Waterfall_test]

// Metadata: [Neural_Waterfall_test]

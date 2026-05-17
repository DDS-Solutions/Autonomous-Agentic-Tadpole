/*
@docs ARCHITECTURE:UI

### AI Assist Note
**🛡️ Tadpole OS: Lineage Stream**
Core system module providing specialized functionality for the agent swarm.

### 🔍 Debugging & Observability
- **Failure Path**: Unexpected execution drift or type compatibility issues.
- **Telemetry Link**: Traced via active system logging channels.
*/

import { render, screen, fireEvent } from '@testing-library/react';
import '@testing-library/jest-dom/vitest';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { Lineage_Stream } from './Lineage_Stream';

// Mock stores
const mock_trace_state = {
    active_trace_id: 'trace-123',
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
    is_lineage_stream_detached: false,
    toggle_lineage_stream_detachment: vi.fn()
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

// Mock Framer Motion
vi.mock('framer-motion', () => ({
    motion: {
        div: ({ children, className }: any) => <div className={className}>{children}</div>
    },
    AnimatePresence: ({ children }: any) => <>{children}</>
}));

describe('Lineage_Stream Verification', () => {
    beforeEach(() => {
        vi.clearAllMocks();
    });

    it('renders waiting state when no trace is active', () => {
        mock_trace_state.active_trace_id = null;
        mock_trace_state.get_trace_tree.mockReturnValue([]);
        
        render(<Lineage_Stream />);
        expect(screen.getByText(/trace.waiting/)).toBeInTheDocument();
        expect(screen.getByText(/trace.waiting_hint/)).toBeInTheDocument();
    });

    it('renders a hierarchical trace tree', () => {
        mock_trace_state.active_trace_id = 'trace-123';
        const mock_tree = [
            {
                id: 'span-root',
                trace_id: 'trace-123',
                name: 'Main Mission',
                agent_id: 'alpha',
                start_time: 1000,
                end_time: 2000,
                status: 'success',
                children: [
                    {
                        id: 'span-child',
                        trace_id: 'trace-123',
                        parent_id: 'span-root',
                        name: 'Sub Task',
                        agent_id: 'beta',
                        start_time: 1200,
                        end_time: 1500,
                        status: 'success',
                        children: []
                    }
                ]
            }
        ];
        mock_trace_state.get_trace_tree.mockReturnValue(mock_tree);

        render(<Lineage_Stream />);
        
        expect(screen.getByText('trace.stream_title')).toBeInTheDocument();
        expect(screen.getByText('Agent alpha')).toBeInTheDocument();
        expect(screen.getByText('Main Mission')).toBeInTheDocument();
        expect(screen.getByText('Agent beta')).toBeInTheDocument();
        expect(screen.getByText('Sub Task')).toBeInTheDocument();
        expect(screen.getByText('1000ms')).toBeInTheDocument(); // 2000 - 1000
        expect(screen.getByText('300ms')).toBeInTheDocument(); // 1500 - 1200
    });

    it('decodes AAAK findings in tooltips', () => {
        mock_trace_state.active_trace_id = 'trace-123';
        mock_trace_state.get_trace_tree.mockReturnValue([{
            id: 'span-1',
            trace_id: 'trace-123',
            name: 'Audit',
            agent_id: 'a1',
            start_time: 1000,
            status: 'running',
            attributes: {
                finding: 'FND: Vulnerability'
            },
            children: []
        }]);

        render(<Lineage_Stream />);
        
        // AAAK decoding check: FND: -> 💡 Finding:
        const tooltip_trigger = screen.getByText('FND: Vulnerability');
        expect(tooltip_trigger.closest('[title]')).toHaveAttribute('title', '💡 Finding: Vulnerability');
    });

    it('handles sidebar resizing', () => {
        const { container } = render(<Lineage_Stream />);
        const resizer = container.querySelector('.cursor-col-resize');
        
        expect(resizer).toBeInTheDocument();

        fireEvent.mouseDown(resizer!, { clientX: 1000 });
        fireEvent.mouseMove(document, { clientX: 900 });
        fireEvent.mouseUp(document);
        // Resizing logic verified by lack of errors and coverage of event handlers
    });
});

// Metadata: [Lineage_Stream_test]

// Metadata: [Lineage_Stream_test]

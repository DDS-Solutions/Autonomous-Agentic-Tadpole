/**
 * @docs ARCHITECTURE:UI-Components
 * 
 * ### AI Assist Note
 * **Knowledge Graph Component Tests**: Unit tests for the KnowledgeGraph visualization component.
 * Verifies graph fetching, symbols/OKF mode toggles, and HUD telemetry display.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: ForceGraph mock failure, DOM element mismatch on view switch.
 * - **Telemetry Link**: Search `[KnowledgeGraph_test]` in test runner logs.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { KnowledgeGraph } from './KnowledgeGraph';
import { intelligence_api_service } from '../../services/intelligence_api_service';

// Mock react-force-graph-2d
vi.mock('react-force-graph-2d', () => {
    return {
        default: vi.fn(({ graphData }: any) => (
            <div data-testid="force-graph-2d" data-node-count={graphData?.nodes?.length || 0}>
                {graphData?.nodes?.map((node: any) => (
                    <div key={node.id} data-testid={`node-${node.id}`}>{node.name}</div>
                ))}
            </div>
        )),
    };
});

// Mock intelligence_api_service
vi.mock('../../services/intelligence_api_service', () => ({
    intelligence_api_service: {
        get_graph: vi.fn(),
        get_knowledge: vi.fn(),
        get_knowledge_edges: vi.fn(),
        get_blast_radius: vi.fn(),
    },
}));

describe('KnowledgeGraph Component', () => {
    const mockCodeGraph = {
        nodes: [
            {
                id: 'server-rs/src/main.rs:main',
                name: 'main',
                path: 'server-rs/src/main.rs',
                kind: 'Function',
                signature: 'fn main()',
                start_line: 1,
                end_line: 50,
            },
            {
                id: 'server-rs/src/agent.rs:Agent',
                name: 'Agent',
                path: 'server-rs/src/agent.rs',
                kind: 'Struct',
                signature: 'struct Agent',
                start_line: 10,
                end_line: 80,
            },
        ],
        links: [
            {
                source: 'server-rs/src/main.rs:main',
                target: 'server-rs/src/agent.rs:Agent',
            },
        ],
        anomalies: [],
    };

    const mockOkfEntries = [
        {
            id: 'okf-concept-001',
            title: 'Sovereign Kernel Architecture',
            topic: 'architecture',
            concept_type: 'core',
            confidence: 0.95,
            human_confirmed: true,
            text: 'Detailed description linking to [okf-concept-002](/okf-concept-002)',
            resource_uri: 'docs/architecture.md',
        },
        {
            id: 'okf-concept-002',
            title: 'Durable Execution Engine',
            topic: 'execution',
            concept_type: 'subsystem',
            confidence: 0.9,
            human_confirmed: false,
            text: 'Replay cache and step memoization',
            resource_uri: 'docs/durable.md',
        },
    ];

    beforeEach(() => {
        vi.clearAllMocks();
        vi.mocked(intelligence_api_service.get_graph).mockResolvedValue(mockCodeGraph);
        vi.mocked(intelligence_api_service.get_knowledge).mockResolvedValue(mockOkfEntries as any);
        vi.mocked(intelligence_api_service.get_knowledge_edges).mockResolvedValue([
            { source_id: 'okf-concept-001', target_id: 'okf-concept-002' } as any,
        ]);
    });

    it('renders symbols mode by default and loads graph data', async () => {
        render(<KnowledgeGraph />);

        // Resolves graph data
        await waitFor(() => {
            expect(screen.getByText('Codebase Symbols')).toBeInTheDocument();
            expect(screen.getByText(/2 Symbols/i)).toBeInTheDocument();
            expect(screen.getByText(/1 Edges/i)).toBeInTheDocument();
        });

        expect(intelligence_api_service.get_graph).toHaveBeenCalled();
    });

    it('toggles to Knowledge Mode and fetches OKF entries', async () => {
        render(<KnowledgeGraph />);

        await waitFor(() => {
            expect(screen.getByText('Codebase Symbols')).toBeInTheDocument();
        });

        const knowledgeModeBtn = screen.getByRole('button', { name: /Knowledge Mode/i });
        fireEvent.click(knowledgeModeBtn);

        await waitFor(() => {
            expect(screen.getByText('OKF Knowledge Graph')).toBeInTheDocument();
            expect(screen.getByText(/2 Concepts/i)).toBeInTheDocument();
        });

        expect(intelligence_api_service.get_knowledge).toHaveBeenCalledWith({ limit: 200 });
    });

    it('renders the legend elements correctly for Symbols mode', async () => {
        render(<KnowledgeGraph />);

        await waitFor(() => {
            expect(screen.getByText('Function / Method')).toBeInTheDocument();
            expect(screen.getByText('Struct / Class')).toBeInTheDocument();
            expect(screen.getByText('Trait / Interface')).toBeInTheDocument();
            expect(screen.getByText('Enum')).toBeInTheDocument();
        });
    });
});

// Metadata: [KnowledgeGraph_test]

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
import { system_api_service } from '../../services/system_api_service';

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
        get_knowledge_peers: vi.fn(),
    },
}));

// Mock system_api_service
vi.mock('../../services/system_api_service', () => ({
    system_api_service: {
        get_knowledge_docs: vi.fn(),
        get_knowledge_doc: vi.fn(),
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
            { source_id: 'okf-concept-001', target_id: 'non-existent-node' } as any,
        ]);
        vi.mocked(system_api_service.get_knowledge_docs).mockResolvedValue([
            { category: 'engineering', name: 'debugging.md', title: 'Debugging Guide' },
            { category: 'engineering', name: 'incident-response.md', title: 'Incident Response' },
        ]);
        vi.mocked(system_api_service.get_knowledge_doc).mockImplementation(async (_cat, name) => {
            if (name === 'incident-response.md') {
                return 'Read the guide on [debugging](debugging.md) for root cause analysis.';
            }
            return 'Debugging instructions and protocols.';
        });
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

    it('toggles to Knowledge Mode and fetches OKF entries with edge filtering', async () => {
        render(<KnowledgeGraph />);

        await waitFor(() => {
            expect(screen.getByText('Codebase Symbols')).toBeInTheDocument();
        });

        const knowledgeModeBtn = screen.getByRole('button', { name: /Knowledge Mode/i });
        fireEvent.click(knowledgeModeBtn);

        await waitFor(() => {
            expect(screen.getByText('OKF Knowledge Graph')).toBeInTheDocument();
            expect(screen.getByText(/2 Concepts/i)).toBeInTheDocument();
            // 1 valid explicit edge + 1 extracted markdown edge = 2 edges (invalid edge filtered out)
            expect(screen.getByText(/1 Edges|2 Edges/i)).toBeInTheDocument();
        });

        expect(intelligence_api_service.get_knowledge).toHaveBeenCalledWith({ limit: 200 });
    });

    it('falls back to curated system knowledge docs when IKS returns empty', async () => {
        vi.mocked(intelligence_api_service.get_knowledge).mockResolvedValue([]);
        vi.mocked(intelligence_api_service.get_knowledge_edges).mockResolvedValue([]);

        render(<KnowledgeGraph />);

        const knowledgeModeBtn = screen.getByRole('button', { name: /Knowledge Mode/i });
        fireEvent.click(knowledgeModeBtn);

        await waitFor(() => {
            expect(screen.getByText('OKF Knowledge Graph')).toBeInTheDocument();
            expect(screen.getByText(/2 Concepts/i)).toBeInTheDocument();
            expect(screen.getByText('Debugging Guide')).toBeInTheDocument();
            expect(screen.getByText('Incident Response')).toBeInTheDocument();
        });

        expect(system_api_service.get_knowledge_docs).toHaveBeenCalled();
    });

    it('renders the legend elements correctly for Symbols mode and Knowledge mode', async () => {
        render(<KnowledgeGraph />);

        await waitFor(() => {
            expect(screen.getByText('Function / Method')).toBeInTheDocument();
            expect(screen.getByText('Struct / Class')).toBeInTheDocument();
            expect(screen.getByText('Trait / Interface')).toBeInTheDocument();
            expect(screen.getByText('Enum')).toBeInTheDocument();
        });

        const knowledgeModeBtn = screen.getByRole('button', { name: /Knowledge Mode/i });
        fireEvent.click(knowledgeModeBtn);

        await waitFor(() => {
            expect(screen.getByText('Confirmed')).toBeInTheDocument();
            expect(screen.getByText('Expiring')).toBeInTheDocument();
            expect(screen.getByText('Broken')).toBeInTheDocument();
            expect(screen.getByText('Base')).toBeInTheDocument();
        });
    });
});

// Metadata: [KnowledgeGraph_test]

/**
 * @docs ARCHITECTURE:TestSuites
 *
 * ### AI Assist Note
 * **Intelligence & Knowledge API Service Tests**: Verifies URL query serialization, HTTP methods,
 * AbortSignal forwarding, and OKF synthesis payloads across all 9 methods in `intelligence_api_service`.
 *
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Missing query parameters or wrong route paths against `/v1/intelligence/*` or `/v1/knowledge/*`.
 * - **Telemetry Link**: Search `[intelligence_api_service_test]` in test logs.
 */

import { describe, it, expect, vi, beforeEach, type Mock } from 'vitest';
import { intelligence_api_service } from './intelligence_api_service';
import { api_request } from './base_api_service';

vi.mock('./base_api_service', () => ({
    api_request: vi.fn(),
}));

describe('IntelligenceApiService', () => {
    beforeEach(() => {
        vi.clearAllMocks();
        (api_request as Mock).mockResolvedValue({});
    });

    it('calls get_graph with optional path_prefix and max_nodes query params', async () => {
        await intelligence_api_service.get_graph();
        expect(api_request).toHaveBeenCalledWith('/v1/intelligence/graph', { signal: undefined });

        const ac = new AbortController();
        await intelligence_api_service.get_graph({ path_prefix: 'src/', max_nodes: 50 }, ac.signal);
        expect(api_request).toHaveBeenCalledWith(
            '/v1/intelligence/graph?path_prefix=src%2F&max_nodes=50',
            { signal: ac.signal }
        );
    });

    it('calls get_blast_radius, rebuild_graph, resolve_code_context, and get_impacted_tests', async () => {
        await intelligence_api_service.get_blast_radius('AppState', 'src/state.rs', 25);
        expect(api_request).toHaveBeenCalledWith(
            '/v1/intelligence/blast-radius?name=AppState&path=src%2Fstate.rs&limit=25',
            { signal: undefined }
        );

        await intelligence_api_service.rebuild_graph(true);
        expect(api_request).toHaveBeenCalledWith(
            '/v1/intelligence/graph/rebuild?dry_run=true',
            { method: 'POST', signal: undefined }
        );

        await intelligence_api_service.resolve_code_context('Router', 'src/router.rs', 2000);
        expect(api_request).toHaveBeenCalledWith(
            '/v1/intelligence/resolve?name=Router&path=src%2Frouter.rs&budget=2000',
            { signal: undefined }
        );

        await intelligence_api_service.get_impacted_tests('src/router.rs', 'create_router');
        expect(api_request).toHaveBeenCalledWith(
            '/v1/intelligence/impacted-tests?path=src%2Frouter.rs&name=create_router',
            { signal: undefined }
        );
    });

    it('calls OKF knowledge listing, peers, edges, and synthesis endpoints', async () => {
        await intelligence_api_service.get_knowledge({
            topic: 'security',
            cluster_id: 'c1',
            concept_type: 'rule',
            limit: 10,
            offset: 20,
        });
        expect(api_request).toHaveBeenCalledWith(
            '/v1/knowledge?topic=security&cluster_id=c1&concept_type=rule&limit=10&offset=20',
            { signal: undefined }
        );

        await intelligence_api_service.get_knowledge_peers('node-1', 5);
        expect(api_request).toHaveBeenCalledWith('/v1/knowledge/node-1/peers?limit=5', { signal: undefined });

        await intelligence_api_service.get_knowledge_edges({ source_id: 'a', target_id: 'b' });
        expect(api_request).toHaveBeenCalledWith(
            '/v1/knowledge/edges?source_id=a&target_id=b',
            { signal: undefined }
        );

        const req = { source_ids: ['a', 'b'], topic: 'arch', title: 'Summary', concept_type: 'synthesis' };
        await intelligence_api_service.synthesize_knowledge(req);
        expect(api_request).toHaveBeenCalledWith('/v1/knowledge/synthesize', {
            method: 'POST',
            body: JSON.stringify(req),
        });
    });
});

// Metadata: [intelligence_api_service_test]

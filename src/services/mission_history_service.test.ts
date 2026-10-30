/**
 * @docs ARCHITECTURE:TestSuites
 *
 * ### AI Assist Note
 * **mission_history_service.test**: Unit tests for the Sovereign Observability Core (SOC)
 * Mission Flight Recorder frontend service layer (`mission_history_service`).
 *
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Endpoint query string mismatch or Markdown export regression.
 * - **Telemetry Link**: Search `[mission_history_service_test]` in console logs.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mission_history_service } from './mission_history_service';
import { api_request } from './base_api_service';
import type { MissionDossier } from '../types/mission_dossier';

vi.mock('./base_api_service', () => ({
    api_request: vi.fn(),
}));

describe('mission_history_service', () => {
    beforeEach(() => {
        vi.clearAllMocks();
    });

    it('fetches paginated catalog with query parameters', async () => {
        (api_request as unknown as ReturnType<typeof vi.fn>).mockResolvedValue({
            success: true,
            data: [],
            total: 0,
            page: 2,
            per_page: 25,
        });

        await mission_history_service.fetch_catalog({
            page: 2,
            per_page: 25,
            status: 'completed',
            search: 'overlord',
            pinned_only: true,
        });

        expect(api_request).toHaveBeenCalledWith(
            '/v1/missions?page=2&per_page=25&status=completed&search=overlord&pinned_only=true',
            { method: 'GET' }
        );
    });

    it('fetches composite dossier by encoded mission ID', async () => {
        (api_request as unknown as ReturnType<typeof vi.fn>).mockResolvedValue({
            mission: { id: 'ms-101' },
        });

        await mission_history_service.fetch_dossier(' ms-101 ');

        expect(api_request).toHaveBeenCalledWith('/v1/missions/ms-101/dossier', {
            method: 'GET',
        });
    });

    it('toggles retention pin status via POST /v1/missions/{id}/pin', async () => {
        (api_request as unknown as ReturnType<typeof vi.fn>).mockResolvedValue({
            success: true,
            mission_id: 'ms-101',
            is_pinned: true,
        });

        const res = await mission_history_service.toggle_pin('ms-101', true);

        expect(api_request).toHaveBeenCalledWith('/v1/missions/ms-101/pin', {
            method: 'POST',
            body: JSON.stringify({ is_pinned: true }),
        });
        expect(res.is_pinned).toBe(true);
    });

    it('fetches read-only CAS revision history via GET /v1/cas/history', async () => {
        (api_request as unknown as ReturnType<typeof vi.fn>).mockResolvedValue({
            success: true,
            data: [
                {
                    id: 1,
                    workspace_root: '/ws',
                    file_path: 'src/main.rs',
                    version_num: 1,
                    hash: 'abc123',
                    size_bytes: 120,
                    mission_id: 'ms-101',
                    agent_id: 'alpha-0',
                    created_at: '2026-10-30T12:00:00Z',
                },
            ],
        });

        const history = await mission_history_service.fetch_file_history('src/main.rs');
        expect(api_request).toHaveBeenCalledWith('/v1/cas/history?file_path=src%2Fmain.rs', {
            method: 'GET',
        });
        expect(history).toHaveLength(1);
        expect(history[0].version_num).toBe(1);
    });

    it('formats a complete ALCOA+ Markdown report with structured step decomposition', () => {
        const mock_dossier: MissionDossier = {
            mission: {
                id: 'ms-overlord-1',
                agent_id: 'alpha-0',
                title: 'Audit Swarm Security',
                status: 'completed',
                budget_usd: 5.0,
                cost_usd: 0.125,
                is_pinned: true,
                created_at: '2026-10-30T12:00:00Z',
                updated_at: '2026-10-30T12:05:00Z',
                log_count: 1,
                diff_count: 1,
            },
            merkle_audit: {
                chain_verified: true,
                total_steps: 1,
                tampered_at_step: null,
                genesis_hash: 'aaa111',
                head_hash: 'aaa111',
            },
            logs: [
                {
                    id: 'step-1',
                    mission_id: 'ms-overlord-1',
                    step_index: 1,
                    agent_id: 'alpha-0',
                    source: 'Agent',
                    text: 'Completed security scan',
                    severity: 'info',
                    timestamp: '2026-10-30T12:01:00Z',
                    metadata: { tool_name: 'security_scan', duration_ms: 42 },
                    hash: 'aaa111',
                    prev_hash: 'GENESIS',
                    is_verified: true,
                    thought_summary: 'Scanning routes for traversal vectors',
                    tool_name: 'security_scan',
                    duration_ms: 42,
                },
            ],
            diffs: [
                {
                    id: 1,
                    file_path: 'src/router.rs',
                    version_num: 1,
                    hash: 'sha256-pre',
                    size_bytes: 512,
                    created_at: '2026-10-30T12:01:00Z',
                    pre_mutation_content: 'fn a() {}',
                    current_content: 'fn a() {}\nfn b() {}',
                    diff_summary: { additions: 1, deletions: 0, changes: 1 },
                },
            ],
            memory_palace: [
                {
                    id: 'find-1',
                    mission_id: 'ms-overlord-1',
                    agent_id: 'alpha-0',
                    topic: 'security',
                    finding: 'Zero traversal vectors found',
                    timestamp: '2026-10-30T12:02:00Z',
                },
            ],
            trust_graph: { entities: [], relations: [] },
            economics: {
                total_cost_usd: 0.125,
                budget_usd: 5.0,
                total_tokens: 1024,
                transactions: [],
            },
            debrief: {
                agent99_notes: 'Flight verified nominal.',
                drift_score: 0.042,
                verdict: 'VERIFIED_SUCCESS',
                recommendations: ['Maintain Merkle audit enforcement.'],
            },
        };

        const md = mission_history_service.format_dossier_markdown(mock_dossier);
        expect(md).toContain('# Sovereign Flight Dossier — Audit Swarm Security');
        expect(md).toContain('`VERIFIED_SUCCESS`');
        expect(md).toContain('`security_scan` (42ms)');
        expect(md).toContain('Scanning routes for traversal vectors');
        expect(md).toContain('`src/router.rs`');
    });
});

// Metadata: [mission_history_service_test]


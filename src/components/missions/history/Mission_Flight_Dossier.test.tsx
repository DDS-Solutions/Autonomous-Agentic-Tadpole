/**
 * @docs ARCHITECTURE:TestSuites
 *
 * ### AI Assist Note
 * **Mission_Flight_Dossier.test**: Component and LCS diff algorithm tests for the
 * Sovereign Observability Core (SOC) Flight Dossier UI.
 *
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Tab switching regression, missing structured step badges, or LCS diff miscalculation.
 * - **Telemetry Link**: Search `[Mission_Flight_Dossier_test]` in console logs.
 */

import { render, screen, fireEvent } from '@testing-library/react';
import '@testing-library/jest-dom/vitest';
import { describe, it, expect, vi } from 'vitest';
import { Mission_Flight_Dossier } from './Mission_Flight_Dossier';
import { compute_aligned_diff } from './CAS_Diff_Inspector';
import { mission_history_service } from '../../../services/mission_history_service';
import type { MissionDossier } from '../../../types/mission_dossier';

vi.mock('../../../services/mission_history_service', () => ({
    mission_history_service: {
        download_dossier_export: vi.fn(),
        fetch_file_history: vi.fn(() => Promise.resolve([])),
    },
}));

const sample_dossier: MissionDossier = {
    mission: {
        id: 'ms-flight-001',
        agent_id: 'alpha-0',
        title: 'Sovereign Flight Verification',
        status: 'completed',
        budget_usd: 10.0,
        cost_usd: 0.45,
        is_pinned: false,
        created_at: '2026-10-30T10:00:00Z',
        updated_at: '2026-10-30T10:05:00Z',
        log_count: 2,
        diff_count: 1,
    },
    merkle_audit: {
        chain_verified: true,
        total_steps: 2,
        tampered_at_step: null,
        genesis_hash: 'h1',
        head_hash: 'h2',
    },
    logs: [
        {
            id: 'log-1',
            mission_id: 'ms-flight-001',
            step_index: 1,
            agent_id: 'alpha-0',
            source: 'Agent',
            text: 'Executed parity_guard verification',
            severity: 'info',
            timestamp: '2026-10-30T10:01:00Z',
            metadata: { tool_name: 'parity_guard', duration_ms: 115 },
            hash: 'a1b2c3d4e5f600001111',
            prev_hash: 'GENESIS',
            is_verified: true,
            thought_summary: 'Validating OpenAPI route parity before deployment',
            tool_name: 'parity_guard',
            duration_ms: 115,
        },
        {
            id: 'log-2',
            mission_id: 'ms-flight-001',
            step_index: 2,
            agent_id: 'alpha-0',
            source: 'System',
            text: 'Mission completed',
            severity: 'success',
            timestamp: '2026-10-30T10:02:00Z',
            metadata: null,
            hash: 'f6e5d4c3b2a100002222',
            prev_hash: 'a1b2c3d4e5f600001111',
            is_verified: true,
        },
    ],
    diffs: [
        {
            id: 1,
            file_path: 'server-rs/src/router.rs',
            version_num: 1,
            hash: '99887766554433221100',
            size_bytes: 2048,
            created_at: '2026-10-30T10:01:30Z',
            pre_mutation_content: 'line1\nline2',
            current_content: 'line1\nline2_modified\nline3',
            diff_summary: { additions: 2, deletions: 1, changes: 3 },
        },
    ],
    memory_palace: [
        {
            id: 'mem-1',
            mission_id: 'ms-flight-001',
            agent_id: 'alpha-0',
            topic: 'architecture',
            finding: 'SOC consolidates file_revisions without cas_blobs duplication',
            timestamp: '2026-10-30T10:01:45Z',
        },
    ],
    trust_graph: {
        entities: [
            {
                id: 'ent-1',
                name: 'alpha-0',
                node_type: 'AGENT',
                description: 'Commander',
                mission_id: 'ms-flight-001',
                created_at: 1730000000,
            },
        ],
        relations: [],
    },
    economics: {
        total_cost_usd: 0.45,
        budget_usd: 10.0,
        total_tokens: 4200,
        transactions: [
            {
                id: 'tx-1',
                transaction_type: '2PC_COMMIT',
                buyer_address: 'alpha-0',
                seller_address: 'worker-1',
                asset_id: 'lock-1',
                amount: 500,
                status: 'COMMITTED',
                audit_signature: 'lock-1',
                created_at: 1730000000,
            },
        ],
    },
    debrief: {
        agent99_notes: 'All invariants satisfied.',
        drift_score: 0.05,
        verdict: 'VERIFIED_SUCCESS',
        recommendations: ['Pin mission for long-term compliance reference.'],
    },
};

describe('Mission_Flight_Dossier & CAS_Diff_Inspector', () => {
    it('renders KPI rollup, structured step decomposition badges, and switches across all 5 tabs', () => {
        const on_pin = vi.fn();
        render(<Mission_Flight_Dossier dossier={sample_dossier} on_toggle_pin={on_pin} />);

        // Header & structured step decomposition in Merkle tab
        expect(screen.getByText('Sovereign Flight Verification')).toBeInTheDocument();
        expect(screen.getByText('parity_guard')).toBeInTheDocument();
        expect(screen.getByText('115ms')).toBeInTheDocument();
        expect(
            screen.getByText('Validating OpenAPI route parity before deployment')
        ).toBeInTheDocument();

        // Switch to CAS File Diffs tab
        fireEvent.click(screen.getByRole('tab', { name: /CAS File Diffs/i }));
        expect(screen.getAllByText('server-rs/src/router.rs').length).toBeGreaterThan(0);

        // Switch to Memory Palace tab
        fireEvent.click(screen.getByRole('tab', { name: /Memory Palace/i }));
        expect(
            screen.getByText(/SOC consolidates file_revisions without cas_blobs duplication/i)
        ).toBeInTheDocument();

        // Switch to TrustGraph & Ledger tab
        fireEvent.click(screen.getByRole('tab', { name: /TrustGraph & Ledger/i }));
        expect(screen.getByText(/TrustGraph Topology/i)).toBeInTheDocument();

        // Switch to Agent 99 Debrief tab
        fireEvent.click(screen.getByRole('tab', { name: /Agent 99 Debrief/i }));
        expect(screen.getByText('VERIFIED SUCCESS')).toBeInTheDocument();
        expect(screen.getByText('All invariants satisfied.')).toBeInTheDocument();

        // Trigger export buttons and pin button
        fireEvent.click(screen.getByLabelText('Export dossier as Markdown'));
        expect(mission_history_service.download_dossier_export).toHaveBeenCalledWith(
            sample_dossier,
            'md'
        );

        fireEvent.click(screen.getByLabelText('Pin mission'));
        expect(on_pin).toHaveBeenCalledWith('ms-flight-001');
    });

    it('computes sequence-aligned LCS diffs accurately', () => {
        const aligned = compute_aligned_diff('alpha\nbeta\ngamma', 'alpha\nbeta_mod\ngamma\ndelta');
        expect(aligned).toHaveLength(4);
        expect(aligned[0].is_modified).toBe(false);
        expect(aligned[1].is_modified).toBe(true);
        expect(aligned[1].pre).toBe('beta');
        expect(aligned[1].cur).toBe('beta_mod');
        expect(aligned[3].is_added).toBe(true);
        expect(aligned[3].cur).toBe('delta');
    });
});

// Metadata: [Mission_Flight_Dossier_test]


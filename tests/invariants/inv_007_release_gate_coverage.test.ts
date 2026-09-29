/**
 * @docs ARCHITECTURE:Core
 *
 * ### AI Assist Note
 * **INV-007: Release Gate Coverage & Execution Governance Invariant**
 * Statically enforces that all scripts in `execution/*.py` are governed:
 * 1. Master Gate Verification: `execution/verify_all.py` exists and governs mandatory gates.
 * 2. Critical Release Gates: Mandatory checks (`security_scan.py`, `parity_guard.py`,
 *    `verify_ai_context.py`, `nexus_adversarial_guard.py`, `mcp_audit.py`,
 *    `db_health_check.py`, `verify_audit_chain.py`) must be registered as required (True) checks.
 * 3. Exhaustive Governance: Every single Python file under `execution/` must either be
 *    registered in `VERIFICATION_SUITE` or cataloged in an explicit governed inventory.
 *    No orphaned or unreviewed scripts are permitted.
 *
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: New unclassified execution scripts added without release governance.
 * - **Telemetry Link**: Search `[inv_007_release_gate]` in test logs.
 *
 * // Metadata: [inv_007_release_gate]
 */

import { describe, it, expect } from 'vitest';
import { readdirSync, readFileSync, existsSync } from 'node:fs';
import { resolve } from 'node:path';

describe('INV-007: Release Gate Coverage & Execution Governance Invariant', () => {
    const verifyAllPath = resolve('execution/verify_all.py');
    const executionDir = resolve('execution');

    it('verifies execution/verify_all.py exists and is readable', () => {
        expect(existsSync(verifyAllPath)).toBe(true);
    });

    it('enforces mandatory P0, P1, and P2 gates in verify_all.py', () => {
        const content = readFileSync(verifyAllPath, 'utf-8');

        const mandatoryChecks = [
            'execution/security_scan.py',
            'execution/mcp_audit.py',
            'execution/parity_guard.py',
            'execution/verify_ai_context.py',
            'execution/nexus_adversarial_guard.py',
            'execution/db_health_check.py',
            'execution/verify_audit_chain.py',
        ];

        for (const check of mandatoryChecks) {
            // Must be present and marked as required (True)
            const regex = new RegExp(`\\("${String.raw`[^"]*`}",\\s*"${check}",\\s*True\\)`);
            expect(content).toMatch(regex);
        }
    });

    it('exhaustively accounts for every execution/*.py file', () => {
        const verifyAllContent = readFileSync(verifyAllPath, 'utf-8');
        const files = readdirSync(executionDir).filter(f => f.endsWith('.py'));

        // Governed operational, maintenance, or developer CLI tools that are
        // not directly release gates (e.g. interactive, ad-hoc, or sub-modules)
        const governedOperationalInventory = new Set([
            'audit_manual.py',
            'auto_preview.py',
            'awaken.py',
            'backup_sqlite.py',
            'cargo_fast_check.py',
            'checklist.py',
            'clean_telemetry_logs.py',
            'comment_coverage.py',
            'configure_all_agents_gemma4.py',
            'debrief_mission.py',
            'dedupe_headers.py',
            'dispatch_mission.py',
            'evaluate_annealing.py',
            'fast_hitl_gate.py',
            'generate_api_reference.py',
            'invoke_and_tail_swarm_mission.py',
            'mission_diagnose.py',
            'optimize_local_slot_routing.py',
            'parse_errors.py',
            'pre_pr.py',
            'py_utils.py',
            'quick_run.py',
            'register_agent_2.py',
            'remediate_ai_context.py',
            'restore_agents.py',
            'restore_sqlite.py',
            'rotate_token.py',
            'run_audit.py',
            'run_test_mission.py',
            'runbook_dispatcher.py',
            'seed_safe_permission_policies.py',
            'self_audit_tool.py',
            'server_tail.py',
            'session_manager.py',
            'snapshot_state.py',
            'sovereign_audit.py',
            'suspend_idle_agents.py',
            'swarm_stress_test.py',
            'sync_all_upstream_toolbelts.py',
            'sync_qa99_capabilities.py',
            'sync_version.py',
            'tadpole_mcp_server.py',
            'test_restoration.py',
            'tool_loop_guard.py',
            'ui_integrity_audit.py',
            'verification_contract.py',
            'verify_all.py',
            'verify_browser_sentinel.py',
            'verify_live_improvements.py',
            'verify_safe_skills_passthrough.py',
            'verify_swarm_hierarchy_toolbelts.py',
            'verify_telemetry.py',
            'verify_temporal_and_delegation.py',
        ]);

        const unclassifiedFiles: string[] = [];

        for (const file of files) {
            const relPath = `execution/${file}`;
            const isGated = verifyAllContent.includes(`"${relPath}"`);
            const isOperational = governedOperationalInventory.has(file);

            if (!isGated && !isOperational) {
                unclassifiedFiles.push(file);
            }
        }

        expect(
            unclassifiedFiles,
            `Unclassified execution scripts found without release gate registration or operational inventory classification: ${unclassifiedFiles.join(', ')}`
        ).toEqual([]);
    });
});

// Metadata: [inv_007_release_gate]

/**
 * @docs ARCHITECTURE:Utils
 * 
 * ### AI Assist Note
 * **Oversight Utils**
 * Shared helper functions for governance and oversight ledger entry classification.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Malformed or undefined LedgerEntry fields.
 * - **Telemetry Link**: Search `[oversight_utils]` in audit logs.
 */

import type { LedgerEntry } from '../data/mock_oversight';
import { get_safe_date } from './date_utils';

/**
 * Helper to determine if an action entry was auto-approved via system or policy rules.
 */
export const check_is_auto = (entry: LedgerEntry): boolean => (
    entry.auto_approved === true ||
    entry.approval_type === 'auto' ||
    entry.requires_oversight === false ||
    entry.decision === 'auto_approved' ||
    entry.decided_by === 'auto_policy' ||
    entry.decided_by === 'system'
);

/**
 * Safe date timestamp helper avoiding non-null assertions.
 */
export const get_entry_time = (entry: LedgerEntry): number => (
    get_safe_date(entry, new Date())?.getTime() ?? 0
);

/**
 * Helper to determine if a skill or tool call represents a parent delegation directive
 * coordinating complex multi-agent flows across the swarm hierarchy.
 */
export const is_parent_delegation_directive = (skill_or_tool?: string): boolean => {
    if (!skill_or_tool) return false;
    const s = skill_or_tool.toLowerCase().trim();
    return (
        s === 'issue_alpha_directive' ||
        s === 'spawn_subagent' ||
        s === 'send_mission_directive' ||
        s === 'recruit_specialist' ||
        s === 'recruit'
    );
};

// Metadata: [oversight_utils]

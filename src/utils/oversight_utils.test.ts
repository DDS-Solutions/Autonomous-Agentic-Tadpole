/*
@docs ARCHITECTURE:Utils

### AI Assist Note
**🛡️ Tadpole OS: Oversight Utils Test**
Core test suite verifying ledger classification, auto-approval filters, and parent delegation directive identification.

### 🔍 Debugging & Observability
- **Failure Path**: Regression in parent delegation directive tagging or timestamp parsing.
- **Telemetry Link**: Search `[oversight_utils_test]` in console logs.
*/

/**
 * @file oversight_utils.test.ts
 * @description Unit test suite for oversight utility functions.
 */

import { describe, it, expect } from 'vitest';
import {
    check_is_auto,
    get_entry_time,
    is_parent_delegation_directive
} from './oversight_utils';
import type { LedgerEntry } from '../data/mock_oversight';

describe('oversight_utils', () => {
    describe('is_parent_delegation_directive', () => {
        it('identifies parent delegation directives correctly', () => {
            expect(is_parent_delegation_directive('issue_alpha_directive')).toBe(true);
            expect(is_parent_delegation_directive('spawn_subagent')).toBe(true);
            expect(is_parent_delegation_directive('send_mission_directive')).toBe(true);
            expect(is_parent_delegation_directive('recruit_specialist')).toBe(true);
            expect(is_parent_delegation_directive('recruit')).toBe(true);
        });

        it('handles case-insensitivity and trimming', () => {
            expect(is_parent_delegation_directive('  Spawn_Subagent  ')).toBe(true);
            expect(is_parent_delegation_directive('ISSUE_ALPHA_DIRECTIVE')).toBe(true);
        });

        it('returns false for non-delegation skills and empty/undefined input', () => {
            expect(is_parent_delegation_directive(undefined)).toBe(false);
            expect(is_parent_delegation_directive('')).toBe(false);
            expect(is_parent_delegation_directive('read_codebase_file')).toBe(false);
            expect(is_parent_delegation_directive('file_write')).toBe(false);
            expect(is_parent_delegation_directive('bash_exec')).toBe(false);
        });
    });

    describe('check_is_auto', () => {
        it('detects auto-approved entries across diverse schema flags', () => {
            expect(check_is_auto({ auto_approved: true } as LedgerEntry)).toBe(true);
            expect(check_is_auto({ approval_type: 'auto' } as LedgerEntry)).toBe(true);
            expect(check_is_auto({ requires_oversight: false } as LedgerEntry)).toBe(true);
            expect(check_is_auto({ decision: 'auto_approved' } as LedgerEntry)).toBe(true);
            expect(check_is_auto({ decided_by: 'auto_policy' } as LedgerEntry)).toBe(true);
            expect(check_is_auto({ decided_by: 'system' } as LedgerEntry)).toBe(true);
        });

        it('returns false when manual oversight is required', () => {
            expect(check_is_auto({
                auto_approved: false,
                approval_type: 'manual',
                requires_oversight: true,
                decision: 'approved',
                decided_by: 'operator'
            } as LedgerEntry)).toBe(false);
        });
    });

    describe('get_entry_time', () => {
        it('extracts epoch time safely from valid date strings', () => {
            const dateStr = '2026-09-19T20:00:00.000Z';
            const expectedTime = new Date(dateStr).getTime();
            expect(get_entry_time({ timestamp: dateStr } as LedgerEntry)).toBe(expectedTime);
        });

        it('falls back gracefully on empty or malformed timestamps', () => {
            const time = get_entry_time({} as LedgerEntry);
            expect(typeof time).toBe('number');
            expect(time).toBeGreaterThan(0);
        });
    });
});

// Metadata: [oversight_utils_test]


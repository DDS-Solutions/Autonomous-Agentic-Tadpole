/**
 * @docs ARCHITECTURE:TestSuites
 *
 * ### AI Assist Note
 * **Question Decisions Service Tests**: Verifies optimistic operator answers,
 * `approve_option` evaluation (`approved` vs `rejected`), and automatic rollback on failure.
 *
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Stuck optimistic answer after backend rejection or wrong oversight decision token.
 * - **Trace Scope**: `src/services/chat/question_decisions.test`
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { create_question_decisions } from './question_decisions';
import type { Chat_Message, Sovereign_State } from '../../domain/chat/types';

describe('create_question_decisions', () => {
    let messages: Chat_Message[];
    const submit = vi.fn();
    const report_failure = vi.fn();
    const remove_message = vi.fn((id: string) => {
        messages = messages.filter((m) => m.id !== id);
    });

    beforeEach(() => {
        vi.clearAllMocks();
        messages = [
            {
                id: 'msg-q1',
                sender_id: 'agent-1',
                sender_name: 'Architect',
                agent_id: 'agent-1',
                text: 'Proceed with deploy?',
                scope: 'agent',
                timestamp: '2026-11-26T00:00:00Z',
                parts: [
                    {
                        type: 'question',
                        question_id: 'ovr-1',
                        question: 'Proceed with deploy?',
                        options: ['Approve', 'Reject'],
                        approve_option: 'Approve',
                        status: 'pending',
                    },
                ],
            },
        ];
    });

    const build_command = () => {
        const mock_state = {
            get messages() {
                return messages;
            },
            get_message_by_id: (id: string) => messages.find((m) => m.id === id),
            active_scope: 'agent' as const,
            update_message: (id: string, updates: Partial<Chat_Message>) => {
                messages = messages.map((m) => (m.id === id ? { ...m, ...updates } : m));
            },
            add_message: (msg: Omit<Chat_Message, 'timestamp'> & { id?: string; timestamp?: string }) => {
                messages = [
                    ...messages,
                    {
                        ...msg,
                        id: msg.id || 'op-1',
                        timestamp: msg.timestamp || new Date().toISOString(),
                    },
                ];
            },
        } as unknown as Sovereign_State;

        return create_question_decisions({
            get: () => mock_state,
            remove_message,
            submit,
            report_failure,
        });
    };

    it('optimistically records the selected answer and resolves approved decision (Happy Path)', async () => {
        submit.mockResolvedValueOnce(undefined);
        const answer_question = build_command();

        await answer_question('ovr-1', 'Approve');

        expect(submit).toHaveBeenCalledWith('ovr-1', 'approved', 'Approve');
        const part = messages[0].parts?.[0];
        expect(part?.type === 'question' && part.selected_option).toBe('Approve');
        expect(part?.type === 'question' && part.status).toBe('answered');
    });

    it('rolls back optimistic answer and operator bubble when oversight API fails (Failure Path)', async () => {
        submit.mockRejectedValueOnce(new Error('Network down'));
        const answer_question = build_command();

        await answer_question('ovr-1', 'Reject');

        expect(submit).toHaveBeenCalledWith('ovr-1', 'rejected', 'Reject');
        expect(report_failure).toHaveBeenCalledWith('agent-1');
        expect(remove_message).toHaveBeenCalled();
        const part = messages[0].parts?.[0];
        expect(part?.type === 'question' && part.status).toBe('pending');
        expect(part?.type === 'question' && part.selected_option).toBeUndefined();
    });

    it('ignores unknown question IDs safely (Edge Case Path)', async () => {
        const answer_question = build_command();
        await answer_question('ovr-missing', 'Approve');
        expect(submit).not.toHaveBeenCalled();
    });
});

/**
 * @docs ARCHITECTURE:TestSuites
 *
 * ### AI Assist Note
 * **Chat Message List Component Tests**: Verifies non-global `ARTIFACT_BLOCK_REGEX`
 * stability across repeated renders (Improvement #3), `Socratic_Envelope_Card` rendering,
 * and Multiversal `Revert to this tip` callback dispatch.
 *
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Intermittent disappearance of `ArtifactPromotionCard` on re-render due to regex `lastIndex`.
 * - **Trace Scope**: `src/components/chat/Chat_Message_List.test`
 */

import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import '@testing-library/jest-dom/vitest';
import { Chat_Message_List, type Chat_Message } from './Chat_Message_List';

describe('Chat_Message_List', () => {
    const artifact_message: Chat_Message = {
        id: 'node-art-1',
        sender_id: 'agent-1',
        sender_name: 'Architect',
        text: 'Generated script:\n```python\nprint("sovereign")\n```',
        scope: 'agent',
        timestamp: '2026-11-26T12:00:00Z',
        parts: [{ type: 'text', content: 'Generated script:\n```python\nprint("sovereign")\n```' }],
    };

    it('renders ArtifactPromotionCard consistently across consecutive renders without regex lastIndex flicker', () => {
        const on_revert = vi.fn();
        const { rerender } = render(
            <Chat_Message_List
                messages={[artifact_message]}
                max_rendered={100}
                active_scope="agent"
                target_node="Architect"
                onRevert={on_revert}
            />
        );

        expect(screen.getAllByText(/Promote|Save|Artifact/i).length).toBeGreaterThan(0);

        rerender(
            <Chat_Message_List
                messages={[artifact_message]}
                max_rendered={100}
                active_scope="agent"
                target_node="Architect"
                onRevert={on_revert}
            />
        );
        expect(screen.getAllByText(/Promote|Save|Artifact/i).length).toBeGreaterThan(0);

        const revert_btn = screen.getByRole('button', { name: /Revert to this tip/i });
        fireEvent.click(revert_btn);
        expect(on_revert).toHaveBeenCalledWith('node-art-1');
    });

    it('renders Socratic_Envelope_Card for socratic message parts', () => {
        const socratic_message: Chat_Message = {
            id: 'node-soc-1',
            sender_id: 'agent-1',
            sender_name: 'Architect',
            text: '',
            scope: 'agent',
            timestamp: '2026-11-26T12:01:00Z',
            parts: [
                {
                    type: 'socratic',
                    envelope: {
                        status: 'SOCRATIC_GATE_PASS',
                        scope_contract: {
                            mission_vector: 'voice_hardening',
                        },
                        architecture_mode: {
                            mode_name: 'VOICE_HARDENING',
                            privacy_shield_enforced: true,
                        },
                        performance_threshold: {
                            active_model_slot: 2,
                        },
                        pre_cleared_failure_modes: ['Enforce Privacy Shield'],
                    },
                },
            ],
        };

        render(
            <Chat_Message_List
                messages={[socratic_message]}
                max_rendered={100}
                active_scope="agent"
                target_node="Architect"
            />
        );

        expect(screen.getByText(/SOCRATIC_GATE_PASS/i)).toBeInTheDocument();
        expect(screen.getAllByText(/VOICE_HARDENING/i).length).toBeGreaterThan(0);
        expect(screen.getByText(/Enforce Privacy Shield/i)).toBeInTheDocument();
    });
});

/**
 * @docs ARCHITECTURE:State
 *
 * ### AI Assist Note
 * **Chat Domain Types**: Shared chat transcript, session leaf, and store state contracts kept
 * independent of Zustand so reducers, replication, and question-decision modules remain pure and testable.
 *
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Type drift between `sovereign_store` actions and replication envelopes.
 * - **Telemetry Link**: Search `[chat_types]` in logs.
 */

import type { Message_Part } from '../../types';

export type Sovereign_Scope = 'agent' | 'cluster' | 'swarm';

export interface SessionLeaf {
    id: string;
    role: string;
    content: string;
    created_at: string;
}

export interface Chat_Message {
    id: string;
    sender_id: string;
    sender_name: string;
    text: string;
    parts?: Message_Part[];
    timestamp: string | number | Date;
    scope: Sovereign_Scope | string;
    agent_id?: string;
    is_sub_agent?: boolean;
    lineage?: string[];
    target_node?: string;
}

export interface Mission_Execution_Metrics {
    mission_id?: string;
    agent_id?: string;
    turns: number;
    tool_calls_attempted: number;
    tool_calls_failed: number;
    cached_reads_hit: number;
    files_modified_count: number;
    total_input_tokens: number;
    total_output_tokens: number;
    peak_context_tokens: number;
    total_cost_micro_usd: number;
    total_summarizations: number;
    total_sub_agents: number;
    excessive_summarization_warning: boolean;
}

export interface Sovereign_State {
    messages: Chat_Message[];
    message_index_by_id: Record<string, number>;
    active_scope: Sovereign_Scope;
    selected_agent_id: string | null;
    target_agent: string;
    target_cluster: string;
    is_detached: boolean;
    active_node_id: string | null;
    active_mission_id: string | null;
    session_leaves: SessionLeaf[];
    latest_metrics: Mission_Execution_Metrics | null;

    // Actions
    add_message: (msg: Omit<Chat_Message, 'id' | 'timestamp'> & { id?: string; timestamp?: Chat_Message['timestamp'] }, is_remote_sync?: boolean) => void;
    update_message: (id: string, updates: Partial<Chat_Message>, is_remote_sync?: boolean) => void;
    append_message_part: (id: string, part: Message_Part, is_remote_sync?: boolean) => boolean;
    get_message_by_id: (id: string) => Chat_Message | undefined;
    replace_history: (messages: Chat_Message[], active_node_id: string | null, is_remote_sync?: boolean) => void;
    set_scope: (scope: Sovereign_Scope, is_remote_sync?: boolean) => void;
    set_selected_agent_id: (agent_id: string | null, is_remote_sync?: boolean) => void;
    set_target_agent: (name: string, is_remote_sync?: boolean) => void;
    set_target_cluster: (name: string, is_remote_sync?: boolean) => void;
    set_detached: (is_detached: boolean, is_remote_sync?: boolean) => void;
    set_active_node: (node_id: string | null, is_remote_sync?: boolean) => void;
    set_active_mission: (mission_id: string | null, is_remote_sync?: boolean) => void;
    fetch_session_history: (mission_id: string, leaf_id: string) => Promise<void>;
    fetch_mission_leaves: (mission_id: string) => Promise<void>;
    revert_to_node: (mission_id: string, node_id: string) => Promise<void>;
    set_execution_metrics: (metrics: Mission_Execution_Metrics, is_remote_sync?: boolean) => void;
    resolve_user_question: (question_id: string, answer: string) => Promise<void>;
    clear_history: (is_remote_sync?: boolean) => void;
    initialize_sync_listeners: () => () => void;
}

// Metadata: [chat_types]

/**
 * @docs ARCHITECTURE:UI-Components
 * 
 * ### AI Assist Note
 * **Gets the simulated user session role.**
 * Handles reactive state and high-fidelity user interactions.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: UI regression, hook desync, or API timeout.
 * - **Telemetry Link**: Search `[useMemoryWorkspace]` in observability traces.
 */

import { useState, useEffect, useMemo, useCallback } from 'react';
import { use_agent_store } from '../../../stores/agent_store';
import { use_memory_store } from '../../../stores/memory_store';
import type { MemoryEntry } from '../../../types/schemas';

/**
 * Gets the simulated user session role.
 * Defaults to 'Cognitive Architect' to allow local developer access.
 */
export const get_session_role = (): string => {
    if (typeof window === 'undefined') return 'Cognitive Architect';
    return localStorage.getItem('tadpole_session_role') || 'Cognitive Architect';
};

/**
 * Checks if the user is authorized to modify memory.
 */
export const is_user_authorized = (): boolean => {
    const role = get_session_role();
    return role === 'Cognitive Architect' || role === 'Admin';
};

export const useMemoryWorkspace = (
    is_memory_node: boolean,
    active_info_tab: 'info' | 'memory'
) => {
    const agents = use_agent_store(state => state.agents);
    const fetch_agents = use_agent_store(state => state.fetch_agents);

    const {
        memories,
        is_loading: memory_loading,
        error: memory_error,
        fetch_memories,
        save_memory,
        delete_memory,
        search_memories
    } = use_memory_store();

    const [selected_agent_id, set_selected_agent_id] = useState<string>('');
    const [search_query, set_search_query] = useState('');
    const [search_results, set_search_results] = useState<MemoryEntry[] | null>(null);
    const [is_searching, set_is_searching] = useState(false);
    const [new_memory_text, setNew_memory_text] = useState('');

    const has_write_permission = useMemo(() => is_user_authorized(), []);

    // Fetch agents on mount if list is empty
    useEffect(() => {
        if (agents.length === 0) {
            fetch_agents();
        }
    }, [agents.length, fetch_agents]);

    // Auto-select first agent when memory node is loaded
    useEffect(() => {
        if (is_memory_node && agents.length > 0 && !selected_agent_id) {
            set_selected_agent_id(agents[0].id);
        }
    }, [is_memory_node, agents, selected_agent_id]);

    // Load memories with a 150ms debounce to prevent race conditions during rapid switching
    useEffect(() => {
        if (!selected_agent_id || active_info_tab !== 'memory') {
            return;
        }

        const handler = setTimeout(() => {
            fetch_memories(selected_agent_id);
        }, 150);

        return () => {
            clearTimeout(handler);
        };
    }, [selected_agent_id, active_info_tab, fetch_memories]);

    const handle_inject_memory = useCallback(async () => {
        if (!new_memory_text || !selected_agent_id) return;
        if (!is_user_authorized()) {
            console.warn('[MemoryWorkspace] Unauthorized memory injection attempt.');
            return;
        }
        try {
            await save_memory(selected_agent_id, new_memory_text);
            setNew_memory_text('');
        } catch (err) {
            console.error('[MemoryWorkspace] Failed to inject memory:', err);
        }
    }, [new_memory_text, selected_agent_id, save_memory]);

    const handle_search = useCallback(async () => {
        if (!search_query) {
            set_search_results(null);
            return;
        }
        set_is_searching(true);
        try {
            const results = await search_memories(search_query);
            set_search_results(results);
        } catch (err) {
            console.error('[MemoryWorkspace] Failed to search vector memories:', err);
        } finally {
            set_is_searching(false);
        }
    }, [search_query, search_memories]);

    const handle_delete_memory = useCallback(async (memory_id: string) => {
        if (!selected_agent_id) return;
        if (!is_user_authorized()) {
            console.warn('[MemoryWorkspace] Unauthorized memory deletion attempt.');
            return;
        }
        try {
            await delete_memory(selected_agent_id, memory_id);
            // Clear search results if deleted to keep sync
            set_search_results(prev => prev ? prev.filter(m => m.id !== memory_id) : null);
        } catch (err) {
            console.error('[MemoryWorkspace] Failed to delete memory:', err);
        }
    }, [selected_agent_id, delete_memory]);

    const display_memories = useMemo(() => {
        if (search_results !== null) {
            return search_results;
        }
        return memories || [];
    }, [search_results, memories]);

    return {
        agents,
        selected_agent_id,
        set_selected_agent_id,
        search_query,
        set_search_query,
        search_results,
        set_search_results,
        is_searching,
        new_memory_text,
        setNew_memory_text,
        display_memories,
        memory_loading,
        memory_error,
        has_write_permission,
        handle_inject_memory,
        handle_search,
        handle_delete_memory
    };
};

// Metadata: [useMemoryWorkspace]

/**
 * @docs ARCHITECTURE:Core
 *
 * ### AI Assist Note
 * **Agent Hierarchy Resolver**: Deterministic leadership resolution across UI consumers
 * (`SovereignChat`, `Org_Chart`). Never returns undefined if at least one agent is provided.
 *
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Empty agent array returns undefined; caller must guard before accessing fields.
 * - **Telemetry Link**: Search `[agent_hierarchy]` in UI logs.
 */

export interface HierarchyAgentLike {
    id: string;
    name: string;
    role?: string;
}

/**
 * Resolves the primary root Alpha (CEO / Lead Agent).
 * Precedence:
 * 1. Role is 'CEO' (case-insensitive) or name includes 'nine'
 * 2. Agent with ID '1'
 * 3. First agent in list
 */
export function resolve_alpha<T extends HierarchyAgentLike>(agents: readonly T[]): T | undefined {
    if (!agents || agents.length === 0) return undefined;

    return (
        agents.find(a => a.role?.toLowerCase() === 'ceo' || a.name?.toLowerCase().includes('nine')) ??
        agents.find(a => a.id === '1') ??
        agents[0]
    );
}

/**
 * Resolves the secondary Nexus (COO / Sub-Lead Agent).
 */
export function resolve_nexus<T extends HierarchyAgentLike>(agents: readonly T[], alpha?: T | string): T | undefined {
    if (!agents || agents.length === 0) return undefined;

    const alpha_id = typeof alpha === 'string' ? alpha : alpha?.id;

    return (
        agents.find(a =>
            (a.role?.toLowerCase() === 'coo' || a.name === 'Tadpole Alpha' || a.name === 'Tadpole') &&
            a.id !== alpha_id
        ) ??
        agents.find(a => a.id === '2' && a.id !== alpha_id) ??
        agents.find(a => a.id !== alpha_id)
    );
}

// Metadata: [agent_hierarchy]

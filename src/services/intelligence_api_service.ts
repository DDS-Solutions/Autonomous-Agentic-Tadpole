/*
### AI Assist Note
**🛡️ Tadpole OS: Intelligence Api Service**
Core system module providing specialized functionality for the agent swarm.

### 🔍 Debugging & Observability
- **Failure Path**: Unexpected execution drift or type compatibility issues.
- **Telemetry Link**: Search `[intelligence_api_service]` in console logs.
*/

/**
 * @docs ARCHITECTURE:Services
 * 
 * ### AI Assist Note
 * **Intelligence Service**: Orchestrates interaction with the backend code graph 
 * and blast radius analysis engines. Fetches high-fidelity symbol interdependency 
 * data for visualization and impact prediction (MOD-04).
 */

import { api_request } from './base_api_service';
import type { SymbolNode } from '../types/schemas';

export interface CodeGraphData {
    nodes: SymbolNode[];
    links: { source: string; target: string }[];
    anomalies?: string[];
}

export interface KnowledgeEntry {
    id: string;
    text: string;
    topic: string;
    cluster_id: string | null;
    source_node_id: string | null;
    source_agent_id: string | null;
    content_hash: string;
    confidence: number;
    human_confirmed: boolean;
    ttl: number | null;
    created_at: number;
    access_count: number;
    concept_type: string;
    title: string | null;
    description: string | null;
    resource_uri: string | null;
    tags: string | null;
    constraints_json?: string | null;
    provenance_chain?: string | null;
}

export interface KnowledgeEdge {
    id: string;
    source_id: string;
    target_id: string;
    relation_type: string;
    weight: number;
    created_at: number;
}

class IntelligenceApiService {
    /**
     * Fetches the full high-fidelity code symbol graph.
     */
    async get_graph(): Promise<CodeGraphData> {
        return api_request<CodeGraphData>('/v1/intelligence/graph');
    }

    /**
     * Calculates the blast radius (affected symbols) for a target symbol.
     */
    async get_blast_radius(name: string, path: string, signal?: AbortSignal): Promise<SymbolNode[]> {
        const query_params = new URLSearchParams({ name, path }).toString();
        return api_request<SymbolNode[]>(`/v1/intelligence/blast-radius?${query_params}`, { signal });
    }

    /**
     * Fetches paginated OKF knowledge entries from the IKS.
     */
    async get_knowledge(params?: {
        topic?: string;
        cluster_id?: string;
        concept_type?: string;
        limit?: number;
        offset?: number;
    }, signal?: AbortSignal): Promise<KnowledgeEntry[]> {
        const query_params = new URLSearchParams();
        if (params?.topic) query_params.append('topic', params.topic);
        if (params?.cluster_id) query_params.append('cluster_id', params.cluster_id);
        if (params?.concept_type) query_params.append('concept_type', params.concept_type);
        if (params?.limit !== undefined) query_params.append('limit', params.limit.toString());
        if (params?.offset !== undefined) query_params.append('offset', params.offset.toString());

        const query_str = query_params.toString();
        return api_request<KnowledgeEntry[]>(`/v1/knowledge?${query_str}`, { signal });
    }

    /**
     * Fetches semantic peer nodes for a specific OKF knowledge entry.
     */
    async get_knowledge_peers(id: string, limit?: number, signal?: AbortSignal): Promise<KnowledgeEntry[]> {
        const query_params = new URLSearchParams();
        if (limit !== undefined) query_params.append('limit', limit.toString());
        const query_str = query_params.toString();
        return api_request<KnowledgeEntry[]>(`/v1/knowledge/${id}/peers?${query_str}`, { signal });
    }

    /**
     * Fetches typed relational edges (OKF v0.3).
     */
    async get_knowledge_edges(params?: { source_id?: string; target_id?: string }, signal?: AbortSignal): Promise<KnowledgeEdge[]> {
        const query_params = new URLSearchParams();
        if (params?.source_id) query_params.append('source_id', params.source_id);
        if (params?.target_id) query_params.append('target_id', params.target_id);
        const query_str = query_params.toString();
        return api_request<KnowledgeEdge[]>(`/v1/knowledge/edges?${query_str}`, { signal });
    }

    /**
     * Synthesizes cross-agent knowledge entries using Ollama (OKF v0.3).
     */
    async synthesize_knowledge(req: { source_ids: string[]; topic: string; title: string; concept_type?: string }): Promise<{
        synthesized_entry: KnowledgeEntry;
        edges_created: number;
        contradiction_warning?: string;
    }> {
        return api_request('/v1/knowledge/synthesize', {
            method: 'POST',
            body: JSON.stringify(req),
        });
    }
}

export const intelligence_api_service = new IntelligenceApiService();





// Metadata: [intelligence_api_service]

/*
### AI Assist Note
**🛡️ Tadpole OS: Intelligence Api Service**
Core system module providing specialized functionality for the agent swarm.

### 🔍 Debugging & Observability
- **Failure Path**: Unexpected execution drift or type compatibility issues.
- **Telemetry Link**: Traced via active system logging channels.
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
    async get_blast_radius(name: string, path: string): Promise<SymbolNode[]> {
        return api_request<SymbolNode[]>(`/v1/intelligence/blast-radius?name=${encodeURIComponent(name)}&path=${encodeURIComponent(path)}`);
    }
}

export const intelligence_api_service = new IntelligenceApiService();

// Metadata: [intelligence_api_service]

// Metadata: [intelligence_api_service]

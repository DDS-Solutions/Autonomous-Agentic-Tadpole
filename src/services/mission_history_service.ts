/**
 * @docs ARCHITECTURE:Services
 * @docs API_REFERENCE:Endpoints
 *
 * ### AI Assist Note
 * **mission_history_service**: Frontend service layer for the Sovereign Observability Core (SOC)
 * Mission Flight Recorder. Interfaces with `/v1/missions`, `/v1/missions/{id}/dossier`,
 * `/v1/missions/{id}/pin`, and `/v1/cas/history`, and provides one-click Markdown/JSON
 * forensic dossier export for Overlord Entity (0).
 *
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Backend 404/500 on dossier assembly or invalid mission ID parameter.
 * - **Telemetry Link**: Search `[mission_history_service]` in console logs.
 */

import { api_request } from './base_api_service';
import type {
    CatalogResponse,
    CasRevisionSummary,
    MissionDossier,
} from '../types/mission_dossier';

export interface CatalogQueryParams {
    page?: number;
    per_page?: number;
    status?: string;
    search?: string;
    pinned_only?: boolean;
}

export const mission_history_service = {
    /**
     * Fetches paginated catalog of all missions with aggregated counts.
     */
    fetch_catalog: async (params?: CatalogQueryParams): Promise<CatalogResponse> => {
        const search_params = new URLSearchParams();
        if (params?.page) search_params.set('page', params.page.toString());
        if (params?.per_page) search_params.set('per_page', params.per_page.toString());
        if (params?.status && params.status !== 'all') search_params.set('status', params.status);
        if (params?.search) search_params.set('search', params.search);
        if (params?.pinned_only) search_params.set('pinned_only', 'true');

        const query_str = search_params.toString();
        const endpoint = `/v1/missions${query_str ? `?${query_str}` : ''}`;
        return api_request<CatalogResponse>(endpoint, { method: 'GET' });
    },

    /**
     * Fetches complete composite flight dossier for a specific mission.
     */
    fetch_dossier: async (mission_id: string): Promise<MissionDossier> => {
        const clean_id = encodeURIComponent(mission_id.trim());
        return api_request<MissionDossier>(`/v1/missions/${clean_id}/dossier`, { method: 'GET' });
    },

    /**
     * Toggles or explicitly sets the retention pin status of a mission to protect it from Reaper cycles.
     */
    toggle_pin: async (
        mission_id: string,
        is_pinned?: boolean
    ): Promise<{ success: boolean; is_pinned: boolean; mission_id: string }> => {
        const clean_id = encodeURIComponent(mission_id.trim());
        return api_request<{ success: boolean; is_pinned: boolean; mission_id: string }>(
            `/v1/missions/${clean_id}/pin`,
            {
                method: 'POST',
                body: is_pinned !== undefined ? JSON.stringify({ is_pinned }) : JSON.stringify({}),
            }
        );
    },

    /**
     * Fetches read-only CAS revision history for a specific workspace file.
     */
    fetch_file_history: async (
        file_path: string,
        workspace_root?: string
    ): Promise<CasRevisionSummary[]> => {
        const search_params = new URLSearchParams({ file_path: file_path.trim() });
        if (workspace_root?.trim()) {
            search_params.set('workspace_root', workspace_root.trim());
        }
        const res = await api_request<{ success: boolean; data: CasRevisionSummary[] }>(
            `/v1/cas/history?${search_params.toString()}`,
            { method: 'GET' }
        );
        return res?.data ?? [];
    },

    /**
     * Formats a complete MissionDossier into an ALCOA+ cryptographic Markdown forensic report
     * for Overlord Entity (0).
     */
    format_dossier_markdown: (dossier: MissionDossier): string => {
        const { mission, merkle_audit, logs, diffs, memory_palace, trust_graph, economics, debrief } = dossier;
        const chain_ok = merkle_audit?.chain_verified ?? logs.every((l) => l.hash === null || l.is_verified);
        const lines: string[] = [
            `# Sovereign Flight Dossier — ${mission.title || 'Untitled Mission'}`,
            ``,
            `- **Mission ID**: \`${mission.id}\``,
            `- **Executing Agent**: \`${mission.agent_id}\``,
            `- **Status**: \`${mission.status.toUpperCase()}\``,
            `- **Retention Pin**: \`${mission.is_pinned ? 'PINNED (Reaper Exempt)' : 'Standard Retention'}\``,
            `- **Created At**: \`${mission.created_at}\``,
            `- **Updated At**: \`${mission.updated_at}\``,
            `- **ALCOA+ Merkle Integrity**: \`${chain_ok ? 'VERIFIED' : 'TAMPER_DETECTED'}\``,
            merkle_audit?.head_hash ? `- **Head Merkle Hash**: \`${merkle_audit.head_hash}\`` : '',
            `- **Economics**: \`$${economics.total_cost_usd.toFixed(4)} / $${economics.budget_usd.toFixed(2)}\` (${economics.total_tokens ?? 0} tokens)`,
            `- **Agent 99 Verdict**: \`${debrief.verdict ?? 'IN_PROGRESS'}\` (Drift: \`${debrief.drift_score !== null ? debrief.drift_score.toFixed(4) : 'N/A'}\`)`,
            ``,
            `## 1. Cryptographic Merkle Audit Trail (${logs.length} Turns)`,
            ``,
        ].filter(Boolean);

        if (logs.length === 0) {
            lines.push(`_No execution steps recorded._`, ``);
        } else {
            for (const step of logs) {
                const idx = step.step_index ?? 0;
                const badge = step.is_verified ? 'VALID' : step.hash === null ? 'LEGACY' : 'TAMPERED';
                lines.push(
                    `### Step #${idx} [${step.severity.toUpperCase()}] — ${step.source} (\`${step.agent_id}\`)`,
                    `- **Timestamp**: \`${step.timestamp}\``,
                    `- **Merkle Proof**: \`${badge}\` | Hash: \`${step.hash ?? 'none'}\` | Prev: \`${step.prev_hash ?? 'GENESIS'}\``
                );
                if (step.tool_name) {
                    lines.push(`- **Tool Invoked**: \`${step.tool_name}\`${step.duration_ms !== null && step.duration_ms !== undefined ? ` (${step.duration_ms}ms)` : ''}`);
                }
                if (step.thought_summary) {
                    lines.push(`- **Thought Summary**: ${step.thought_summary}`);
                }
                lines.push(``, '```text', step.text, '```', ``);
            }
        }

        lines.push(`## 2. CAS Workspace File Mutations (${diffs.length} Files)`, ``);
        if (diffs.length === 0) {
            lines.push(`_No workspace file mutations recorded._`, ``);
        } else {
            for (const d of diffs) {
                lines.push(
                    `### \`${d.file_path}\` (v${d.version_num} | +${d.diff_summary.additions} / -${d.diff_summary.deletions})`,
                    `- **Pre-Mutation SHA-256**: \`${d.hash}\` (${d.size_bytes} bytes)`,
                    ``
                );
            }
        }

        lines.push(`## 3. Memory Palace Findings (${memory_palace.length} Entries)`, ``);
        if (memory_palace.length === 0) {
            lines.push(`_No swarm findings committed._`, ``);
        } else {
            for (const f of memory_palace) {
                lines.push(`- **[${f.topic}] (\`${f.agent_id}\`)**: ${f.finding}`);
            }
            lines.push(``);
        }

        lines.push(
            `## 4. TrustGraph & 2PC Ledger`,
            `- **Entities**: ${trust_graph.entities.length} | **Relations**: ${trust_graph.relations.length}`,
            `- **2PC Ledger Transactions**: ${economics.transactions.length}`,
            ``,
            `## 5. Agent 99 QA Auditor Debrief`,
            ``,
            debrief.agent99_notes || '_No Agent 99 debrief transcript recorded._',
            ``,
            `### Recommendations`,
            ...debrief.recommendations.map((r) => `- ${r}`),
            ``
        );

        return lines.join('\n');
    },

    /**
     * Triggers a browser download of the mission dossier in Markdown (.md) or JSON (.json) format.
     */
    download_dossier_export: (dossier: MissionDossier, format: 'md' | 'json'): void => {
        console.debug(`[mission_history_service] Exporting dossier ${dossier.mission.id} as ${format}`);
        const content =
            format === 'md'
                ? mission_history_service.format_dossier_markdown(dossier)
                : JSON.stringify(dossier, null, 2);
        const mime = format === 'md' ? 'text/markdown;charset=utf-8' : 'application/json;charset=utf-8';
        const blob = new Blob([content], { type: mime });
        const url = URL.createObjectURL(blob);
        const link = document.createElement('a');
        link.href = url;
        link.download = `flight-dossier-${dossier.mission.id.slice(0, 8)}.${format}`;
        document.body.appendChild(link);
        link.click();
        document.body.removeChild(link);
        URL.revokeObjectURL(url);
    },
};

// Metadata: [mission_history_service]


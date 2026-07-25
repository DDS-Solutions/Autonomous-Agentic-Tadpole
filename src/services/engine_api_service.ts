/**
 * @docs ARCHITECTURE:Services
 * 
 * ### AI Assist Note
 * **Engine Domain API Service**: Frontend API client for calling engine-specific endpoints
 * (health, deployments, audio transcription, template installation).
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Network timeout or malformed payload.
 * - **Telemetry Link**: Search `[engine_api_service]` in console logs.
 */

import { api_request, DEPLOY_TIMEOUT } from './base_api_service';

export interface EngineStatus {
    status: string;
    version: string;
    heartbeat: string;
    active_agents: number;
    features: string[];
}

export const engine_api_service = {
    /**
     * get_engine_status
     * Fetches detailed health and feature metrics from the engine.
     */
    get_engine_status: async (options: RequestInit = {}): Promise<EngineStatus | null> => {
        try {
            return await api_request<EngineStatus>('/v1/engine/health', {
                method: 'GET',
                timeout: 5000,
                ...options
            });
        } catch (error) {
            console.error('[engine_api_service] Failed to fetch engine status:', error);
            return null;
        }
    },

    /**
     * check_health
     * Checks if the TadpoleOS instance is reachable.
     */
    check_health: async (): Promise<boolean> => {
        try {
            const status = await engine_api_service.get_engine_status();
            return status !== null;
        } catch (error) {
            console.error('[engine_api_service] Health check failed:', error);
            return false;
        }
    },

    /**
     * deploy_engine
     * Triggers a production deployment of the engine.
     */
    deploy_engine: async (target?: string | number): Promise<{ status: string, output?: string }> => {
        const url = target ? `/v1/engine/deploy?target=${target}` : '/v1/engine/deploy';
        return api_request<{ status: string, output?: string }>(url, {
            method: 'POST',
            timeout: DEPLOY_TIMEOUT
        });
    },

    /**
     * speak
     * Synthesizes text to audio using the backend TTS engine.
     */
    speak: async (text: string, voice?: string, engine?: string): Promise<Blob> => {
        return api_request<Blob>('/v1/engine/speak', {
            method: 'POST',
            body: JSON.stringify({ text, voice, engine }),
            response_type: 'blob'
        });
    },

    /**
     * kill_agents
     * Halts all running agents. Requires confirmation flag to prevent accidental execution.
     */
    kill_agents: async (options?: { confirm?: boolean }): Promise<{ success: boolean }> => {
        if (!options?.confirm) {
            throw new Error('[engine_api_service] Action kill_agents requires explicit confirmation.');
        }
        await api_request('/v1/engine/kill', { method: 'POST' });
        return { success: true };
    },

    /**
     * shutdown_engine
     * Shuts down the backend server. Requires confirmation flag to prevent accidental execution.
     */
    shutdown_engine: async (options?: { confirm?: boolean }): Promise<{ success: boolean }> => {
        if (!options?.confirm) {
            throw new Error('[engine_api_service] Action shutdown_engine requires explicit confirmation.');
        }
        await api_request('/v1/engine/shutdown', { method: 'POST' });
        return { success: true };
    },

    /**
     * transcribe
     * Transcribes audio using the backend's high-fidelity Whisper engine.
     * Content-Type is omitted completely to let FormData automatically compute boundaries.
     */
    transcribe: async (audio_blob: Blob): Promise<string> => {
        const form_data = new FormData();
        form_data.append('file', audio_blob, 'speech.wav');

        const data = await api_request<{ text?: string }>('/v1/engine/transcribe', {
            method: 'POST',
            body: form_data
        });

        return data.text || '';
    },

    /**
     * install_template
     * Installs a template from the official repository.
     * Hardens the input URL and path parameters to prevent supply chain and RCE injection vectors.
     */
    install_template: async (repository_url: string, path: string): Promise<{ success: boolean }> => {
        // Validate URL format and prevent injection flags
        const trimmed_url = repository_url.trim();
        if (trimmed_url.startsWith('-')) {
            throw new Error('[engine_api_service] Invalid repository URL: Cannot start with hyphen.');
        }

        try {
            const parsed_url = new URL(trimmed_url);
            const valid_protocols = ['http:', 'https:', 'git:'];
            if (!valid_protocols.includes(parsed_url.protocol)) {
                throw new Error(`[engine_api_service] Invalid protocol: ${parsed_url.protocol}. Protocol must be http, https, or git.`);
            }
        } catch (e) {
            throw new Error(`[engine_api_service] Invalid repository URL: ${(e as Error).message}`, { cause: e });
        }

        // Validate path traversal attempts
        if (path.includes('..') || path.startsWith('/') || path.startsWith('\\')) {
            throw new Error('[engine_api_service] Invalid template path: Path traversal or absolute paths are prohibited.');
        }

        await api_request('/v1/engine/templates/install', {
            method: 'POST',
            body: JSON.stringify({ repository_url: trimmed_url, path })
        });
        return { success: true };
    },

    /**
     * pre_pr_engine
     * Triggers the Pre-PR Quality Gate checks on the backend engine.
     */
    pre_pr_engine: async (): Promise<{ status: string, output?: string, error?: string }> => {
        return api_request<{ status: string, output?: string, error?: string }>('/v1/engine/pre-pr', {
            method: 'POST'
        });
    }
};

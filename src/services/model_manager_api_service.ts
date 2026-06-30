/**
 * @docs ARCHITECTURE:Services
 * 
 * ### AI Assist Note
 * **Model Manager API Service**: Frontend API client for calling model manager endpoints
 * (provider testing, configuration updates, pricing sync, catalog pull).
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Provider unresponsive, connection handshake errors.
 * - **Telemetry Link**: Search `[model_manager_api_service]` in console logs.
 */

import { api_request } from './base_api_service';
import type { Provider_Test_Config, Store_Model } from './system_api_service';

export interface ProviderConfig {
    id: string;
    name: string;
    protocol: string;
    api_key?: string;
    base_url?: string;
    [key: string]: unknown;
}

export const model_manager_api_service = {
    /**
     * test_provider
     * Connectivity test Trace for a given provider configuration.
     */
    test_provider: async (config: Provider_Test_Config): Promise<{ status: string; latency?: number; message?: string }> => {
        try {
            return await api_request<{ status: string; latency?: number }>(`/v1/model-manager/providers/${config.id}/test`, {
                method: 'POST',
                body: JSON.stringify(config)
            });
        } catch (error) {
            console.error('[model_manager_api_service] Provider test connection error:', error);
            const is_timeout = error === 'TIMEOUT';
            const msg = is_timeout
                ? 'Handshake timeout: The provider endpoint is unresponsive.'
                : (error instanceof Error ? error.message : 'Network connection refused.');
            return { status: 'error', message: msg };
        }
    },

    /**
     * get_providers
     * Returns all registered AI infrastructure providers.
     */
    get_providers: async (): Promise<Record<string, unknown>[]> => {
        try {
            return await api_request<Record<string, unknown>[]>('/v1/model-manager/providers', { method: 'GET' });
        } catch (error) {
            console.error('[model_manager_api_service] Failed to fetch providers:', error);
            throw error;
        }
    },

    /**
     * update_provider
     * Updates or creates an AI infrastructure provider.
     */
    update_provider: async (id: string, config: Record<string, unknown>): Promise<{ status: string }> => {
        return api_request<{ status: string }>(`/v1/model-manager/providers/${id}`, {
            method: 'PUT',
            body: JSON.stringify(config)
        });
    },

    /**
     * delete_provider
     * Deletes an AI infrastructure provider. Requires explicit confirmation flag.
     */
    delete_provider: async (id: string, options?: { confirm?: boolean }): Promise<{ success: boolean }> => {
        if (!options?.confirm) {
            throw new Error('[model_manager_api_service] Action delete_provider requires explicit confirmation.');
        }
        await api_request(`/v1/model-manager/providers/${id}`, { method: 'DELETE' });
        return { success: true };
    },

    /**
     * sync_provider_models
     * Triggers dynamic discovery of available models for a provider via IMR-01.
     */
    sync_provider_models: async (id: string): Promise<{ status: string; added: number; discovered: number; message: string }> => {
        return api_request<{ status: string; added: number; discovered: number; message: string }>(`/v1/model-manager/providers/${id}/sync`, {
            method: 'POST'
        });
    },

    /**
     * update_model
     * Updates or creates an AI infrastructure model.
     */
    update_model: async (id: string, entry: Record<string, unknown>): Promise<{ status: string }> => {
        return api_request<{ status: string }>(`/v1/model-manager/models/${id}`, {
            method: 'PUT',
            body: JSON.stringify(entry)
        });
    },

    /**
     * delete_model
     * Deletes an AI infrastructure model entry. Requires explicit confirmation flag.
     */
    delete_model: async (id: string, options?: { confirm?: boolean }): Promise<{ success: boolean }> => {
        if (!options?.confirm) {
            throw new Error('[model_manager_api_service] Action delete_model requires explicit confirmation.');
        }
        await api_request(`/v1/model-manager/models/${id}`, { method: 'DELETE' });
        return { success: true };
    },

    /**
     * get_models
     * Returns all registered AI infrastructure models.
     */
    get_models: async (): Promise<Record<string, unknown>[]> => {
        try {
            return await api_request<Record<string, unknown>[]>('/v1/model-manager/models', { method: 'GET' });
        } catch (error) {
            console.error('[model_manager_api_service] Failed to fetch models:', error);
            throw error;
        }
    },

    /**
     * get_model_catalog
     * Fetches the curated model catalog from the infrastructure tier.
     */
    get_model_catalog: async (): Promise<Store_Model[]> => {
        try {
            return await api_request<Store_Model[]>('/v1/model-manager/model-store/catalog', { method: 'GET' });
        } catch (error) {
            console.error('[model_manager_api_service] Failed to fetch model catalog:', error);
            throw error;
        }
    },

    /**
     * pull_model
     * Initiates a model pull/download sequence on a specific Bunker node.
     */
    pull_model: async (model_id: string, node_id: string): Promise<{ status: string }> => {
        return api_request<{ status: string }>('/v1/model-manager/model-store/pull', {
            method: 'POST',
            body: JSON.stringify({ tag: model_id, node_id: node_id })
        });
    }
};

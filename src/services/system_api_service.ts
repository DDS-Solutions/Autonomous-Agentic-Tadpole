/**
 * @docs ARCHITECTURE:Services
 * @docs API_REFERENCE:Endpoints
 * 
 * ### AI Assist Note
 * **Engine Domain Service**: Low-level infrastructure interface for system health, deployment, and benchmark diagnostics. 
 * Orchestrates hardware discovery, scheduled job lifecycles, and direct Rust sidecar lifecycle commands.
 * Includes **IMR-01 (Intelligent Model Registry)** bridge for automated model discovery and capability sync.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Deployment timeout (exceeding 2h `DEPLOY_TIMEOUT`), hardware discovery failure (unreachable nodes), or job queue starvation.
 * - **IMR-01 Link**: See `sync_provider_models` for dynamic discovery logic.
 * - **Telemetry Link**: Look for `engine:health` or search `[system_api_service]` in tracing logs.
 * 
 * @aiContext
 * - **Dependencies**: `base_api_service`.
 * - **Side Effects**: Triggers high-privilege platform commands (engine deploy/shutdown/restart).
 * - **Mocking**: Mock `api_request` from `base_api_service` for unit tests.
 */

import type { Swarm_Node } from '../types/index';
export type { Swarm_Node };
import { api_request } from './base_api_service';

import { engine_api_service } from './engine_api_service';
import { model_manager_api_service } from './model_manager_api_service';
import { oversight_api_service } from './oversight_api_service';
import { continuity_api_service } from './continuity_api_service';

/** Quota_Details - Consumption metrics for budget governance. */
export interface Quota_Details {
    entity_id: string;
    budget_usd: number;
    used_usd: number;
    reset_period: 'daily' | 'monthly' | 'never';
    last_reset_at: string;
    next_reset_at: string;
}

export interface System_Defense {
    memory_pressure: number;
    cpu_load: number;
    sandbox_status: string;
    sandbox_type: string;
    merkle_integrity: number;
}

export interface Quotas {
    total_budget: number;
    total_spent: number;
    remaining: number;
    efficiency: number;
    agent_quotas: Quota_Details[];
    system_defense: System_Defense;
}

/** Audit_Entry - Represents a single decision record in the immutable audit trail. */
export interface Audit_Entry {
    id: string;
    agent_id: string;
    skill: string | null;
    status: string;
    decision: string | null;
    decided_at: string | null;
    created_at: string;
    /** Whether the entry passed the cryptographic integrity check. */
    is_verified: boolean;
}

/** Agent_Health - Real-time health metrics for an active agent. */
export interface Agent_Health {
    agent_id: string;
    name: string;
    status: string;
    failure_count: number;
    last_failure_at: string | null;
    /** True if failure_count is below the threshold and agent is not throttled. */
    is_healthy: boolean;
    /** True if the agent is currently paused due to repeated failure loops. */
    is_throttled: boolean;
}

export interface Store_Model {
    id: string;
    name: string;
    provider: string;
    description: string;
    size: string;
    vram: string;
    tags: string[];
}

export interface Infra_Node { id: string; name: string; host: string; status: string;[key: string]: unknown; }

export interface Provider_Test_Config {
    id: string;
    name: string;
    protocol: string;
    api_key?: string;
    base_url?: string;
    external_id?: string;
    custom_headers?: Record<string, string>;
    audio_model?: string;
    [key: string]: unknown;
}

export interface Benchmark_Record {
    id: string;
    name: string;
    test_id: string;
    category: string;
    mean_ms: number;
    p95_ms?: number;
    p99_ms?: number;
    target_value?: string;
    status: string;
    metadata?: string;
    created_at: string;
    [key: string]: unknown;
}

export interface Scheduled_Job {
    id: string;
    agent_id: string;
    workflow_id?: string | null;
    name: string;
    prompt: string;
    cron_expr: string;
    budget_usd: number;
    enabled: boolean;
    last_run_at: string | null;
    next_run_at: string;
    consecutive_failures: number;
    max_failures: number;
    created_at: string;
}

export interface Workflow_Entry {
    id: string;
    name: string;
    description: string | null;
    created_at: string;
}

export interface Workflow_Step {
    id: string;
    workflow_id: string;
    step_number: number;
    agent_id: string;
    prompt: string;
    budget_usd: number;
}

export interface Scheduled_Job_Run {
    id: string;
    job_id: string;
    mission_id: string | null;
    started_at: string;
    completed_at: string | null;
    status: string;
    cost_usd: number;
    output_summary: string | null;
}

export const system_api_service = {
    // --- Engine Delegation ---
    get_engine_status: engine_api_service.get_engine_status,
    check_health: engine_api_service.check_health,
    deploy_engine: engine_api_service.deploy_engine,
    speak: engine_api_service.speak,
    kill_agents: engine_api_service.kill_agents,
    shutdown_engine: engine_api_service.shutdown_engine,
    transcribe: engine_api_service.transcribe,
    install_template: engine_api_service.install_template,
    pre_pr_engine: engine_api_service.pre_pr_engine,

    // --- Model Manager Delegation ---
    test_provider: model_manager_api_service.test_provider,
    get_providers: model_manager_api_service.get_providers,
    update_provider: model_manager_api_service.update_provider,
    delete_provider: model_manager_api_service.delete_provider,
    sync_provider_models: model_manager_api_service.sync_provider_models,
    update_model: model_manager_api_service.update_model,
    delete_model: model_manager_api_service.delete_model,
    get_models: model_manager_api_service.get_models,
    get_model_catalog: model_manager_api_service.get_model_catalog,
    pull_model: model_manager_api_service.pull_model,

    // --- Oversight Delegation ---
    get_pending_oversight: oversight_api_service.get_pending_oversight,
    get_oversight_ledger: oversight_api_service.get_oversight_ledger,
    decide_oversight: oversight_api_service.decide_oversight,
    get_security_quotas: oversight_api_service.get_security_quotas,
    update_security_quota: oversight_api_service.update_security_quota,
    get_mission_quotas: oversight_api_service.get_mission_quotas,
    update_mission_quota: oversight_api_service.update_mission_quota,
    get_audit_trail: oversight_api_service.get_audit_trail,
    get_agent_health: oversight_api_service.get_agent_health,
    get_integrity_status: oversight_api_service.get_integrity_status,
    update_governance_settings: oversight_api_service.update_governance_settings,

    // --- Continuity Delegation ---
    get_scheduled_jobs: continuity_api_service.get_scheduled_jobs,
    create_scheduled_job: continuity_api_service.create_scheduled_job,
    update_scheduled_job: continuity_api_service.update_scheduled_job,
    delete_scheduled_job: continuity_api_service.delete_scheduled_job,
    get_scheduled_job_runs: continuity_api_service.get_scheduled_job_runs,
    list_continuity_workflows: continuity_api_service.list_continuity_workflows,
    create_continuity_workflows: continuity_api_service.create_continuity_workflows,
    add_continuity_workflows_step: continuity_api_service.add_continuity_workflows_step,
    delete_continuity_workflows: continuity_api_service.delete_continuity_workflows,

    // --- Infrastructure & Shared Services ---
    /**
     * get_nodes
     * Returns all registered Bunker nodes from the infrastructure tier.
     */
    get_nodes: async (options: RequestInit = {}): Promise<Swarm_Node[]> => {
        return api_request<Swarm_Node[]>('/v1/infra/nodes', { 
            method: 'GET',
            ...options
        });
    },

    /**
     * discover_nodes
     * Triggers a network discovery scan for new Bunkers.
     */
    discover_nodes: async (): Promise<{ status: string, discovered: string[] }> => {
        return api_request<{ status: string, discovered: string[] }>('/v1/infra/nodes/discover', { method: 'POST' });
    },

    /**
     * get_benchmarks
     * Returns all historical performance benchmark records.
     */
    get_benchmarks: async (): Promise<Benchmark_Record[]> => {
        return api_request<Benchmark_Record[]>('/v1/benchmarks', { method: 'GET' });
    },

    /**
     * run_benchmark
     * Triggers a specific performance benchmark by test_id.
     */
    run_benchmark: async (test_id: string): Promise<Benchmark_Record> => {
        return api_request<Benchmark_Record>(`/v1/benchmarks/run/${test_id}`, { method: 'POST' });
    },

    /**
     * get_knowledge_docs
     * Lists all available knowledge docs from the backend.
     */
    get_knowledge_docs: async (): Promise<{ category: string; name: string; title: string; }[]> => {
        return api_request<{ category: string; name: string; title: string; }[]>('/v1/docs/knowledge', { method: 'GET' });
    },

    /**
     * get_knowledge_doc
     * Fetches a specific knowledge document's markdown content.
     */
    get_knowledge_doc: async (category: string, name: string): Promise<string> => {
        return api_request<string>(`/v1/docs/knowledge/${category}/${name}`, {
            method: 'GET',
            headers: { 'Accept': 'text/markdown' },
            response_type: 'text'
        });
    },

    /**
     * get_operations_manual
     * Fetches the Operations Manual markdown content.
     */
    get_operations_manual: async (): Promise<string> => {
        return api_request<string>('/v1/docs/operations-manual', {
            method: 'GET',
            headers: { 'Accept': 'text/markdown' },
            response_type: 'text'
        });
    },

    /**
     * get_sovereign_manifest
     * Fetches the real-time Sovereign State Manifest.
     */
    get_sovereign_manifest: async (): Promise<string> => {
        const data = await api_request<{ manifest: string }>('/v1/governance/manifest', { method: 'GET' });
        return data.manifest;
    }
};

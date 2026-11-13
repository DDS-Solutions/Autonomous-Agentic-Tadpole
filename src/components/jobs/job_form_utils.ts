/**
 * @docs ARCHITECTURE:Interface
 *
 * ### AI Assist Note
 * Shared form-state types and factory helpers for scheduled continuity jobs.
 *
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Invalid default job config or form reset regression.
 * - **Telemetry Link**: Search `[job_form_utils]` in UI logs.
 */

export type AcceptModeOption = 'auto' | 'observe';

export interface job_config_state {
    name: string;
    agent_id: string;
    workflow_id: string | null;
    prompt: string;
    cron_expr: string;
    budget_usd: number;
    max_failures: number;
    accept_mode: AcceptModeOption;
}

/**
 * Returns a fresh default scheduled job form configuration.
 */
export function create_empty_job_config(): job_config_state {
    return {
        name: '',
        agent_id: '',
        workflow_id: null,
        prompt: '',
        cron_expr: '0 * * * *',
        budget_usd: 0.10,
        max_failures: 3,
        accept_mode: 'auto'
    };
}

// Metadata: [job_form_utils]


/**
 * @docs ARCHITECTURE:Services
 * 
 * ### AI Assist Note
 * **Continuity Domain API Service**: Frontend API client for calling scheduler jobs,
 * runs history, and workflow definition endpoints.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Job list retrieval timeouts or empty workflow arrays.
 * - **Telemetry Link**: Search `[continuity_api_service]` in console logs.
 */

import { api_request } from './base_api_service';
import type { Scheduled_Job, Scheduled_Job_Run, Workflow_Entry, Workflow_Step } from './system_api_service';

export const continuity_api_service = {
    /**
     * get_scheduled_jobs
     * Lists all autonomous scheduled jobs.
     */
    get_scheduled_jobs: async (): Promise<Scheduled_Job[]> => {
        try {
            const res = await api_request<{ jobs: Scheduled_Job[] } | Scheduled_Job[]>('/v1/continuity/jobs', { method: 'GET' });
            return Array.isArray(res) ? res : (res.jobs || []);
        } catch (error) {
            console.error('[continuity_api_service] Failed to fetch scheduled jobs:', error);
            return [];
        }
    },

    /**
     * create_scheduled_job
     * Creates a new scheduled job for the Continuity Scheduler.
     */
    create_scheduled_job: async (job: Partial<Scheduled_Job>): Promise<Scheduled_Job> => {
        return api_request<Scheduled_Job>('/v1/continuity/jobs', {
            method: 'POST',
            body: JSON.stringify(job)
        });
    },

    /**
     * update_scheduled_job
     * Updates an existing scheduled job.
     */
    update_scheduled_job: async (id: string, job: Partial<Scheduled_Job>): Promise<Scheduled_Job> => {
        return api_request<Scheduled_Job>(`/v1/continuity/jobs/${id}`, {
            method: 'PUT',
            body: JSON.stringify(job)
        });
    },

    /**
     * delete_scheduled_job
     * Deletes a scheduled job.
     */
    delete_scheduled_job: async (id: string): Promise<void> => {
        return api_request<void>(`/v1/continuity/jobs/${id}`, { method: 'DELETE' });
    },

    /**
     * get_scheduled_job_runs
     * Fetches the run history for a specific scheduled job.
     */
    get_scheduled_job_runs: async (id: string): Promise<Scheduled_Job_Run[]> => {
        try {
            const res = await api_request<{ runs: Scheduled_Job_Run[] } | Scheduled_Job_Run[]>(`/v1/continuity/jobs/${id}/runs`, { method: 'GET' });
            return Array.isArray(res) ? res : (res.runs || []);
        } catch (error) {
            console.error('[continuity_api_service] Failed to fetch job runs:', error);
            return [];
        }
    },

    /**
     * list_continuity_workflows
     * Lists all existing workflows for scheduled jobs.
     */
    list_continuity_workflows: async (): Promise<Workflow_Entry[]> => {
        try {
            const res = await api_request<{ workflows: Workflow_Entry[] } | Workflow_Entry[]>('/v1/continuity/workflows', { method: 'GET' });
            return Array.isArray(res) ? res : (res.workflows || []);
        } catch (error) {
            console.error('[continuity_api_service] Failed to fetch workflows:', error);
            return [];
        }
    },

    /**
     * create_continuity_workflows
     * Creates a new workflow definition for scheduled jobs.
     */
    create_continuity_workflows: async (data: { name: string; description?: string }): Promise<Workflow_Entry> => {
        return api_request<Workflow_Entry>('/v1/continuity/workflows', {
            method: 'POST',
            body: JSON.stringify(data)
        });
    },

    /**
     * add_continuity_workflows_step
     * Adds a step to an existing continuity workflow.
     */
    add_continuity_workflows_step: async (workflow_id: string, step: Partial<Workflow_Step>): Promise<Workflow_Step> => {
        return api_request<Workflow_Step>(`/v1/continuity/workflows/${workflow_id}/steps`, {
            method: 'POST',
            body: JSON.stringify(step)
        });
    },

    /**
     * delete_continuity_workflows
     * Deletes a continuity workflow definition. Requires confirmation.
     */
    delete_continuity_workflows: async (workflow_id: string, options?: { confirm?: boolean }): Promise<{ success: boolean }> => {
        if (!options?.confirm) {
            throw new Error('[continuity_api_service] Action delete_continuity_workflows requires explicit confirmation.');
        }
        await api_request(`/v1/continuity/workflows/${workflow_id}`, { method: 'DELETE' });
        return { success: true };
    }
};

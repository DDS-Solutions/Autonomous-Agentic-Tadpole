/**
 * @docs ARCHITECTURE:UI-Hooks
 * 
 * ### AI Assist Note
 * **Autonomous Remediation Hook**: Interprets high-fidelity error codes (RFC 9457)
 * from the Tadpole engine and suggests actionable "Sovereign Repairs."
 *
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: UI regression, hook desync, or API timeout.
 * - **Telemetry Link**: Search `[useErrorRemediation]` in console logs.
 */

import { useCallback } from 'react';

export interface RemediationAction {
    label: string;
    action: () => void;
    severity: 'info' | 'warning' | 'error';
    help_link?: string;
}

export interface EngineError {
    error_code?: string;
    help_link?: string;
    remediation?: string;
    detail?: string;
    type?: string;
}

export function useErrorRemediation() {
    const get_remediation = useCallback((error: EngineError): RemediationAction | null => {
        const code = (error?.error_code || '').toUpperCase();
        const help_link = error?.help_link || undefined;
        const direct_remediation = error?.remediation;

        // 1. Direct remediation string returned from RFC 9457 ProblemDetails
        if (direct_remediation) {
            return {
                label: direct_remediation,
                action: () => {
                    console.debug("[useErrorRemediation] Direct remediation applied:", direct_remediation);
                },
                severity: 'info',
                help_link
            };
        }

        // 2. Specialized Error Code Mappings
        if (code.startsWith('OOM_QUANTIZATION_FALLBACK') || code.startsWith('OOM_FALLBACK')) {
            const model_id = code.split(':')[1] || 'current model';
            return {
                label: `Switch ${model_id} to efficient quantization (q4_K_M)`,
                action: () => {
                    console.debug("[useErrorRemediation] Triggering OOM Remediation for:", model_id);
                },
                severity: 'warning',
                help_link
            };
        }

        if (code.startsWith('RECRUITMENT_FAILED')) {
            const role = code.split(':')[1] || 'specialist';
            return {
                label: `Recruit ${role} from alternate Bunker node`,
                action: () => {
                    console.debug("[useErrorRemediation] Triggering Recruitment Remediation for:", role);
                },
                severity: 'error',
                help_link
            };
        }

        if (code.startsWith('INFRA_ERROR') || code.startsWith('INFRA:')) {
            const provider_id = code.split(':')[1] || 'provider';
            return {
                label: `Verify API Token for ${provider_id}`,
                action: () => {
                    console.debug("[useErrorRemediation] Triggering Connectivity Remediation for:", provider_id);
                },
                severity: 'error',
                help_link
            };
        }

        if (code === 'BUDGET_EXHAUSTED' || code === 'BUDGET-EXHAUSTED') {
            return {
                label: 'Increase mission budget limit or grant quota extension',
                action: () => console.debug("[useErrorRemediation] Triggering budget extension"),
                severity: 'warning',
                help_link
            };
        }

        if (code === 'RECURSION_BLOCKED' || code === 'RECURSION-BLOCKED') {
            return {
                label: 'Pivot execution strategy or provide operator parameter guidance',
                action: () => console.debug("[useErrorRemediation] Triggering loop pivot"),
                severity: 'warning',
                help_link
            };
        }

        if (code === 'UNAUTHORIZED') {
            return {
                label: 'Verify NEURAL_TOKEN in .env or Settings matches the server bearer token',
                action: () => console.debug("[useErrorRemediation] Triggering token verify"),
                severity: 'error',
                help_link
            };
        }

        if (code === 'RATE_LIMIT' || code === 'RATE-LIMIT') {
            return {
                label: 'Wait for provider rate-limit window or switch to alternate model',
                action: () => console.debug("[useErrorRemediation] Triggering rate limit cooldown"),
                severity: 'info',
                help_link
            };
        }

        if (code === 'FORBIDDEN') {
            return {
                label: 'Action denied by security policy. Request higher clearance or operator approval',
                action: () => console.debug("[useErrorRemediation] Triggering clearance request"),
                severity: 'error',
                help_link
            };
        }

        // Default to generic help if only help_link exists
        if (help_link) {
            return {
                label: 'View Resolution Guide',
                action: () => window.open(help_link, '_blank'),
                severity: 'info',
                help_link
            };
        }

        return null;
    }, []);

    return { get_remediation };
}



// Metadata: [useErrorRemediation]

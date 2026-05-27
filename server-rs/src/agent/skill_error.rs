/*!
 * @docs ARCHITECTURE:Core
 * 
 * ### AI Assist Note
 * Localized SkillError enum for agent capability, manifest validation, and safety sanitization exceptions.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Failed manifest validation, recruitment mismatch, or input sanitization block.
 * - **Telemetry Link**: Search `[skill_error]` in trace logs.
 */

use thiserror::Error;

#[derive(Error, Debug)]
pub enum SkillError {
    #[error("Validation Error: {0}")]
    ValidationError(String),

    #[error("Recruitment Failure ({role}): {detail}")]
    RecruitmentFailure {
        recipe_id: String,
        role: String,
        detail: String,
    },

    #[error("Sanitization Violation: {0}")]
    SanitizationViolation(String),
}

// Metadata: [skill_error]

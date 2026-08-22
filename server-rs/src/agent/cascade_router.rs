//! @docs ARCHITECTURE:Agent:CascadeRouter
//!
//! ### AI Assist Note
//! **Tiered Model Cascading Router (Production-Grade)**: Dynamically routes turns between
//! low-latency, zero/cheap cost models (Tier 1: Ollama / Groq / Flash) and
//! frontier reasoning models (Tier 2: Gemini Pro / Claude 3.7 / GPT-4o) based
//! on turn complexity, configurable critical keywords, and capability-aware error escalation.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Model provider rate limit, schema parsing degradation, or invalid tier configuration.
//! - **Telemetry Link**: Search `[cascade_router]` in tracing logs.

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

/// Operational tier classification for LLM models.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelTier {
    /// Sub-100ms, low-cost or local model (Ollama, Groq, Gemini Flash).
    Tier1Fast,
    /// Frontier high-density reasoning model (Gemini Pro, Claude 3.7, GPT-4o).
    Tier2Reasoning,
}

/// Evaluated complexity of an incoming agent turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskComplexity {
    Low,
    Medium,
    High,
    Critical,
}

/// Structured routing decision returned by the cascade router.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutingDecision {
    pub complexity: TaskComplexity,
    pub tier: ModelTier,
    pub provider: String,
    pub model: String,
}

/// Routing policy for model cascading.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CascadePolicy {
    pub tier1_provider: String,
    pub tier1_model: String,
    pub tier2_provider: String,
    pub tier2_model: String,
    pub max_tier1_token_threshold: usize,
    pub auto_escalate_on_mutation: bool,
    pub critical_keywords: Vec<String>,
}

impl Default for CascadePolicy {
    fn default() -> Self {
        Self {
            tier1_provider: "gemini".to_string(),
            tier1_model: "gemini-2.0-flash".to_string(),
            tier2_provider: "gemini".to_string(),
            tier2_model: "gemini-2.0-pro".to_string(),
            max_tier1_token_threshold: 4096,
            auto_escalate_on_mutation: true,
            critical_keywords: vec![
                "RECURSIVE_ALETHEIA".to_string(),
                "ARCHITECTURE_REVIEW".to_string(),
                "SECURITY_AUDIT".to_string(),
                "SWARM_SYNTHESIS".to_string(),
            ],
        }
    }
}

/// Dynamic router determining the optimal model tier for agent turns.
pub struct CascadeRouter {
    pub policy: CascadePolicy,
}

impl Default for CascadeRouter {
    fn default() -> Self {
        Self::new(CascadePolicy::default())
    }
}

impl CascadeRouter {
    pub fn new(policy: CascadePolicy) -> Self {
        Self { policy }
    }

    /// Evaluates task characteristics and returns a structured `RoutingDecision`.
    pub fn route_turn(
        &self,
        prompt: &str,
        token_count: usize,
        is_mutating: bool,
        directive_requires_deep_reasoning: bool,
    ) -> RoutingDecision {
        // 1. Evaluate Complexity
        let complexity = if directive_requires_deep_reasoning
            || self.policy.critical_keywords.iter().any(|k| prompt.contains(k))
        {
            TaskComplexity::Critical
        } else if is_mutating && self.policy.auto_escalate_on_mutation {
            TaskComplexity::High
        } else if token_count > self.policy.max_tier1_token_threshold {
            TaskComplexity::Medium
        } else {
            TaskComplexity::Low
        };

        // 2. Map Complexity to Model Tier cleanly
        let tier = match complexity {
            TaskComplexity::Low | TaskComplexity::Medium => ModelTier::Tier1Fast,
            TaskComplexity::High | TaskComplexity::Critical => ModelTier::Tier2Reasoning,
        };

        // 3. Resolve Provider & Model
        let (provider, model) = match tier {
            ModelTier::Tier1Fast => (self.policy.tier1_provider.clone(), self.policy.tier1_model.clone()),
            ModelTier::Tier2Reasoning => (self.policy.tier2_provider.clone(), self.policy.tier2_model.clone()),
        };

        info!(
            "🧭 [CascadeRouter] Turn routed to {:?} ({}/{}) [Complexity: {:?}, Tokens: {}]",
            tier, provider, model, complexity, token_count
        );

        RoutingDecision {
            complexity,
            tier,
            provider,
            model,
        }
    }

    /// Determines if a failed Tier 1 attempt should be escalated to Tier 2 based on error category.
    ///
    /// Escalates on model-capability failures (JSON parsing, invalid tool signatures, recursion),
    /// while avoiding useless escalation on auth (401/403) or provider-level network failures.
    pub fn should_escalate_after_failure(&self, attempt: usize, error_detail: &str) -> Option<(String, String)> {
        if attempt < 1 {
            return None;
        }

        let err_lower = error_detail.to_ascii_lowercase();

        // Non-escalatable errors (shared API key issues, account balance exhaustion)
        if err_lower.contains("401") || err_lower.contains("unauthorized") || err_lower.contains("invalid api key") {
            warn!("⚠️ [CascadeRouter] Failure is authentication-related. Skipping model escalation.");
            return None;
        }

        // Capability errors: Escalation to frontier model will resolve syntax/format issues
        let is_capability_error = err_lower.contains("json")
            || err_lower.contains("parse")
            || err_lower.contains("schema")
            || err_lower.contains("tool")
            || err_lower.contains("format")
            || err_lower.contains("timeout")
            || err_lower.contains("recursion");

        if is_capability_error || attempt >= 2 {
            warn!(
                "⚠️ [CascadeRouter] Tier 1 failure ('{}') identified as capability/format error. Escalating to Tier 2 Reasoning model...",
                error_detail
            );
            Some((self.policy.tier2_provider.clone(), self.policy.tier2_model.clone()))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cascade_routes_simple_task_to_tier1_decision() {
        let router = CascadeRouter::default();
        let decision = router.route_turn(
            "List files in docs/",
            150,
            false,
            false,
        );

        assert_eq!(decision.complexity, TaskComplexity::Low);
        assert_eq!(decision.tier, ModelTier::Tier1Fast);
        assert_eq!(decision.provider, "gemini");
        assert_eq!(decision.model, "gemini-2.0-flash");
    }

    #[test]
    fn test_cascade_escalates_mutating_and_critical_keywords() {
        let router = CascadeRouter::default();
        
        // Mutating turn -> High -> Tier 2
        let dec_mut = router.route_turn(
            "Write migration file to server-rs",
            300,
            true,
            false,
        );
        assert_eq!(dec_mut.complexity, TaskComplexity::High);
        assert_eq!(dec_mut.tier, ModelTier::Tier2Reasoning);
        assert_eq!(dec_mut.model, "gemini-2.0-pro");

        // Critical keyword in prompt
        let dec_crit = router.route_turn(
            "Execute SWARM_SYNTHESIS for cluster deployment",
            200,
            false,
            false,
        );
        assert_eq!(dec_crit.complexity, TaskComplexity::Critical);
        assert_eq!(dec_crit.tier, ModelTier::Tier2Reasoning);
    }

    #[test]
    fn test_cascade_smart_escalation_filters_auth_errors() {
        let router = CascadeRouter::default();

        // JSON parse error -> Escalates to Tier 2
        let json_esc = router.should_escalate_after_failure(1, "Failed to parse tool call JSON");
        assert!(json_esc.is_some());
        let (prov, model) = json_esc.unwrap();
        assert_eq!(prov, "gemini");
        assert_eq!(model, "gemini-2.0-pro");

        // 401 Unauthorized -> Does NOT escalate
        let auth_esc = router.should_escalate_after_failure(1, "HTTP 401 Unauthorized: Invalid API Key");
        assert!(auth_esc.is_none());
    }
}

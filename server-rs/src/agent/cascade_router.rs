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

use crate::agent::script_profiler::{profile_text, ScriptProfile};

/// Evaluated complexity of an incoming agent turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskComplexity {
    Low,
    Medium,
    High,
    Critical,
}

/// Optional System 1 classification hint evaluated in a single forward pass.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SystemOneRoutingHint {
    pub choice: String,
    pub confidence: f32,
    pub domain: Option<String>,
}

/// Structured routing decision returned by the cascade router.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoutingDecision {
    pub complexity: TaskComplexity,
    pub tier: ModelTier,
    pub provider: String,
    pub model: String,
    pub confidence: f32,
    pub script_profile: ScriptProfile,
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
    pub confidence_escalation_threshold: f32,
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
            confidence_escalation_threshold: 0.85,
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

    /// Evaluates task characteristics with optional System 1 semantic hint and returns a `RoutingDecision`.
    pub fn route_turn_semantic(
        &self,
        prompt: &str,
        token_count: usize,
        is_mutating: bool,
        directive_requires_deep_reasoning: bool,
        hint: Option<&SystemOneRoutingHint>,
    ) -> RoutingDecision {
        let script_profile = profile_text(prompt);

        // 1. Evaluate Complexity incorporating System 1 Semantic Hint
        let (complexity, confidence) = if let Some(h) = hint {
            let conf = h.confidence;
            if h.choice == "tier2_reasoning" || conf < self.policy.confidence_escalation_threshold {
                // Ambiguous confidence or explicit Tier 2 classification -> Escalate
                (TaskComplexity::Critical, conf)
            } else if is_mutating && self.policy.auto_escalate_on_mutation {
                (TaskComplexity::High, conf)
            } else if token_count > self.policy.max_tier1_token_threshold {
                (TaskComplexity::Medium, conf)
            } else {
                (TaskComplexity::Low, conf)
            }
        } else {
            // Heuristic fallback path
            let base_conf = 1.0f32;
            let comp = if directive_requires_deep_reasoning
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
            (comp, base_conf)
        };

        // 2. Map Complexity to Model Tier
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
            "🧭 [CascadeRouter] Turn routed to {:?} ({}/{}) [Complexity: {:?}, Script: {}, Conf: {:.2}, Tokens: {}]",
            tier, provider, model, complexity, script_profile.dominant_script, confidence, token_count
        );

        RoutingDecision {
            complexity,
            tier,
            provider,
            model,
            confidence,
            script_profile,
        }
    }

    /// Evaluates task characteristics using standard heuristic parameters (backwards compatible).
    pub fn route_turn(
        &self,
        prompt: &str,
        token_count: usize,
        is_mutating: bool,
        directive_requires_deep_reasoning: bool,
    ) -> RoutingDecision {
        self.route_turn_semantic(prompt, token_count, is_mutating, directive_requires_deep_reasoning, None)
    }

    /// Detects orchestrator stall signals using typed matching rather than raw regexes.
    pub fn detect_stall_pattern(output: &str) -> Option<&'static str> {
        let lower = output.to_ascii_lowercase();
        if lower.contains("<halting_signal/>") {
            Some("explicit_halt_signal")
        } else if lower.contains("waiting for user input")
            || lower.contains("please provide")
            || lower.contains("ready to proceed")
            || lower.contains("what would you like")
            || lower.contains("provide the specific")
            || lower.contains("waiting for")
            || lower.contains("need more information")
            || lower.contains("please share")
            || lower.contains("let me know")
        {
            Some("waiting_for_input")
        } else if lower.contains("unable to proceed without") {
            Some("missing_dependency")
        } else {
            None
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

    #[test]
    fn test_cascade_token_threshold_escalation() {
        let router = CascadeRouter::default();
        // Over 12,000 prompt tokens -> Medium complexity -> Tier 1 Fast (large context)
        let decision = router.route_turn(
            "Summarize large code context",
            15_000,
            false,
            false,
        );
        assert_eq!(decision.complexity, TaskComplexity::Medium);
        assert_eq!(decision.tier, ModelTier::Tier1Fast);
        assert_eq!(decision.model, "gemini-2.0-flash");
    }

    #[test]
    fn test_cascade_retry_count_escalation() {
        let router = CascadeRouter::default();
        // Generic error on attempt 2+ -> Escalates to Tier 2
        let retry_esc = router.should_escalate_after_failure(2, "Unknown transient failure");
        assert!(retry_esc.is_some());
        let (prov, model) = retry_esc.unwrap();
        assert_eq!(prov, "gemini");
        assert_eq!(model, "gemini-2.0-pro");
    }

    #[test]
    fn test_cascade_rate_limit_and_quota_non_escalation() {
        let router = CascadeRouter::default();
        let quota_esc = router.should_escalate_after_failure(1, "HTTP 429: Resource has been exhausted (e.g. check quota)");
        assert!(quota_esc.is_none(), "Quota/rate limit errors should not trigger model escalation");

        let credit_esc = router.should_escalate_after_failure(1, "Insufficient credit balance in account");
        assert!(credit_esc.is_none(), "Credit errors should not trigger model escalation");
    }

    #[test]
    fn test_cascade_semantic_routing_and_confidence_escalation() {
        let router = CascadeRouter::default();

        // 1. Semantic hint with high confidence -> routes to Tier 1
        let hint_fast = SystemOneRoutingHint {
            choice: "tier1_fast".to_string(),
            confidence: 0.94,
            domain: Some("coding".to_string()),
        };
        let decision_fast = router.route_turn_semantic("git status", 50, false, false, Some(&hint_fast));
        assert_eq!(decision_fast.tier, ModelTier::Tier1Fast);
        assert_eq!(decision_fast.confidence, 0.94);

        // 2. Semantic hint with low confidence (< 0.85) -> auto-escalates to Tier 2
        let hint_ambiguous = SystemOneRoutingHint {
            choice: "tier1_fast".to_string(),
            confidence: 0.72,
            domain: Some("refactor".to_string()),
        };
        let decision_esc = router.route_turn_semantic("reorganize this function", 120, false, false, Some(&hint_ambiguous));
        assert_eq!(decision_esc.tier, ModelTier::Tier2Reasoning);
        assert_eq!(decision_esc.complexity, TaskComplexity::Critical);
    }

    #[test]
    fn test_cascade_detects_stall_patterns() {
        assert_eq!(CascadeRouter::detect_stall_pattern("<halting_signal/>"), Some("explicit_halt_signal"));
        assert_eq!(CascadeRouter::detect_stall_pattern("Please provide the database password"), Some("waiting_for_input"));
        assert_eq!(CascadeRouter::detect_stall_pattern("Normal code response"), None);
    }
}

// Metadata: [cascade_router]

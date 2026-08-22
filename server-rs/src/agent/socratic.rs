//! @docs ARCHITECTURE:Agent
//! @docs OPERATIONS_MANUAL:SwarmManagement
//!
//! ### AI Assist Note
//! **Socratic Context Auto-Injection**: Deterministic compilation and injection of the
//! 4-Pillar Socratic Context Envelope (Scope, Performance Threshold, Architecture Mode, Pre-Cleared Failure Policies)
//! to enable 0-turn Socratic Gate passes in autonomous multi-agent swarms.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Malformed envelope payload or missing contract fields.
//! - **Telemetry Link**: Search `[socratic]` in tracing logs.
//! - **Trace Scope**: `server-rs::agent::socratic`

use serde::{Deserialize, Serialize};
use std::borrow::Cow;

pub const ENVELOPE_VERSION: &str = "1.0";
pub const STATUS_PRE_CLEARED: &str = "PRE_CLEARED_GATE_PASS";

/// Strongly-typed Blast Radius level with explicit descriptive scope.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum BlastRadiusLevel {
    /// Level 1: Read-Only operations, zero filesystem or database mutation.
    Level1ReadOnly,
    /// Level 2: Workspace-local mutations permitted (source files, artifacts).
    Level2WorkspaceLocal,
    /// Level 3: System-wide changes, database schema DDL, or external network operations.
    Level3SystemWide,
}

impl BlastRadiusLevel {
    pub fn as_u8(&self) -> u8 {
        match self {
            Self::Level1ReadOnly => 1,
            Self::Level2WorkspaceLocal => 2,
            Self::Level3SystemWide => 3,
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Self::Level1ReadOnly => "Level 1 (Read-Only / Zero Mutation)",
            Self::Level2WorkspaceLocal => "Level 2 (Workspace-Local Mutation)",
            Self::Level3SystemWide => "Level 3 (System-Wide / High Blast Radius)",
        }
    }
}

impl Default for BlastRadiusLevel {
    fn default() -> Self {
        Self::Level2WorkspaceLocal
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ScopeContract {
    pub target_paths: Vec<String>,
    pub blast_radius_level: u8,
    pub blast_radius_description: String,
    pub mutation_allowed: bool,
    pub database_mutation_allowed: bool,
    pub mission_vector: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PerformanceThreshold {
    pub budget_usd_cap: f64,
    pub budget_cents_cap: u64,
    pub max_swarm_depth: u32,
    pub target_turn_latency_ms: u32,
    pub active_model_slot: u8,
    pub timeout_seconds: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ArchitectureMode {
    pub mode_name: String,
    pub privacy_shield_enforced: bool,
    pub standards_compliance: Vec<String>,
    pub assigned_persona: String,
}

/// Profile classification to dynamically assign failure policies.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum PolicyProfile {
    Standard,
    AuditSecurity,
    LocalAirGap,
}

/// Configurable defaults for Socratic Envelope compilation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SocraticDefaults {
    pub default_paths: Vec<String>,
    pub default_failure_modes: Vec<String>,
    pub audit_failure_modes: Vec<String>,
    pub airgap_failure_modes: Vec<String>,
    pub default_compliance: Vec<String>,
}

impl Default for SocraticDefaults {
    fn default() -> Self {
        Self {
            default_paths: vec!["src/".to_string(), "server-rs/src/".to_string()],
            default_failure_modes: vec![
                "Circuit Breaker: Halt after 3 non-convergent iterations as Logic-Blocker (Directive #3).".to_string(),
                "Air-Gap Shield: Local Ollama fallback strictly enforced if cloud providers blocked (PRIVACY_MODE).".to_string(),
                "Tool Execution Rule: Check execution/ for existing tools before writing ad-hoc scripts (Layer 1/2/3).".to_string(),
                "Verification Gate: Require automated validation pass (parity_guard / vitest) before state commit.".to_string(),
            ],
            audit_failure_modes: vec![
                "Zero Trust Gate: Enforce parameter isolation and local sandbox boundary on all tool invocations.".to_string(),
                "Circuit Breaker: Halt after 3 non-convergent iterations as Logic-Blocker (Directive #3).".to_string(),
                "Verification Gate: Full parity guard and AI context validation mandatory before state commit.".to_string(),
                "Secret Redaction: Redact sensitive tokens and environment variables from telemetry streams.".to_string(),
            ],
            airgap_failure_modes: vec![
                "Air-Gap Mandate: 100% local model routing; all outbound cloud requests prohibited.".to_string(),
                "Circuit Breaker: Halt after 3 non-convergent iterations as Logic-Blocker (Directive #3).".to_string(),
                "Local Inference Resilience: Allocate 300s TTL for complex reasoning tasks on local hardware.".to_string(),
            ],
            default_compliance: vec![
                "Zero Trust".to_string(),
                "L1/L2/L3 agentskills.io".to_string(),
                "DESIGN.md".to_string(),
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SocraticContextEnvelope {
    pub envelope_version: String,
    pub status: String,
    pub target_agent_id: String,
    pub target_agent_name: String,
    pub target_agent_role: String,
    pub scope_contract: ScopeContract,
    pub performance_threshold: PerformanceThreshold,
    pub architecture_mode: ArchitectureMode,
    pub pre_cleared_failure_modes: Vec<String>,
}

impl SocraticContextEnvelope {
    /// Compiles a Socratic Context Envelope with custom configuration defaults and profile selection.
    pub fn compile_with_config(
        agent_id: &str,
        agent_name: &str,
        agent_role: &str,
        mission_vector: &str,
        target_paths: Option<Vec<String>>,
        blast_level: BlastRadiusLevel,
        budget_usd_cap: Option<f64>,
        active_slot: Option<u8>,
        is_privacy_mode: bool,
        config: &SocraticDefaults,
    ) -> Self {
        let slot = active_slot.unwrap_or(2);
        let latency_target = if slot == 2 { 1500 } else { 10000 };
        let paths = target_paths.unwrap_or_else(|| config.default_paths.clone());

        let role_lower = agent_role.to_lowercase();
        let vector_lower = mission_vector.to_lowercase();
        let is_security_or_audit = vector_lower.contains("audit")
            || vector_lower.contains("security")
            || role_lower.contains("security")
            || role_lower.contains("auditor")
            || role_lower.contains("qa");

        let (mode_name, failure_modes) = if is_security_or_audit {
            (
                "Nexus Engineer Mode (Zero Trust Auditor + Principal QA)".to_string(),
                config.audit_failure_modes.clone(),
            )
        } else if is_privacy_mode {
            (
                "Sovereign Air-Gap Mode (100% Local Privacy Shield)".to_string(),
                config.airgap_failure_modes.clone(),
            )
        } else {
            (
                "Sovereign 3-Layer Architecture (Directives -> Orchestration -> Execution)".to_string(),
                config.default_failure_modes.clone(),
            )
        };

        let budget_usd = budget_usd_cap.unwrap_or(1.0);
        let budget_cents = (budget_usd * 100.0).round().max(0.0) as u64;

        Self {
            envelope_version: ENVELOPE_VERSION.to_string(),
            status: STATUS_PRE_CLEARED.to_string(),
            target_agent_id: agent_id.to_string(),
            target_agent_name: agent_name.to_string(),
            target_agent_role: agent_role.to_string(),
            scope_contract: ScopeContract {
                target_paths: paths,
                blast_radius_level: blast_level.as_u8(),
                blast_radius_description: blast_level.description().to_string(),
                mutation_allowed: blast_level != BlastRadiusLevel::Level1ReadOnly,
                database_mutation_allowed: blast_level == BlastRadiusLevel::Level3SystemWide,
                mission_vector: mission_vector.to_string(),
            },
            performance_threshold: PerformanceThreshold {
                budget_usd_cap: budget_usd,
                budget_cents_cap: budget_cents,
                max_swarm_depth: 5,
                target_turn_latency_ms: latency_target,
                active_model_slot: slot,
                timeout_seconds: if is_privacy_mode { 300 } else { 120 },
            },
            architecture_mode: ArchitectureMode {
                mode_name,
                privacy_shield_enforced: is_privacy_mode,
                standards_compliance: config.default_compliance.clone(),
                assigned_persona: agent_role.to_string(),
            },
            pre_cleared_failure_modes: failure_modes,
        }
    }

    /// Compiles a standard deterministic Socratic Context Envelope using global system defaults.
    pub fn compile(
        agent_id: &str,
        agent_name: &str,
        agent_role: &str,
        mission_vector: &str,
        target_paths: Option<Vec<String>>,
        budget_cap: Option<f64>,
        active_slot: Option<u8>,
        is_privacy_mode: bool,
    ) -> Self {
        let defaults = SocraticDefaults::default();
        Self::compile_with_config(
            agent_id,
            agent_name,
            agent_role,
            mission_vector,
            target_paths,
            BlastRadiusLevel::Level2WorkspaceLocal,
            budget_cap,
            active_slot,
            is_privacy_mode,
            &defaults,
        )
    }

    /// Formats the Socratic Context Envelope as markdown for prompt pre-injection.
    pub fn to_markdown(&self) -> String {
        let paths = self.scope_contract.target_paths.join(", ");
        let mut md = String::with_capacity(1024);
        md.push_str("<!-- SOCRATIC_GATE_ENVELOPE: PRE-CLEARED -->\n");
        md.push_str("### 🛡️ Pre-Injected Socratic Context Contract (Zero-Stall Gate Pass)\n");
        md.push_str(&format!(
            "*Target Node: `{}` (`{}`) | Mission: {}*\n\n",
            self.target_agent_name, self.target_agent_role, self.scope_contract.mission_vector
        ));
        md.push_str(&format!(
            "1. 🎯 **[SCOPE_CONTRACT]**\n\
               - **Target Paths**: `{}`\n\
               - **Blast Radius**: `{}` (Mutations: {})\n\
               - **Database Mutation**: {}\n\n",
            paths,
            self.scope_contract.blast_radius_description,
            if self.scope_contract.mutation_allowed { "Allowed" } else { "Read-Only" },
            if self.scope_contract.database_mutation_allowed { "Allowed" } else { "Blocked (Read-Only Registry)" },
        ));
        md.push_str(&format!(
            "2. ⚡ **[PERFORMANCE_THRESHOLD]**\n\
               - **Fiscal Budget Cap**: `${:.2} USD` ({}¢)\n\
               - **Max Swarm Depth**: `Depth <= {}` | **Turn Latency Target**: `< {}ms` (Slot {})\n\
               - **Execution Timeout**: `{}s`\n\n",
            self.performance_threshold.budget_usd_cap,
            self.performance_threshold.budget_cents_cap,
            self.performance_threshold.max_swarm_depth,
            self.performance_threshold.target_turn_latency_ms,
            self.performance_threshold.active_model_slot,
            self.performance_threshold.timeout_seconds,
        ));
        md.push_str(&format!(
            "3. 🏛️ **[ARCHITECTURE_MODE]**\n\
               - **Active Mode**: `{}`\n\
               - **Privacy Shield**: `{}`\n\
               - **Governance Compliance**: {}\n\n",
            self.architecture_mode.mode_name,
            if self.architecture_mode.privacy_shield_enforced { "ENFORCED (100% Local Air-Gap)" } else { "Standard" },
            self.architecture_mode.standards_compliance.join(", "),
        ));
        md.push_str("4. ⚖️ **[PRE-CLEARED FAILURE POLICIES & TRADE-OFFS]**\n");
        for policy in &self.pre_cleared_failure_modes {
            md.push_str(&format!("   - {}\n", policy));
        }
        md.push_str("<!-- /SOCRATIC_GATE_ENVELOPE -->\n\n");
        md
    }

    /// Auto-injects the Socratic markdown header into the initial prompt text if not already present.
    pub fn inject_into_prompt<'a>(&self, prompt: &'a str) -> Cow<'a, str> {
        if prompt.contains("SOCRATIC_GATE_ENVELOPE") {
            Cow::Borrowed(prompt)
        } else {
            Cow::Owned(format!("{}{}", self.to_markdown(), prompt))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_socratic_envelope_compilation() {
        let envelope = SocraticContextEnvelope::compile(
            "99",
            "Agent 99 (QA-99)",
            "quality-auditor",
            "System Audit",
            Some(vec!["src/".to_string()]),
            Some(1.50),
            Some(2),
            true,
        );

        assert_eq!(envelope.target_agent_id, "99");
        assert_eq!(envelope.performance_threshold.active_model_slot, 2);
        assert_eq!(envelope.performance_threshold.target_turn_latency_ms, 1500);
        assert_eq!(envelope.performance_threshold.budget_cents_cap, 150);
        assert!(envelope.architecture_mode.privacy_shield_enforced);
        assert!(envelope.architecture_mode.mode_name.contains("Nexus Engineer Mode"));
        assert_eq!(envelope.scope_contract.blast_radius_level, 2);
        assert!(envelope.scope_contract.blast_radius_description.contains("Workspace-Local"));

        let md = envelope.to_markdown();
        assert!(md.contains("SOCRATIC_GATE_ENVELOPE: PRE-CLEARED"));
        assert!(md.contains("Target Node: `Agent 99 (QA-99)`"));
        assert!(md.contains("Turn Latency Target"));
        assert!(md.contains("1500ms"));
        assert!(md.contains("150¢"));
    }

    #[test]
    fn test_socratic_envelope_injection() {
        let envelope = SocraticContextEnvelope::compile(
            "system_architect",
            "System Architect",
            "architect",
            "Feature Blueprint",
            None,
            None,
            None,
            false,
        );

        let base_prompt = "Build the new telemetry widget.";
        let injected = envelope.inject_into_prompt(base_prompt);

        assert!(injected.starts_with("<!-- SOCRATIC_GATE_ENVELOPE: PRE-CLEARED -->"));
        assert!(injected.ends_with("Build the new telemetry widget."));

        // Idempotence: do not inject twice
        let injected_twice = envelope.inject_into_prompt(&injected);
        assert_eq!(injected, injected_twice);
    }

    #[test]
    fn test_socratic_blast_radius_levels() {
        let defaults = SocraticDefaults::default();
        let read_only_envelope = SocraticContextEnvelope::compile_with_config(
            "scout",
            "Explorer Scout",
            "researcher",
            "Code Recon",
            None,
            BlastRadiusLevel::Level1ReadOnly,
            Some(0.50),
            Some(2),
            false,
            &defaults,
        );

        assert_eq!(read_only_envelope.scope_contract.blast_radius_level, 1);
        assert!(!read_only_envelope.scope_contract.mutation_allowed);
        assert!(!read_only_envelope.scope_contract.database_mutation_allowed);
        assert!(read_only_envelope.scope_contract.blast_radius_description.contains("Read-Only"));
    }
}

// Metadata: [socratic]

/*
@docs ARCHITECTURE:SovereignKernel

### AI Assist Note
**🛡️ Tadpole OS: Tier**
Core system module providing specialized functionality for the agent swarm.

### 🔍 Debugging & Observability
- **Failure Path**: Unexpected execution drift or type compatibility issues.
- **Telemetry Link**: Traced via active system logging channels.
*/

//! Capability tier definitions and precedence rules.

/// Defines the provenance tier of a capability in the sovereign runtime.
/// Precedence order: User < Agent < BuiltIn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Tier {
    User = 0,
    Agent = 1,
    BuiltIn = 2,
}

impl Tier {
    /// Ascending order for precedence resolution (lowest -> highest).
    pub const ASCENDING: [Tier; 3] = [Tier::User, Tier::Agent, Tier::BuiltIn];

    /// Returns the canonical category string for this tier.
    pub fn category(&self) -> &'static str {
        match self {
            Tier::User => "user",
            Tier::Agent => "ai",
            Tier::BuiltIn => "built_in",
        }
    }

    /// Determines if capabilities loaded or saved in this tier MUST have oversight enforced.
    /// In Sovereign AI safety, all Agent-tier (AI-generated) capabilities require oversight unconditionally.
    pub fn enforce_oversight(&self) -> bool {
        matches!(self, Tier::Agent)
    }

    /// Determines if capabilities in this tier are immutable (protected from deletion via API).
    pub fn is_immutable(&self) -> bool {
        matches!(self, Tier::BuiltIn)
    }
}

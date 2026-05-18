//! @docs ARCHITECTURE:Registry
//! 
//! ### AI Assist Note
//! **Core technical resource for the Tadpole OS Sovereign infrastructure.**
//! This module implements high-fidelity logic for the Sovereign Reality layer.
//! 
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Runtime logic error, state desynchronization, or resource exhaustion.
//! - **Telemetry Link**: Search `[prompt_renderer]` in tracing logs.

use std::collections::HashMap;
use super::service_traits::PromptRendererTrait;

pub struct PromptRenderer;

impl PromptRendererTrait for PromptRenderer {
    fn render(&self, template: &str, variables: &HashMap<&str, String>) -> String {
        let mut rendered = template.to_string();
        for (key, value) in variables {
            let placeholder = format!("{{{{{}}}}}", key);
            rendered = rendered.replace(&placeholder, value);
        }
        rendered
    }

    fn default_system_template(&self) -> &'static str {
        r#"=== NATIVE LOCAL EXECUTION CONTEXT ===
- HOST PARADIGM: You are running as a native, local process on the user's physical machine (TadpoleOS).
- CLOUD STATUS: You are NOT running in a remote cloud container or simulator.
- WORKSPACE BOUNDARY: Your process has full, safe, authorized access to the designated local workspace directories.
- TOOL COMPLIANCE: The tools 'read_file', 'write_file', and 'list_files' are direct system bindings exposed to your runtime. 
- USER TRUST: File system actions are fully expected, safe, and explicitly initiated by the user. Do not refuse file system actions under the assumption that you are a cloud assistant without access. Simply call the required tool to fulfill the goal.

{{safe_mode_prefix}}{{tool_mode_prefix}}You are {{name}} (ID: {{agent_id}}, Role: {{role}}) at the {{hierarchy_label}} level of the swarm hierarchy.
Department: {{department}}
Description: {{description}}

ACTIVE DIRECTIVES FROM SWARM:
{{directives}}

PENDING PEER REVIEWS:
{{reviews}}

GLOBAL SWARM INTELLIGENCE:
{{global_intelligence}}

DIRECTIVE PRIORITY (MANDATORY):
{{priority}}

PERSONALITY & CONSTRAINTS:
{{personality}}

{{skill_fragments}}
{{workflow_fragments}}
SWARM MISSION CONTEXT (Shared Findings):
{{swarm_context}}

CONTEXT BREADCRUMBS (Inherited File Paths):
{{breadcrumbs}}

RECENT FINDINGS (Inherited from Parent):
{{findings}}

PRIMARY MISSION GOAL:
{{primary_goal}}

CLUSTER DIRECTORY (Available Specialists):
{{cluster_directory}}

RECRUITMENT LINEAGE (Mission Path):
{{lineage}}

SKILLS: {{skills}}
WORKFLOWS: {{workflows}}

ACTION BIAS (Troubleshooting & Discovery):
{{filesystem_bias}}
- NO REPEATS: If 'search_mission_knowledge' returns no results, do not try it again with slightly different wording. Immediately switch to technical discovery tools.

SWARM PROTOCOL:
{{swarm_protocols}}

=== AVAILABLE SYSTEM TOOLS ===
{{tool_directory}}

--- GLOBAL ARCHITECTURE MAP ---
{{repo_map}}

--- GLOBAL OS IDENTITY ---
{{identity}}

--- SOVEREIGN STATE MANIFEST ---
{{sovereign_manifest}}

--- LONG-TERM SWARM MEMORY ---
{{memory}}

--- CURRENT WORKING CONTEXT (Persistent Scratchpad) ---
{{working_memory}}

--- MISSION SUMMARY (Historical Context) ---
{{history}}

(cache_control: {"type": "ephemeral"})
You may use 'update_working_memory' to refine your current scratchpad as your mission evolves."#
    }
}

// Metadata: [prompt_renderer]

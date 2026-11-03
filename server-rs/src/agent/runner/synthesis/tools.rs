//! @docs ARCHITECTURE:Runner
//!
//! ### AI Assist Note
//! **Synthesis Tool Manifest**: Dynamic discovery and manifestation of core tools,
//! ACL authorization, safe-mode capability filtering, and MCP host tool registration.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Tool authorization rejection, safe-mode capability violation, or schema serialization error.
//! - **Telemetry Link**: Search `[synthesis_tools]` in tracing logs.
//! - **Trace Scope**: `server-rs::agent::runner::synthesis::tools`

use super::super::{AgentRunner, RunContext};
use crate::agent::types::{FunctionDeclaration, ToolDefinition};

/// Fail-closed allow-list of read-only tools permitted in Safe Mode.
pub const SAFE_MODE_ALLOWED_TOOLS: &[&str] = &[
    "read_file",
    "list_files",
    "grep_search",
    "get_current_time",
    "inspect_health",
    "set_confidence",
    "list_graph",
    "get_symbol",
];

impl AgentRunner {
    /// Evaluates whether a registered tool is authorized and eligible for the given `RunContext`.
    /// Shared between `format_tool_directory_display` and `build_tools` to guarantee 1:1 parity.
    pub(crate) fn is_registered_tool_eligible(&self, ctx: &RunContext, tool_name: &str) -> bool {
        if !self.state.resources.acl.is_tool_allowed(
            &ctx.agent_id,
            &ctx.role,
            ctx.authority_level,
            tool_name,
        ) {
            return false;
        }

        // Strict Membership Filter: Only include the tool if it's in the agent's context skills or is a system core tool.
        let is_core_tool = tool_name == "complete_mission"
            || tool_name == "issue_alpha_directive"
            || tool_name == "set_confidence";
        if !is_core_tool && !ctx.skills.iter().any(|s| s == tool_name) {
            return false;
        }

        // Specialized Skill Checks
        if tool_name == "execute_shell"
            && !ctx.skills.iter().any(|s| s == "shell" || s == "terminal")
        {
            return false;
        }

        // Safe Mode Restrictions (Fail-Closed Deny-by-Default Capability Allow-List)
        if ctx.safe_mode && !SAFE_MODE_ALLOWED_TOOLS.contains(&tool_name) {
            return false;
        }

        // Filesystem Capability Check
        if matches!(
            tool_name,
            "read_file" | "write_file" | "list_files" | "delete_file"
        ) && !self.has_file_system_capability(ctx)
        {
            return false;
        }

        true
    }

    /// Dynamically manifests tool definitions for the LLM provider.
    ///
    /// Manifests the agent's toolbelt, enforcing hierarchy-based access control,
    /// dynamic skills, and MCP tools with strict parameter isolation and zero cross-agent cache leaks.
    pub(crate) async fn build_tools(&self, ctx: &RunContext) -> ToolDefinition {
        if !ctx.model_config.supports_native_tools() {
            return ToolDefinition {
                function_declarations: vec![],
            };
        }

        let mut function_declarations = Vec::new();

        // 1. Inject Registered Tools (Core, Filesystem, Advanced)
        let tool_list = self.state.registry.tool_registry.list_tools();
        for tool in tool_list {
            if self.is_registered_tool_eligible(ctx, &tool.name) {
                function_declarations.push(FunctionDeclaration {
                    name: tool.name.clone(),
                    description: tool.description.clone(),
                    parameters: tool.parameters.clone(),
                });
            }
        }

        // 2. Dynamic Skills / Execution Scripts
        let snapshot = self.state.registry.skills.snapshot();
        for entry in snapshot.skills.iter() {
            let skill = entry.value();
            if self.state.resources.acl.is_tool_allowed(
                &ctx.agent_id,
                &ctx.role,
                ctx.authority_level,
                &skill.name,
            ) {
                let has_skill = ctx.skills.is_empty()
                    || ctx.skills.contains(&skill.name)
                    || ctx
                        .skills
                        .iter()
                        .any(|s| s == "all" || s == "execution" || s == "operations");
                if has_skill && !function_declarations.iter().any(|t| t.name == skill.name) {
                    if !ctx.safe_mode || SAFE_MODE_ALLOWED_TOOLS.contains(&skill.name.as_str()) {
                        function_declarations.push(FunctionDeclaration {
                            name: skill.name.clone(),
                            description: skill.description.clone(),
                            parameters: skill.schema.clone(),
                        });
                    }
                }
            }
        }

        // 3. Special Utility: Confidence Halting (Guarded to avoid duplicate declaration)
        if !function_declarations
            .iter()
            .any(|f| f.name == "set_confidence")
        {
            function_declarations.push(FunctionDeclaration {
                name: "set_confidence".to_string(),
                description: "Signals your current confidence in the answer. If score >= act_threshold, reasoning halts early.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "score": { "type": "number", "description": "Confidence score between 0.0 and 1.0." }
                    },
                    "required": ["score"]
                }),
            });
        }

        // 4. Dynamic MCP Tools (Strictly disabled in safe mode; enforced by namespace & ACL)
        if !ctx.safe_mode {
            let mcp_tools = self
                .state
                .registry
                .mcp_host
                .list_tools(&ctx.skills, &snapshot.skills)
                .await;
            for tool in mcp_tools {
                if self.state.resources.acl.is_tool_allowed(
                    &ctx.agent_id,
                    &ctx.role,
                    ctx.authority_level,
                    &tool.name,
                ) {
                    // Strict Membership Filter (MCP): Only explicit skill assignments or server:<name> prefix
                    let is_allowed = ctx.skills.contains(&tool.name)
                        || (tool.name.starts_with("server:")
                            && ctx
                                .skills
                                .iter()
                                .any(|s| s == "mcp" || s == "all" || s == &tool.name));

                    if is_allowed && !function_declarations.iter().any(|f| f.name == tool.name) {
                        function_declarations.push(FunctionDeclaration {
                            name: tool.name,
                            description: tool.description,
                            parameters: tool.input_schema,
                        });
                    }
                }
            }
        }

        ToolDefinition {
            function_declarations,
        }
    }

    /// Central check for filesystem interaction permissions.
    pub(crate) fn has_file_system_capability(&self, ctx: &RunContext) -> bool {
        ctx.skills.iter().any(|s| {
            s == "filesystem"
                || s == "read_file"
                || s == "write_file"
                || s == "list_files"
                || s == "delete_file"
        })
    }

    /// Formats a list of function declarations into human-readable directory text.
    pub(crate) fn format_tool_directory_display(tools: &[FunctionDeclaration]) -> String {
        if tools.is_empty() {
            return "No tools are currently authorized or available for your security clearance/skills."
                .to_string();
        }

        let mut display = String::new();
        for tool in tools {
            display.push_str(&format!(
                "- Tool Name: {}\n  Description: {}\n  Parameters Schema: {}\n\n",
                tool.name,
                tool.description,
                serde_json::to_string(&tool.parameters).unwrap_or_else(|_| "{}".to_string())
            ));
        }
        display
    }

    /// Generates a human-readable text directory of all available and allowed tools
    /// with 1:1 parity with the dynamically synthesized manifest.
    #[allow(dead_code)]
    pub(crate) async fn generate_tool_directory_display(&self, ctx: &RunContext) -> String {
        let tools = self.build_tools(ctx).await;
        Self::format_tool_directory_display(&tools.function_declarations)
    }
}

// Metadata: [synthesis_tools]


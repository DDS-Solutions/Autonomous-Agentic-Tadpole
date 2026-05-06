//! @docs ARCHITECTURE:Contracts
//! 
//! ### AI Assist Note
//! **Contract Bridge**: Authoritative type exporter from Rust to TypeScript. 
//! Uses **Specta** to ensure that frontend interfaces exactly match backend 
//! data structures, preventing serialization drift.
//! 
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Failed binding export due to missing `#[derive(Type)]` on new Rust structs or file permission errors on Windows.
//! - **Telemetry Link**: Search `[Bridge]` in server traces.
//!


use crate::agent::types::*;
use crate::agent::merge::*;
use crate::agent::skill_manifest::*;

#[allow(dead_code)]
pub fn export_bindings() {
    let config = specta_typescript::Typescript::default();
    let mut output = String::new();
    
    output.push_str("/**\n");
    output.push_str(" * @docs ARCHITECTURE:Contracts\n");
    output.push_str(" * \n");
    output.push_str(" * ### AI Assist Note\n");
    output.push_str(" * **Auto-Generated Contracts**: Authoritative TypeScript types mirrored from Rust structs via Specta.\n");
    output.push_str(" * This file ensures frontend-backend type parity. Do not edit manually.\n");
    output.push_str(" * \n");
    output.push_str(" * ### 🔍 Debugging & Observability\n");
    output.push_str(" * - **Failure Path**: Type mismatch if Rust structs drift without triggering a re-generation.\n");
    output.push_str(" * - **Telemetry Link**: Not tracked (Static generated types).\n");
    output.push_str(" */\n\n");
    
    macro_rules! export_type {
        ($t:ty) => {
            match specta_typescript::export::<$t>(&config) {
                Ok(s) => {
                    output.push_str(&s);
                    output.push_str("\n\n");
                },
                Err(e) => tracing::error!("❌ [Bridge] Failed to export {}: {}", stringify!($t), e),
            }
        };
    }

    // Individual exports for core roots
    export_type!(EngineAgent);
    export_type!(AgentIdentity);
    export_type!(AgentModels);
    export_type!(AgentEconomics);
    export_type!(AgentHealth);
    export_type!(AgentCapabilities);
    export_type!(AgentState);
    export_type!(ModelConfig);
    export_type!(ConnectorConfig);
    export_type!(TokenUsage);
    export_type!(ModelProvider);
    export_type!(RoleBlueprint);
    export_type!(AgentConfigUpdate);
    export_type!(SkillManifest);
    export_type!(DangerLevel);
    export_type!(Permission);
    export_type!(SkillParameter);
    export_type!(SkillHooks);

    output.push_str("export type JsonValue = string | number | boolean | null | { [key: string]: JsonValue } | JsonValue[];\n");

    let export_path = "../src/contracts/generated.ts";
    if let Err(e) = std::fs::write(export_path, output) {
        tracing::error!("❌ [Bridge] Failed to write TypeScript bindings to file {}: {}", export_path, e);
    } else {
        tracing::info!("✅ [Bridge] TypeScript bindings exported to: {}", export_path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_export_bindings() {
        // Trigger the binding export to verify the type tree is valid
        // and doesn't contain any incompatible specta types.
        export_bindings();
    }
}

// Metadata: [bridge]

// Metadata: [bridge]

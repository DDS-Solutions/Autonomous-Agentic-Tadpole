//! @docs ARCHITECTURE:Agent
//!
//! ### AI Assist Note
//! **Agent Schema Contract & Unit Tests**: Verifies serde defaults, camelCase/snake_case wire parity, and contract invariants for agent types.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Serialization regression across Rust/TS contract boundary.
//! - **Telemetry Link**: Search `[types_tests]` in tracing logs.

use super::*;

#[cfg(test)]
mod suite {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_engine_agent_deserialization_defaults() {
        let agent_json = json!({
            "id": "test-agent",
            "name": "Test Agent",
            "role": "Tester",
            "department": "QA",
            "description": "Tests things",
            "status": "active",
            "model": "gpt-4",
            "modelConfig": {
                "provider": "openai",
                "modelId": "gpt-4"
            },
            "tokensUsed": 0,
            "budgetUsd": 10.0,
            "costUsd": 0.0
        });

        let agent_str = agent_json.to_string();
        let agent: EngineAgent = serde_json::from_str(&agent_str)
            .expect("Failed to deserialize agent with missing fields");

        assert_eq!(agent.identity.id, "test-agent");
        assert_eq!(agent.economics.token_usage.total_tokens, 0);
        assert!(agent.capabilities.skills.is_empty());
        assert!(agent.capabilities.workflows.is_empty());
        assert!(agent.metadata.is_empty());
        assert_eq!(agent.economics.budget_usd, 10.0);
    }

    #[test]
    fn test_engine_agent_deserialization_full() {
        let agent_json = json!({
            "id": "full-agent",
            "name": "Full Agent",
            "role": "Lead",
            "department": "Engineering",
            "description": "Full description",
            "status": "active",
            "model": "gpt-4o",
            "modelConfig": {
                "provider": "openai",
                "modelId": "gpt-4o"
            },
            "tokensUsed": 100,
            "tokenUsage": {
                "inputTokens": 40,
                "outputTokens": 60,
                "totalTokens": 100
            },
            "skills": ["coding"],
            "workflows": ["deploy"],
            "metadata": {"key": "value"},
            "budgetUsd": 100.0,
            "costUsd": 0.5
        });

        let agent_str = agent_json.to_string();
        let agent: EngineAgent =
            serde_json::from_str(&agent_str).expect("Failed to deserialize full agent");

        assert_eq!(agent.capabilities.skills, vec!["coding"]);
        assert_eq!(agent.capabilities.workflows, vec!["deploy"]);
        assert!(agent.capabilities.mcp_tools.is_empty());
        assert_eq!(agent.metadata.get("key").unwrap(), &json!("value"));
    }

    #[test]
    fn test_model_config_merge() {
        let mut base_extras = std::collections::HashMap::new();
        base_extras.insert("json_mode".to_string(), json!(true));
        base_extras.insert("seed".to_string(), json!(42));

        let base = ModelConfig {
            provider: ModelProvider::Openai,
            model_id: "gpt-4".to_string(),
            temperature: Some(0.7),
            max_tokens: Some(1000),
            extra_parameters: Some(base_extras),
            ..Default::default()
        };

        let mut override_extras = std::collections::HashMap::new();
        override_extras.insert("seed".to_string(), json!(123)); // Should take precedence
        override_extras.insert("thinking".to_string(), json!(true)); // Should be added

        let overrides = ModelConfig {
            provider: ModelProvider::Openai,
            model_id: "gpt-4".to_string(),
            temperature: Some(0.0), // Should take precedence
            extra_parameters: Some(override_extras),
            ..Default::default()
        };

        let merged = overrides.merge(&base);

        assert_eq!(merged.temperature, Some(0.0));
        assert_eq!(merged.max_tokens, Some(1000)); // From base

        let extras = merged.extra_parameters.unwrap();
        assert_eq!(extras.get("json_mode").unwrap(), &json!(true)); // From base
        assert_eq!(extras.get("seed").unwrap(), &json!(123)); // From overrides
        assert_eq!(extras.get("thinking").unwrap(), &json!(true)); // From overrides
    }
}

#[cfg(test)]
mod contract_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_agent_config_update_serialization_parity() {
        let update = AgentConfigUpdate {
            name: Some("Test Agent".to_string()),
            role: Some("Analyst".to_string()),
            department: Some("QA".to_string()),
            budget_usd: Some(500.0),
            metadata: Some(std::collections::HashMap::from([("key".to_string(), json!("value"))])),
            input_tokens: Some(100),
            ..Default::default()
        };

        let serialized = serde_json::to_value(&update).unwrap();
        
        // Assert camelCase serialization (as required by frontend mappers)
        assert_eq!(serialized["name"], "Test Agent");
        assert_eq!(serialized["budgetUsd"], 500.0);
        assert_eq!(serialized["inputTokens"], 100);
        assert!(serialized.get("metadata").is_some());
    }

    #[test]
    fn test_engine_agent_serialization_parity() {
        let agent = EngineAgent {
            identity: AgentIdentity {
                id: "agent-1".to_string(),
                name: "Test Agent".to_string(),
                ..AgentIdentity::default()
            },
            economics: AgentEconomics {
                budget_usd: 100.0,
                ..AgentEconomics::default()
            },
            ..EngineAgent::default()
        };

        let serialized = serde_json::to_value(&agent).unwrap();
        
        // Assert camelCase serialization
        assert_eq!(serialized["id"], "agent-1");
        assert_eq!(serialized["name"], "Test Agent");
        assert_eq!(serialized["budgetUsd"], 100.0);
    }

    #[test]
    fn test_supports_native_tools_slm_filtering() {
        let gemma = ModelConfig {
            model_id: "gemma4:e4b".to_string(),
            ..Default::default()
        };
        assert!(!gemma.supports_native_tools(), "Gemma should not be marked as native tool supporting");

        let phi = ModelConfig {
            model_id: "phi3:medium".to_string(),
            ..Default::default()
        };
        assert!(!phi.supports_native_tools(), "Phi-3 should not be marked as native tool supporting");

        let gpt4 = ModelConfig {
            model_id: "gpt-4o".to_string(),
            ..Default::default()
        };
        assert!(gpt4.supports_native_tools(), "GPT-4o should support native tools");
    }

    #[test]
    fn test_from_legacy_turn_observation_parsing() {
        let legacy_str = "OBSERVATION: Tool read_codebase_file Result: file content here";
        let msg = ChatMessage::from_legacy_turn(legacy_str);
        match msg {
            ChatMessage::Tool { tool_call_id, name, content } => {
                assert_eq!(tool_call_id, "legacy");
                assert_eq!(name, "read_codebase_file");
                assert_eq!(content, "file content here");
            }
            other => panic!("Expected ChatMessage::Tool, got {:?}", other),
        }

        let generic_obs = "OBSERVATION: generic observation text";
        let msg2 = ChatMessage::from_legacy_turn(generic_obs);
        match msg2 {
            ChatMessage::Tool { tool_call_id, name, content } => {
                assert_eq!(tool_call_id, "legacy");
                assert_eq!(name, "observation");
                assert_eq!(content, "generic observation text");
            }
            other => panic!("Expected ChatMessage::Tool, got {:?}", other),
        }
    }
}





// Metadata: [types]

// Metadata: [types_tests]

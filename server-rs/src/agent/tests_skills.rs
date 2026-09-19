//! Skill Verification — Sandbox and tool dispatch tests
//!
//! @docs ARCHITECTURE:Agent
//!
//! @state SkillsRegistry: (Initialized | MockedStorage)
//!
//! ### AI Assist Note
//! **Verification Strategy**: Uses `Uuid` based unique identifiers to avoid
//! collision in the physical file system during concurrent test execution.
//! Tests both the in-memory `DashMap` and the debounced disk sync.
//!
//! ### 🔍 Debugging & Observability
//! - **Telemetry Link**: Search `[tests_skills]` in tracing logs.
//! - **Failure Path**: IO permission errors, malformed Markdown parsing, or
//!   stale file handles preventing clean deletion.
//! - **Trace Scope**: `server-rs::agent::tests_skills`

use super::script_skills::{ScriptSkillsRegistry, SkillDefinition, WorkflowDefinition};
use uuid::Uuid;
use std::error::Error;

#[tokio::test]
async fn test_skills_registry_save_and_sanitize() -> Result<(), Box<dyn Error>> {
    let registry = ScriptSkillsRegistry::new().await?;

    // Create a mock skill with problematic characters in the name
    let weird_name = format!("Bad Skill! *Name_{}", Uuid::new_v4());
    let skill = SkillDefinition {
        id: None,
        name: weird_name.clone(),
        description: "Test skill".to_string(),
        execution_command: "echo test".to_string(),
        schema: serde_json::json!({
            "type": "object",
            "properties": {}
        }),
        oversight_required: true,
        doc_url: None,
        tags: None,
        full_instructions: None,
        negative_constraints: None,
        verification_script: None,
        category: "user".to_string(),
    };

    // Save should sanitize the file name but preserve the internal name
    registry.save_skill(skill.clone()).await?;

    // Verify it is in the in-memory map
    assert!(
        registry.snapshot().skills.contains_key(&weird_name),
        "Skill must be in memory with exact name"
    );

    // Check if the file was created
    // We don't have direct access to registry.skills_dir, but we can attempt to load it
    // by reloading the registry and ensuring our weird name still parses
    let new_registry = ScriptSkillsRegistry::new().await?;
    assert!(
        new_registry.snapshot().skills.contains_key(&weird_name),
        "Skill must persist and load properly"
    );

    // Clean up
    registry.delete_skill(&weird_name).await?;
    assert!(
        !registry.snapshot().skills.contains_key(&weird_name),
        "Skill must be removed from memory"
    );

    let cleanup_registry = ScriptSkillsRegistry::new().await?;
    assert!(
        !cleanup_registry.snapshot().skills.contains_key(&weird_name),
        "Skill must be removed from disk"
    );

    Ok(())
}

#[tokio::test]
async fn test_workflows_registry_save_and_delete() -> Result<(), Box<dyn Error>> {
    let registry = ScriptSkillsRegistry::new().await?;

    let workflow_name = format!("test_workflow_{}", Uuid::new_v4());
    let workflow = WorkflowDefinition {
        id: None,
        name: workflow_name.clone(),
        content: "## Test Workflow\nSteps...".to_string(),
        doc_url: None,
        tags: None,
        category: "user".to_string(),
    };

    registry.save_workflow(workflow.clone()).await?;
    assert!(registry.snapshot().workflows.contains_key(&workflow_name));

    let loaded_registry = ScriptSkillsRegistry::new().await?;
    let snapshot = loaded_registry.snapshot();
    assert!(snapshot.workflows.contains_key(&workflow_name));
    assert_eq!(
        snapshot
            .workflows
            .get(&workflow_name)
            .unwrap()
            .content,
        "## Test Workflow\nSteps..."
    );

    registry.delete_workflow(&workflow_name).await?;
    assert!(!registry.snapshot().workflows.contains_key(&workflow_name));

    Ok(())
}

#[tokio::test]
async fn test_agent_skill_save_delete_and_reload() -> Result<(), Box<dyn Error>> {
    let registry = ScriptSkillsRegistry::new().await?;

    let agent_skill_name = format!("agent_skill_{}", Uuid::new_v4());
    let skill = SkillDefinition {
        id: None,
        name: agent_skill_name.clone(),
        description: "Agent generated skill".to_string(),
        execution_command: "python execution/agent_generated/skills/test.py".to_string(),
        schema: serde_json::json!({
            "type": "object",
            "properties": {}
        }),
        oversight_required: true,
        doc_url: None,
        tags: None,
        full_instructions: None,
        negative_constraints: None,
        verification_script: None,
        category: "ai".to_string(),
    };

    // Save into agent directory
    registry.save_agent_skill(skill).await?;
    assert!(registry.snapshot().skills.contains_key(&agent_skill_name));

    // Delete skill
    registry.delete_skill(&agent_skill_name).await?;
    assert!(!registry.snapshot().skills.contains_key(&agent_skill_name));

    // Reload from disk and verify the skill is NOT resurrected
    let reloaded = ScriptSkillsRegistry::new().await?;
    assert!(
        !reloaded.snapshot().skills.contains_key(&agent_skill_name),
        "Deleted agent skill must NOT be resurrected on reload"
    );

    Ok(())
}

#[test]
fn test_frontmatter_yaml_with_embedded_dashes() {
    let content = r#"---
name: doc_generator
description: Generates markdown docs
command: python doc.py
---
# Documentation
Here is section 1.

---

Here is section 2 after a horizontal rule.
"#;

    let parsed = super::script_skills::parse_skill_md(content).expect("Should parse frontmatter");
    assert_eq!(parsed.name, "doc_generator");
    let body = parsed.full_instructions.expect("Should have body");
    assert!(body.contains("---"), "Body must preserve internal horizontal rule dashes");
    assert!(body.contains("Here is section 2"));
}





// Metadata: [tests_skills]

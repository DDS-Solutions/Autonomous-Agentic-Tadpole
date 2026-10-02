/*
@docs ARCHITECTURE:SovereignKernel

### AI Assist Note
**🛡️ Tadpole OS: Tests**
Core system module providing specialized functionality for the agent swarm.

### 🔍 Debugging & Observability
- **Failure Path**: Unexpected execution drift or type compatibility issues.
- **Telemetry Link**: Traced via active system logging channels.
*/

//! Unit and invariant test suite for ScriptSkillsRegistry.

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use tempfile::tempdir;
    use tokio::fs;
    use crate::error::AppError;
    use crate::agent::script_skills::model::{HookDefinition, SkillDefinition, WorkflowDefinition};
    use crate::agent::script_skills::parse::{
        collision_safe_skill_filename, extract_script_docstring, fnv1a64, parse_skill_md,
    };
    use crate::agent::script_skills::registry::ScriptSkillsRegistry;
    use crate::agent::script_skills::tier::Tier;

    fn temp_registry() -> (tempfile::TempDir, ScriptSkillsRegistry) {
        let temp = tempdir().expect("Failed to create tempdir");
        let registry = ScriptSkillsRegistry::mock(temp.path().to_path_buf());
        (temp, registry)
    }

    #[test]
    fn test_fnv1a64_consistency() {
        assert_eq!(fnv1a64(""), 0xcbf29ce484222325);
        assert_ne!(fnv1a64("a"), fnv1a64("b"));
    }

    #[test]
    fn test_parse_skill_md_basic() {
        let content = r#"---
name: test_skill
description: A test skill
command: python test.py
oversight: false
tags: ["test", "verify"]
---
This is the body content."#;

        let skill = parse_skill_md(content).expect("Should parse valid markdown");
        assert_eq!(skill.name, "test_skill");
        assert_eq!(skill.description, "A test skill");
        assert_eq!(skill.execution_command, "python test.py");
        assert!(!skill.oversight_required);
        assert_eq!(
            skill.tags.unwrap(),
            vec!["test".to_string(), "verify".to_string()]
        );
        assert_eq!(
            skill.full_instructions.unwrap(),
            "This is the body content."
        );
    }

    #[test]
    fn test_parse_skill_md_with_horizontal_rules() {
        let content = r#"---
name: hr_skill
description: A skill with HR
command: python hr.py
---
First line
---
Second line after rule"#;

        let skill = parse_skill_md(content).expect("Should parse markdown with internal horizontal rules");
        assert_eq!(skill.name, "hr_skill");
        assert_eq!(
            skill.full_instructions.unwrap(),
            "First line\n---\nSecond line after rule"
        );
    }

    #[test]
    fn test_extract_script_docstring() {
        let py_script = r#"#!/usr/bin/env python3
"""
This is a module docstring.
It spans multiple lines.

Detailed explanations follow.
"""

import sys
print("hello")
"#;
        assert_eq!(
            extract_script_docstring(py_script),
            Some("This is a module docstring. It spans multiple lines.".to_string())
        );

        let sh_script = r#"#!/bin/bash
# Clean up temporary logs
# This script is run by the telemetry engine.

rm -rf /tmp/logs
"#;
        assert_eq!(
            extract_script_docstring(sh_script),
            Some("Clean up temporary logs This script is run by the telemetry engine.".to_string())
        );
    }

    #[tokio::test]
    async fn test_agent_tier_oversight_enforcement() {
        let (_temp, registry) = temp_registry();

        // Write a skill JSON directly to disk with oversight_required = false in agent directory
        let malicious_skill = SkillDefinition {
            id: None,
            name: "bypass_skill".to_string(),
            description: "Attempting to bypass oversight".to_string(),
            execution_command: "python execution/agent_generated/skills/bypass.py".to_string(),
            schema: serde_json::json!({ "type": "object", "properties": {} }),
            oversight_required: false,
            doc_url: None,
            tags: None,
            full_instructions: None,
            negative_constraints: None,
            verification_script: None,
            category: "ai".to_string(),
        };

        let file_path = registry.agent_skills_dir.join("bypass_skill-0000000000000001.json");
        fs::write(&file_path, serde_json::to_vec(&malicious_skill).unwrap()).await.unwrap();

        // Reload all capabilities
        registry.reload_all().await.unwrap();

        let snap = registry.snapshot();
        let loaded = snap.get_skill("bypass_skill").expect("Skill must be loaded");
        assert!(loaded.oversight_required, "Agent tier MUST enforce oversight_required = true unconditionally");
    }

    #[tokio::test]
    async fn test_generation_monotonicity_across_reload() {
        let (_temp, registry) = temp_registry();
        let gen0 = registry.snapshot().current_generation();

        let skill = SkillDefinition {
            id: None,
            name: "test_gen_skill".to_string(),
            description: "test".to_string(),
            execution_command: "python execution/test.py".to_string(),
            schema: serde_json::json!({ "type": "object", "properties": {} }),
            oversight_required: true,
            doc_url: None,
            tags: None,
            full_instructions: None,
            negative_constraints: None,
            verification_script: None,
            category: "user".to_string(),
        };

        registry.save_skill(skill).await.unwrap();
        let gen1 = registry.snapshot().current_generation();
        assert!(gen1 > gen0, "Generation must increment on write (gen0={}, gen1={})", gen0, gen1);

        registry.reload_all().await.unwrap();
        let gen2 = registry.snapshot().current_generation();
        assert!(gen2 > gen1, "Generation must increment across reload (gen1={}, gen2={})", gen1, gen2);
    }

    #[tokio::test]
    async fn test_built_in_capability_immutable_rejection() {
        let (temp, registry) = temp_registry();
        let built_in_dir = temp.path().join(".agent").join("skills");
        fs::create_dir_all(&built_in_dir).await.unwrap();

        let built_in_skill = r#"---
name: system_probe
description: Built-in immutable probe
command: python probe.py
---
Built-in instructions."#;
        fs::write(built_in_dir.join("SKILL.md"), built_in_skill).await.unwrap();

        registry.reload_all().await.unwrap();
        assert!(registry.snapshot().skills.contains_key("system_probe"));

        let res = registry.delete_skill("system_probe").await;
        assert!(res.is_err(), "Deleting built-in skill must be rejected");
        match res.unwrap_err() {
            AppError::Conflict(msg) => assert!(msg.contains("Built-in capability is immutable")),
            other => panic!("Expected Conflict error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_lookup_time_workflow_alias() {
        let (_temp, registry) = temp_registry();

        let workflow = WorkflowDefinition {
            id: None,
            name: "Audit Process!".to_string(),
            content: "## Steps\n1. Check code".to_string(),
            doc_url: None,
            tags: None,
            category: "user".to_string(),
        };

        registry.save_workflow(workflow).await.unwrap();
        let snap = registry.snapshot();

        // Exact match
        assert!(snap.workflows.contains_key("Audit Process!"));
        // Alias resolved at lookup-time
        let found = snap.get_workflow("audit_process!").expect("Alias must resolve at lookup-time");
        assert_eq!(found.name, "Audit Process!");

        // Deleting by alias deletes the backing file
        registry.delete_workflow("audit_process!").await.unwrap();
        let snap_post = registry.snapshot();
        assert!(snap_post.get_workflow("Audit Process!").is_none());
        assert!(snap_post.get_workflow("audit_process!").is_none());
    }

    #[tokio::test]
    async fn test_hook_lifecycle() {
        let (_temp, registry) = temp_registry();

        let hook = HookDefinition {
            name: "pre_commit".to_string(),
            description: "Lint before commit".to_string(),
            hook_type: "pre_validation".to_string(),
            content: "python execution/lint.py".to_string(),
            active: true,
            category: "user".to_string(),
        };

        registry.save_hook(hook.clone()).await.unwrap();
        assert!(registry.snapshot().hooks.contains_key("pre_commit"));

        registry.delete_hook("pre_commit").await.unwrap();
        assert!(!registry.snapshot().hooks.contains_key("pre_commit"));
    }

    #[test]
    fn test_skill_filename_collision_safe_and_deterministic_resistance() {
        assert_ne!(
            collision_safe_skill_filename("foo bar", "json"),
            collision_safe_skill_filename("foobar", "json")
        );
        assert_ne!(
            collision_safe_skill_filename("foo!", "json"),
            collision_safe_skill_filename("foo?", "json")
        );

        let name_a = "foo!";
        let hash_a = format!("{:016x}", fnv1a64(name_a));
        let name_b = format!("foo-{}", hash_a);

        let file_a = collision_safe_skill_filename(name_a, "json");
        let file_b = collision_safe_skill_filename(&name_b, "json");
        assert_ne!(file_a, file_b, "Hash collision defense must hold");
    }
}

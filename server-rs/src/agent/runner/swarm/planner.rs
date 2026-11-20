//! @docs ARCHITECTURE:Runner
//!
//! ### AI Assist Note
//! **Swarm Subtask DAG Planner**: Parses and validates single, batch, and DAG subtask specifications with duplicate/missing-dependency and cycle guards.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Invalid task array, unknown dependency reference, or cycle detected in DAG.
//! - **Telemetry Link**: Search `[planner]` in tracing logs.

use super::sanitize::validate_slug;
use super::SubTaskSpec;
use crate::agent::runner::tools::error::ToolExecutionError;
use std::collections::HashSet;

pub(super) fn parse_subtask_specs(args: &serde_json::Value) -> Result<Vec<SubTaskSpec>, ToolExecutionError> {
    if let Some(tasks_val) = args.get("tasks") {
        let tasks = tasks_val.as_array().ok_or_else(|| {
            ToolExecutionError::ExecutionFailed("'tasks' parameter must be an array".to_string())
        })?;
        if tasks.is_empty() {
            return Err(ToolExecutionError::ExecutionFailed(
                "PROTOCOL_VIOLATION: 'tasks' array must not be empty".to_string(),
            ));
        }
        let default_msg = args.get("message").and_then(|v| v.as_str()).unwrap_or("");
        let mut specs = Vec::with_capacity(tasks.len());
        let mut seen_ids = HashSet::with_capacity(tasks.len());
        for (idx, item) in tasks.iter().enumerate() {
            let id = match item.get("id") {
                Some(raw_id) => {
                    let s = raw_id.as_str().ok_or_else(|| {
                        ToolExecutionError::ExecutionFailed(format!(
                            "PROTOCOL_VIOLATION: tasks[{}].id must be a string",
                            idx
                        ))
                    })?;
                    validate_slug(s, "task.id")?.to_string()
                }
                None => format!("task_{}", idx),
            };
            if !seen_ids.insert(id.clone()) {
                return Err(ToolExecutionError::ExecutionFailed(format!(
                    "PROTOCOL_VIOLATION: Duplicate task id '{}' in 'tasks'",
                    id
                )));
            }
            let raw_agent = item.get("agent_id").and_then(|v| v.as_str()).ok_or_else(|| {
                ToolExecutionError::ExecutionFailed(format!(
                    "PROTOCOL_VIOLATION: Task '{}' in 'tasks' is missing a valid 'agent_id'",
                    id
                ))
            })?;
            let agent_id = validate_slug(raw_agent, "task.agent_id")?.to_string();
            let message = item
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or(default_msg)
                .to_string();
            let depends_on = match item.get("depends_on") {
                None | Some(serde_json::Value::Null) => Vec::new(),
                Some(deps_val) => {
                    let arr = deps_val.as_array().ok_or_else(|| {
                        ToolExecutionError::ExecutionFailed(format!(
                            "PROTOCOL_VIOLATION: Task '{}' 'depends_on' must be an array",
                            id
                        ))
                    })?;
                    let mut deps = Vec::with_capacity(arr.len());
                    for d in arr {
                        let dep_str = d.as_str().ok_or_else(|| {
                            ToolExecutionError::ExecutionFailed(format!(
                                "PROTOCOL_VIOLATION: Task '{}' 'depends_on' entries must be strings",
                                id
                            ))
                        })?;
                        let valid_dep = validate_slug(dep_str, "task.depends_on[]")?;
                        if valid_dep == id {
                            return Err(ToolExecutionError::ExecutionFailed(format!(
                                "PROTOCOL_VIOLATION: Task '{}' cannot depend on itself",
                                id
                            )));
                        }
                        deps.push(valid_dep.to_string());
                    }
                    deps
                }
            };
            specs.push(SubTaskSpec {
                id,
                agent_id,
                message,
                depends_on,
            });
        }
        for spec in &specs {
            for dep in &spec.depends_on {
                if !seen_ids.contains(dep) {
                    return Err(ToolExecutionError::ExecutionFailed(format!(
                        "PROTOCOL_VIOLATION: Task '{}' depends on unknown task '{}'",
                        spec.id, dep
                    )));
                }
            }
        }
        return Ok(specs);
    }

    let sub_message = args.get("message").and_then(|v| v.as_str()).unwrap_or("");
    let mut target_ids = Vec::new();
    if let Some(ids_val) = args.get("agent_ids") {
        let ids = ids_val.as_array().ok_or_else(|| {
            ToolExecutionError::ExecutionFailed(
                "PROTOCOL_VIOLATION: 'agent_ids' parameter must be an array".to_string(),
            )
        })?;
        if ids.is_empty() {
            return Err(ToolExecutionError::ExecutionFailed(
                "PROTOCOL_VIOLATION: 'agent_ids' array must not be empty".to_string(),
            ));
        }
        for item in ids {
            let s = item.as_str().ok_or_else(|| {
                ToolExecutionError::ExecutionFailed(
                    "PROTOCOL_VIOLATION: 'agent_ids' entries must be strings".to_string(),
                )
            })?;
            target_ids.push(validate_slug(s, "agent_ids[]")?.to_string());
        }
    } else if let Some(agent_val) = args.get("agent_id") {
        let s = agent_val.as_str().ok_or_else(|| {
            ToolExecutionError::ExecutionFailed(
                "PROTOCOL_VIOLATION: 'agent_id' must be a non-empty string".to_string(),
            )
        })?;
        target_ids.push(validate_slug(s, "agent_id")?.to_string());
    } else {
        target_ids.push("general".to_string());
    }

    Ok(target_ids
        .into_iter()
        .enumerate()
        .map(|(idx, agent_id)| SubTaskSpec {
            id: format!("task_{}", idx),
            agent_id,
            message: sub_message.to_string(),
            depends_on: Vec::new(),
        })
        .collect())
}


// Metadata: [planner]

//! @docs ARCHITECTURE:Agent:DAG
//!
//! ### AI Assist Note
//! **Dynamic DAG Task Execution Engine (Production-Grade)**: Manages complex swarm
//! task graphs with explicit dependency tracking, topological cycle detection,
//! state transition validation, recursive failure cascading (Skipped propagation),
//! and deadlock-free terminal resolution.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Circular dependencies in plan generation, node execution panic, or deadlocked barriers.
//! - **Telemetry Link**: Search `[dag]` in tracing logs.

use crate::error::AppError;
use petgraph::algo::is_cyclic_directed;
use petgraph::stable_graph::{NodeIndex, StableDiGraph};
use petgraph::Direction;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use tracing::{error, info, warn};

/// Execution status of a DAG task node with strict state machine transitions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskNodeStatus {
    Pending,
    Ready,
    Running,
    Completed,
    Failed,
    Skipped,
}

/// An individual task vertex in the dependency graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskNode {
    pub id: String,
    pub name: String,
    pub assigned_agent_id: String,
    pub input_payload: serde_json::Value,
    pub output_payload: Option<serde_json::Value>,
    pub status: TaskNodeStatus,
    pub error_detail: Option<String>,
}

/// Directed Acyclic Graph managing multi-agent task dependencies.
#[derive(Debug, Clone)]
pub struct TaskGraph {
    graph: StableDiGraph<TaskNode, ()>,
    node_map: HashMap<String, NodeIndex>,
}

impl Default for TaskGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskGraph {
    pub fn new() -> Self {
        Self {
            graph: StableDiGraph::new(),
            node_map: HashMap::new(),
        }
    }

    /// Adds a task node to the DAG.
    pub fn add_node(
        &mut self,
        id: impl Into<String>,
        name: impl Into<String>,
        assigned_agent_id: impl Into<String>,
        input_payload: serde_json::Value,
    ) -> Result<String, AppError> {
        let id_str = id.into();
        if self.node_map.contains_key(&id_str) {
            return Err(AppError::BadRequest(format!("Node '{}' already exists in TaskGraph", id_str)));
        }

        let node = TaskNode {
            id: id_str.clone(),
            name: name.into(),
            assigned_agent_id: assigned_agent_id.into(),
            input_payload,
            output_payload: None,
            status: TaskNodeStatus::Pending,
            error_detail: None,
        };

        let idx = self.graph.add_node(node);
        self.node_map.insert(id_str.clone(), idx);
        Ok(id_str)
    }

    /// Adds a directed dependency: `from_parent` must complete before `to_child` can start.
    pub fn add_dependency(&mut self, from_parent: &str, to_child: &str) -> Result<(), AppError> {
        let from_idx = *self
            .node_map
            .get(from_parent)
            .ok_or_else(|| AppError::NotFound(format!("Parent node '{}' not found", from_parent)))?;
        let to_idx = *self
            .node_map
            .get(to_child)
            .ok_or_else(|| AppError::NotFound(format!("Child node '{}' not found", to_child)))?;

        let edge_idx = self.graph.add_edge(from_idx, to_idx, ());

        if is_cyclic_directed(&self.graph) {
            self.graph.remove_edge(edge_idx);
            error!("🚨 [TaskGraph] Cycle detected when linking '{}' -> '{}'. Edge rejected!", from_parent, to_child);
            return Err(AppError::BadRequest(format!("Cycle detected in TaskGraph: '{}' -> '{}'", from_parent, to_child)));
        }

        info!("🔗 [TaskGraph] Dependency registered: '{}' -> '{}'", from_parent, to_child);
        Ok(())
    }

    /// Retrieves an immutable reference to a task node by ID.
    pub fn get_node(&self, id: &str) -> Option<&TaskNode> {
        let idx = self.node_map.get(id)?;
        Some(&self.graph[*idx])
    }

    /// Computes and returns all nodes currently `Ready` for parallel execution.
    ///
    /// A node transitions from `Pending` -> `Ready` if all incoming predecessor nodes are `Completed`.
    pub fn get_ready_nodes(&mut self) -> Vec<TaskNode> {
        let mut ready_indices = Vec::new();

        for idx in self.graph.node_indices() {
            let node = &self.graph[idx];
            if node.status != TaskNodeStatus::Pending {
                continue;
            }

            // Check if all parent dependencies are COMPLETED
            let mut all_parents_completed = true;
            let mut parents = self.graph.neighbors_directed(idx, Direction::Incoming);
            while let Some(parent_idx) = parents.next() {
                if self.graph[parent_idx].status != TaskNodeStatus::Completed {
                    all_parents_completed = false;
                    break;
                }
            }

            if all_parents_completed {
                ready_indices.push(idx);
            }
        }

        let mut ready_nodes = Vec::new();
        for idx in ready_indices {
            self.graph[idx].status = TaskNodeStatus::Ready;
            ready_nodes.push(self.graph[idx].clone());
        }

        ready_nodes
    }

    /// Marks a node as `Running` with state transition validation.
    pub fn mark_running(&mut self, id: &str) -> Result<(), AppError> {
        let idx = *self.node_map.get(id).ok_or_else(|| AppError::NotFound(format!("Node '{}' not found", id)))?;
        let current_status = self.graph[idx].status.clone();

        if current_status != TaskNodeStatus::Ready {
            return Err(AppError::BadRequest(format!(
                "Invalid state transition: Node '{}' must be Ready before Running (current: {:?})",
                id, current_status
            )));
        }

        self.graph[idx].status = TaskNodeStatus::Running;
        info!("▶️ [TaskGraph] Node '{}' transitioned to RUNNING", id);
        Ok(())
    }

    /// Marks a node as `Completed` with its execution output.
    pub fn mark_completed(&mut self, id: &str, output: serde_json::Value) -> Result<(), AppError> {
        let idx = *self.node_map.get(id).ok_or_else(|| AppError::NotFound(format!("Node '{}' not found", id)))?;
        let current_status = self.graph[idx].status.clone();

        if current_status != TaskNodeStatus::Running && current_status != TaskNodeStatus::Ready {
            return Err(AppError::BadRequest(format!(
                "Invalid state transition: Node '{}' must be Running or Ready before Completed (current: {:?})",
                id, current_status
            )));
        }

        self.graph[idx].status = TaskNodeStatus::Completed;
        self.graph[idx].output_payload = Some(output);
        info!("✅ [TaskGraph] Node '{}' marked as COMPLETED", id);
        Ok(())
    }

    /// Marks a node as `Failed` and transitively cascades `Skipped` status to all downstream descendants.
    ///
    /// This eliminates swarm deadlocks when an upstream task fails.
    pub fn mark_failed(&mut self, id: &str, error: impl Into<String>) -> Result<(), AppError> {
        let origin_idx = *self.node_map.get(id).ok_or_else(|| AppError::NotFound(format!("Node '{}' not found", id)))?;
        let err_str = error.into();

        // 1. Mark origin node as Failed
        self.graph[origin_idx].status = TaskNodeStatus::Failed;
        self.graph[origin_idx].error_detail = Some(err_str.clone());
        warn!("❌ [TaskGraph] Node '{}' marked as FAILED: {}", id, err_str);

        // 2. Cascade 'Skipped' to all downstream descendants via BFS
        let mut queue = VecDeque::new();
        let mut children = self.graph.neighbors_directed(origin_idx, Direction::Outgoing);
        while let Some(child_idx) = children.next() {
            queue.push_back(child_idx);
        }

        while let Some(current_idx) = queue.pop_front() {
            let child_node = &mut self.graph[current_idx];
            if child_node.status == TaskNodeStatus::Pending || child_node.status == TaskNodeStatus::Ready {
                let skip_reason = format!("Skipped due to upstream predecessor failure in '{}'", id);
                child_node.status = TaskNodeStatus::Skipped;
                child_node.error_detail = Some(skip_reason.clone());
                warn!("⏭️ [TaskGraph] Cascading skip to dependent node '{}': {}", child_node.id, skip_reason);

                // Queue next level children
                let mut next_children = self.graph.neighbors_directed(current_idx, Direction::Outgoing);
                while let Some(next_idx) = next_children.next() {
                    queue.push_back(next_idx);
                }
            }
        }

        Ok(())
    }

    /// Returns true if all nodes are in terminal states (`Completed`, `Failed`, or `Skipped`).
    pub fn is_finished(&self) -> bool {
        self.graph.node_indices().all(|idx| {
            matches!(
                self.graph[idx].status,
                TaskNodeStatus::Completed | TaskNodeStatus::Failed | TaskNodeStatus::Skipped
            )
        })
    }

    /// Returns true if all nodes in the DAG completed successfully (zero failures or skips).
    pub fn all_succeeded(&self) -> bool {
        self.graph.node_indices().all(|idx| self.graph[idx].status == TaskNodeStatus::Completed)
    }

    /// Returns the total node count.
    pub fn node_count(&self) -> usize {
        self.graph.node_count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diamond_dag_parallel_ready_resolution() {
        let mut dag = TaskGraph::new();

        // Construct Diamond DAG: A -> [B, C] -> D
        dag.add_node("A", "Scout Codebase", "scout_agent", serde_json::json!({})).unwrap();
        dag.add_node("B", "Analyze AST", "ast_specialist", serde_json::json!({})).unwrap();
        dag.add_node("C", "Analyze Dependencies", "dep_specialist", serde_json::json!({})).unwrap();
        dag.add_node("D", "Synthesize Report", "alpha_agent", serde_json::json!({})).unwrap();

        dag.add_dependency("A", "B").unwrap();
        dag.add_dependency("A", "C").unwrap();
        dag.add_dependency("B", "D").unwrap();
        dag.add_dependency("C", "D").unwrap();

        // Step 1: Only A is ready initially
        let ready1 = dag.get_ready_nodes();
        assert_eq!(ready1.len(), 1);
        assert_eq!(ready1[0].id, "A");

        dag.mark_running("A").unwrap();
        dag.mark_completed("A", serde_json::json!({"files": ["main.rs"]})).unwrap();

        // Step 2: B and C should BOTH become ready concurrently
        let ready2 = dag.get_ready_nodes();
        assert_eq!(ready2.len(), 2);

        dag.mark_running("B").unwrap();
        dag.mark_completed("B", serde_json::json!({"ast": "ok"})).unwrap();

        // D must wait for C before becoming ready
        let ready_partial = dag.get_ready_nodes();
        assert_eq!(ready_partial.len(), 0);

        dag.mark_running("C").unwrap();
        dag.mark_completed("C", serde_json::json!({"deps": "ok"})).unwrap();

        let ready_final = dag.get_ready_nodes();
        assert_eq!(ready_final.len(), 1);
        assert_eq!(ready_final[0].id, "D");

        dag.mark_running("D").unwrap();
        dag.mark_completed("D", serde_json::json!({"report": "passed"})).unwrap();

        assert!(dag.is_finished());
        assert!(dag.all_succeeded());
    }

    #[test]
    fn test_failure_cascades_skipped_to_descendants_preventing_deadlock() {
        let mut dag = TaskGraph::new();

        // Pipeline: A -> B -> C
        dag.add_node("A", "Fetch Remote Resource", "fetcher", serde_json::json!({})).unwrap();
        dag.add_node("B", "Transform Data", "transformer", serde_json::json!({})).unwrap();
        dag.add_node("C", "Persist Database", "db_writer", serde_json::json!({})).unwrap();

        dag.add_dependency("A", "B").unwrap();
        dag.add_dependency("B", "C").unwrap();

        let ready = dag.get_ready_nodes();
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].id, "A");

        dag.mark_running("A").unwrap();
        // A FAILS (e.g. 404 remote resource missing)
        dag.mark_failed("A", "Resource unavailable (404)").unwrap();

        // B and C must be transitively marked as SKIPPED
        assert_eq!(dag.get_node("A").unwrap().status, TaskNodeStatus::Failed);
        assert_eq!(dag.get_node("B").unwrap().status, TaskNodeStatus::Skipped);
        assert_eq!(dag.get_node("C").unwrap().status, TaskNodeStatus::Skipped);

        // Crucial: DAG must be FINISHED without hanging
        assert!(dag.is_finished(), "DAG must finish after failure cascade without deadlocking");
        assert!(!dag.all_succeeded(), "all_succeeded must be false due to failure");
    }

    #[test]
    fn test_state_transition_validation_guards() {
        let mut dag = TaskGraph::new();
        dag.add_node("Node1", "Task", "agent", serde_json::json!({})).unwrap();

        // Node is Pending, cannot immediately transition to Running without becoming Ready
        let res = dag.mark_running("Node1");
        assert!(res.is_err(), "Must reject Running state when node is Pending");

        // get_ready_nodes marks it Ready
        let ready = dag.get_ready_nodes();
        assert_eq!(ready.len(), 1);

        // Now Running succeeds
        assert!(dag.mark_running("Node1").is_ok());
    }
}

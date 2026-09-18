//! @docs ARCHITECTURE:Persistence
//!
//! ### AI Assist Note
//! **Swarm Persistence**: Manages the lifecycle of inter-agent directives and 
//! peer reviews. Ensures **Atomic Coordination** (SWARM-01) by persisting 
//! mission-critical delegation and audit state to SQLite. Features **Cross-Agent 
//! Visibility** to enable the hierarchical and decentralized swarm loops.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: SQL constraints violation during directive insertion, 
//!   orphaned review requests, or mission_id mismatch during cross-agent 
//!   retrieval.
//! - **Telemetry Link**: Search `[swarm_persistence]` in tracing logs.
//!

use sqlx::SqlitePool;
use uuid::Uuid;
use crate::agent::runner::RunContext;
use crate::error::AppError;

#[derive(sqlx::FromRow)]
#[allow(dead_code)]
pub(crate) struct AgentDirective {
    pub id: String,
    pub mission_id: String,
    pub source_agent_id: String,
    pub target_agent_id: String,
    pub instruction: String,
    pub status: String,
    pub result: Option<String>,
}

#[derive(sqlx::FromRow)]
#[allow(dead_code)]
pub(crate) struct PeerReviewRequest {
    pub id: String,
    pub mission_id: String,
    pub requester_id: String,
    pub reviewer_id: String,
    pub content_to_review: String,
    pub criteria: Option<String>,
    pub status: String,
}

/// Helper function to perform BFS reachability on directed edges:
/// Checks if `target` is reachable from `start` in the directed graph.
pub fn is_reachable(edges: &[(String, String)], start: &str, target: &str) -> bool {
    if start == target {
        return true;
    }
    let mut adj: std::collections::HashMap<&str, Vec<&str>> = std::collections::HashMap::new();
    for (src, dst) in edges {
        adj.entry(src.as_str()).or_default().push(dst.as_str());
    }

    let mut visited = std::collections::HashSet::new();
    let mut queue = std::collections::VecDeque::new();
    queue.push_back(start);
    visited.insert(start);

    while let Some(curr) = queue.pop_front() {
        if let Some(neighbors) = adj.get(curr) {
            for &nxt in neighbors {
                if nxt == target {
                    return true;
                }
                if visited.insert(nxt) {
                    queue.push_back(nxt);
                }
            }
        }
    }

    false
}

/// Saves a new mission directive to the database, enforcing topological acyclicity to prevent delegation deadlocks.
pub async fn save_directive(
    pool: &SqlitePool,
    ctx: &RunContext,
    target_agent_id: &str,
    instruction: &str,
) -> Result<String, AppError> {
    // 1. Check 1-hop self-delegation loop
    if ctx.agent_id == target_agent_id {
        return Err(AppError::BadRequest(
            format!("Direct self-delegation is prohibited: agent '{}' cannot delegate to itself", ctx.agent_id)
        ));
    }

    // 2. Fetch existing active/pending directives for this mission to detect circular chains
    let rows = sqlx::query_as::<_, (String, String)>(
        "SELECT source_agent_id, target_agent_id FROM agent_directives WHERE mission_id = ? AND status IN ('pending', 'active')"
    )
    .bind(&ctx.mission_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::Sqlx)?;

    // 3. Cycle Detection: Check if adding edge (ctx.agent_id -> target_agent_id) creates a cycle.
    // This happens if target_agent_id can already reach ctx.agent_id via active directive edges.
    if is_reachable(&rows, target_agent_id, &ctx.agent_id) {
        return Err(AppError::DomainError {
            code: "CIRCULAR_DELEGATION".to_string(),
            detail: format!(
                "Circular delegation detected: agent '{}' is already in the active delegation chain of agent '{}'",
                ctx.agent_id, target_agent_id
            ),
            help_link: Some("https://tadpole.os/errors/circular-delegation".to_string()),
        });
    }

    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO agent_directives (id, mission_id, source_agent_id, target_agent_id, instruction, status) 
         VALUES (?, ?, ?, ?, ?, 'pending')"
    )
    .bind(&id)
    .bind(&ctx.mission_id)
    .bind(&ctx.agent_id)
    .bind(target_agent_id)
    .bind(instruction)
    .execute(pool)
    .await
    .map_err(AppError::Sqlx)?;
    
    Ok(id)
}

/// Retrieves all pending directives for a specific agent.
pub async fn get_pending_directives(
    pool: &SqlitePool,
    agent_id: &str,
) -> Result<Vec<AgentDirective>, AppError> {
    let rows = sqlx::query_as::<_, AgentDirective>(
        "SELECT id, mission_id, source_agent_id, target_agent_id, instruction, status, result 
         FROM agent_directives 
         WHERE target_agent_id = ? AND status = 'pending'
         ORDER BY created_at ASC",
    )
    .bind(agent_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::Sqlx)?;
    
    Ok(rows)
}

/// Updates the status of a directive.
#[allow(dead_code)]
pub async fn update_directive_status(
    pool: &SqlitePool,
    id: &str,
    status: &str,
    result: Option<&str>,
) -> Result<(), AppError> {
    sqlx::query(
        "UPDATE agent_directives SET status = ?, result = ? WHERE id = ?"
    )
    .bind(status)
    .bind(result)
    .bind(id)
    .execute(pool)
    .await
    .map_err(AppError::Sqlx)?;
    
    Ok(())
}

/// Submits a peer review result.
pub async fn submit_review(
    pool: &SqlitePool,
    id: &str,
    feedback: &str,
    status: &str,
) -> Result<(), AppError> {
    sqlx::query(
        "UPDATE peer_reviews SET status = ?, feedback = ? WHERE id = ?"
    )
    .bind(status)
    .bind(feedback)
    .bind(id)
    .execute(pool)
    .await
    .map_err(AppError::Sqlx)?;
    
    Ok(())
}

/// Submits a peer review request.
pub async fn save_review_request(
    pool: &SqlitePool,
    ctx: &RunContext,
    reviewer_id: &str,
    content: &str,
    criteria: Option<&str>,
) -> Result<String, AppError> {
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO peer_reviews (id, mission_id, requester_id, reviewer_id, content_to_review, criteria, status) 
         VALUES (?, ?, ?, ?, ?, ?, 'requested')"
    )
    .bind(&id)
    .bind(&ctx.mission_id)
    .bind(&ctx.agent_id)
    .bind(reviewer_id)
    .bind(content)
    .bind(criteria)
    .execute(pool)
    .await
    .map_err(AppError::Sqlx)?;
    
    Ok(id)
}

/// Retrieves all review requests for a specific reviewer.
pub async fn get_pending_reviews(
    pool: &SqlitePool,
    reviewer_id: &str,
) -> Result<Vec<PeerReviewRequest>, AppError> {
    let rows = sqlx::query_as::<_, PeerReviewRequest>(
        "SELECT id, mission_id, requester_id, reviewer_id, content_to_review, criteria, status 
         FROM peer_reviews 
         WHERE reviewer_id = ? AND status = 'requested'
         ORDER BY created_at ASC",
    )
    .bind(reviewer_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::Sqlx)?;
    
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_reachable_empty_and_disconnected() {
        let edges = vec![
            ("A".to_string(), "B".to_string()),
            ("B".to_string(), "C".to_string()),
        ];
        assert!(is_reachable(&edges, "A", "C"));
        assert!(is_reachable(&edges, "A", "B"));
        assert!(!is_reachable(&edges, "C", "A"));
        assert!(!is_reachable(&edges, "B", "A"));
        assert!(!is_reachable(&edges, "A", "D"));
        assert!(is_reachable(&edges, "A", "A"));
    }

    #[test]
    fn test_is_reachable_diamond_dag() {
        let edges = vec![
            ("A".to_string(), "B".to_string()),
            ("A".to_string(), "C".to_string()),
            ("B".to_string(), "D".to_string()),
            ("C".to_string(), "D".to_string()),
        ];
        assert!(is_reachable(&edges, "A", "D"));
        assert!(!is_reachable(&edges, "D", "A"));
        assert!(!is_reachable(&edges, "B", "C"));
    }

    #[tokio::test]
    async fn test_save_directive_cycle_detection() -> Result<(), AppError> {
        let state = crate::state::AppState::new_mock().await;
        let pool = &state.resources.pool;

        // Seed agents in agents table
        for id in &["ag-alpha", "ag-beta", "ag-gamma"] {
            sqlx::query("INSERT INTO agents (id, name, role, department, description, status, metadata) \
                         VALUES (?, 'Test Agent', 'Worker', 'Core', 'desc', 'idle', '{}')")
                .bind(id)
                .execute(pool).await?;
        }

        // Seed mission into mission_history to satisfy foreign key constraint
        sqlx::query("INSERT INTO mission_history (id, title, status, agent_id) \
                     VALUES ('ms-test-cycle', 'Test Cycle Mission', 'active', 'ag-alpha')")
            .execute(pool).await?;

        // Context for ag-alpha
        let mut ctx_alpha = crate::agent::runner::RunContext::default();
        ctx_alpha.agent_id = "ag-alpha".to_string();
        ctx_alpha.mission_id = "ms-test-cycle".to_string();

        // 1. Direct self-delegation should fail immediately
        let self_err = save_directive(pool, &ctx_alpha, "ag-alpha", "loop").await;
        assert!(self_err.is_err(), "Self-delegation must be blocked");

        // 2. Alpha delegates to Beta (valid)
        let d1 = save_directive(pool, &ctx_alpha, "ag-beta", "task 1").await;
        assert!(d1.is_ok(), "Alpha -> Beta should succeed");

        // Context for ag-beta
        let mut ctx_beta = crate::agent::runner::RunContext::default();
        ctx_beta.agent_id = "ag-beta".to_string();
        ctx_beta.mission_id = "ms-test-cycle".to_string();

        // 3. Beta delegates back to Alpha (2-hop cycle: Alpha -> Beta -> Alpha) should be rejected!
        let cycle_err = save_directive(pool, &ctx_beta, "ag-alpha", "task 2").await;
        match cycle_err {
            Err(AppError::DomainError { code, .. }) => assert_eq!(code, "CIRCULAR_DELEGATION"),
            other => panic!("Expected CIRCULAR_DELEGATION, got: {:?}", other),
        }

        // 4. Beta delegates to Gamma (valid chain Alpha -> Beta -> Gamma)
        let d2 = save_directive(pool, &ctx_beta, "ag-gamma", "task 3").await;
        assert!(d2.is_ok(), "Beta -> Gamma should succeed");

        // Context for ag-gamma
        let mut ctx_gamma = crate::agent::runner::RunContext::default();
        ctx_gamma.agent_id = "ag-gamma".to_string();
        ctx_gamma.mission_id = "ms-test-cycle".to_string();

        // 5. Gamma delegates to Alpha (3-hop cycle: Alpha -> Beta -> Gamma -> Alpha) should be rejected!
        let cycle3_err = save_directive(pool, &ctx_gamma, "ag-alpha", "task 4").await;
        match cycle3_err {
            Err(AppError::DomainError { code, .. }) => assert_eq!(code, "CIRCULAR_DELEGATION"),
            other => panic!("Expected CIRCULAR_DELEGATION, got: {:?}", other),
        }

        Ok(())
    }
}

// Metadata: [swarm_persistence]

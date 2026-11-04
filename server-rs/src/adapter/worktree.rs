//! @docs ARCHITECTURE:Infrastructure:Sandboxing
//!
//! ### AI Assist Note
//! **Git Worktree Sandbox Adapter**: Provides lightweight, isolated workspace
//! environments for agent coding and refactoring tasks. Inspired by Infinity's
//! `sandbox-local` pattern, this adapter isolates disk mutations in temporary
//! worktrees (`.tmp/worktrees/<task_id>`), allowing zero-risk branch rollbacks,
//! conflict-free concurrent editing, and atomic diff-based reviews before merging.
//!
//! ### 🔍 Debugging & Observability
//! - **Telemetry Link**: Search `[worktree]` in tracing logs.
//! - **Failure Path**: Git command failure, invalid task ID characters, or worktree collision.
//! - **Trace Scope**: `server-rs::adapter::worktree`

use crate::error::AppError;
use std::path::{Path, PathBuf};
use std::process::Command;
use tracing::{error, info, warn};

/// An isolated Git worktree sandbox for agent-driven code operations.
#[derive(Debug, Clone)]
pub struct WorktreeSandbox {
    pub repo_root: PathBuf,
    pub worktree_path: PathBuf,
    pub branch_name: String,
}

impl WorktreeSandbox {
    /// Creates a new isolated Git worktree under `.tmp/worktrees/<task_id>`.
    pub fn create(repo_root: &Path, task_id: &str) -> Result<Self, AppError> {
        // Enforce strict identifier safety: alphanumeric, hyphen, underscore only
        if task_id.is_empty()
            || !task_id
                .chars()
                .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            return Err(AppError::BadRequest(format!(
                "Invalid task_id '{}': only alphanumeric, hyphen, and underscore characters are allowed",
                task_id
            )));
        }

        let branch_name = format!("agent-worktree-{}", task_id);
        let worktree_path = repo_root.join(".tmp").join("worktrees").join(task_id);

        if let Some(parent) = worktree_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                AppError::InternalServerError(format!("Failed to create worktree parent directory: {}", e))
            })?;
        }

        info!(
            "🌳 [worktree] Creating isolated sandbox at '{}' on branch '{}'",
            worktree_path.display(),
            branch_name
        );

        let output = Command::new("git")
            .current_dir(repo_root)
            .args([
                "worktree",
                "add",
                "-b",
                &branch_name,
                worktree_path.to_str().unwrap_or_default(),
                "HEAD",
            ])
            .output()
            .map_err(|e| {
                AppError::InternalServerError(format!("Failed to execute 'git worktree add': {}", e))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("🚨 [worktree] 'git worktree add' failed: {}", stderr);
            return Err(AppError::InternalServerError(format!(
                "git worktree add failed: {}",
                stderr.trim()
            )));
        }

        Ok(Self {
            repo_root: repo_root.to_path_buf(),
            worktree_path,
            branch_name,
        })
    }

    /// Computes the git diff of changes made inside the sandbox.
    pub fn diff(&self) -> Result<String, AppError> {
        let output = Command::new("git")
            .current_dir(&self.worktree_path)
            .args(["diff", "HEAD"])
            .output()
            .map_err(|e| AppError::InternalServerError(format!("Failed to run 'git diff': {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AppError::InternalServerError(format!(
                "git diff failed: {}",
                stderr.trim()
            )));
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Safely resolves a subpath within the isolated worktree directory.
    pub fn get_path(&self, relative_path: &str) -> Result<PathBuf, AppError> {
        let clean = relative_path
            .replace('\\', "/")
            .trim_start_matches('/')
            .to_string();
        if clean.contains("..") {
            return Err(AppError::BadRequest(
                "Path traversal '..' prohibited in worktree sandbox".into(),
            ));
        }
        Ok(self.worktree_path.join(clean))
    }

    /// Forcefully removes the worktree and cleans up the temporary branch.
    pub fn cleanup(self) -> Result<(), AppError> {
        info!(
            "🧹 [worktree] Cleaning up sandbox at '{}' (branch '{}')",
            self.worktree_path.display(),
            self.branch_name
        );

        let remove_output = Command::new("git")
            .current_dir(&self.repo_root)
            .args([
                "worktree",
                "remove",
                "--force",
                self.worktree_path.to_str().unwrap_or_default(),
            ])
            .output();

        if let Err(e) = remove_output {
            warn!("⚠️ [worktree] Error executing 'git worktree remove': {}", e);
        }

        let branch_output = Command::new("git")
            .current_dir(&self.repo_root)
            .args(["branch", "-D", &self.branch_name])
            .output();

        if let Err(e) = branch_output {
            warn!("⚠️ [worktree] Error executing 'git branch -D': {}", e);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn init_mock_git_repo(dir: &Path) {
        let _ = Command::new("git").current_dir(dir).args(["init"]).output();
        let _ = Command::new("git")
            .current_dir(dir)
            .args(["config", "user.name", "Tadpole"])
            .output();
        let _ = Command::new("git")
            .current_dir(dir)
            .args(["config", "user.email", "agent@tadpole.os"])
            .output();

        let main_file = dir.join("main.rs");
        let _ = std::fs::write(&main_file, "fn main() { println!(\"original\"); }");
        let _ = Command::new("git")
            .current_dir(dir)
            .args(["add", "."])
            .output();
        let _ = Command::new("git")
            .current_dir(dir)
            .args(["commit", "-m", "Initial commit"])
            .output();
    }

    #[test]
    fn test_worktree_task_id_validation() {
        let temp_dir = TempDir::new().unwrap();
        // Path traversal attempts must be rejected
        let err_traversal = WorktreeSandbox::create(temp_dir.path(), "../escape");
        assert!(err_traversal.is_err());

        // Illegal characters must be rejected
        let err_special = WorktreeSandbox::create(temp_dir.path(), "task;rm -rf /");
        assert!(err_special.is_err());
    }

    #[test]
    fn test_worktree_happy_path_and_isolation() {
        let temp_dir = TempDir::new().unwrap();
        init_mock_git_repo(temp_dir.path());

        let sandbox_res = WorktreeSandbox::create(temp_dir.path(), "test-task-1");
        if let Ok(sandbox) = sandbox_res {
            assert!(sandbox.worktree_path.exists());

            // Write modified file inside worktree
            let target = sandbox.get_path("main.rs").unwrap();
            std::fs::write(&target, "fn main() { println!(\"isolated_agent_edit\"); }").unwrap();

            // Diff inside sandbox must show change
            let diff = sandbox.diff().unwrap();
            assert!(diff.contains("isolated_agent_edit"));

            // Main repo must be untouched
            let original = std::fs::read_to_string(temp_dir.path().join("main.rs")).unwrap();
            assert!(original.contains("original"));
            assert!(!original.contains("isolated_agent_edit"));

            // Cleanup
            sandbox.cleanup().unwrap();
        }
    }
}

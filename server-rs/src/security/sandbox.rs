//! @docs ARCHITECTURE:AgentExecutionRuntime
//! @docs ARCHITECTURE:SecurityModel
//!
//! ### AI Assist Note
//! **Isolated Skill Execution Sandbox**: Implements secure execution environments
//! for promoted AI agent skills using Docker containerization or WebAssembly (Wasmtime).
//! Gated by **Resource Limits** (CPU, memory) and filesystem isolation to mitigate
//! Remote Code Execution (RCE) on the host machine.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Docker daemon unresponsive, Wasm file missing or malformed,
//!   resource exhaustion (OOM), or execution timeouts.
//! - **Telemetry Link**: Search `[sandbox]` in logs.

use crate::error::AppError;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;

const SANDBOX_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone)]
pub struct SandboxConfig {
    pub use_docker: bool,
    pub use_wasm: bool,
    pub allow_host_fallback: bool,
    pub cpu_limit: Option<f32>,
    pub memory_limit_mb: Option<usize>,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        let use_docker = std::env::var("USE_SANDBOX_DOCKER")
            .map(|v| v == "true")
            .unwrap_or(false);
        let use_wasm = std::env::var("USE_SANDBOX_WASM")
            .map(|v| v == "true")
            .unwrap_or(false);
        let allow_host_fallback = std::env::var("ALLOW_HOST_SKILL_EXECUTION")
            .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
            .unwrap_or(false);
        let cpu_limit = std::env::var("SANDBOX_CPU_LIMIT")
            .ok()
            .and_then(|v| v.parse::<f32>().ok());
        let memory_limit_mb = std::env::var("SANDBOX_MEMORY_LIMIT_MB")
            .ok()
            .and_then(|v| v.parse::<usize>().ok());

        Self {
            use_docker,
            use_wasm,
            allow_host_fallback,
            cpu_limit,
            memory_limit_mb,
        }
    }
}

/// Run a command with a hard timeout. On timeout, kill the child process
/// (and optionally a Docker container by name) so work cannot continue orphaned.
async fn output_with_timeout(
    mut cmd: Command,
    timeout: Duration,
    provider_id: &str,
    timeout_detail: &str,
    docker_container_name: Option<&str>,
) -> Result<std::process::Output, AppError> {
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    cmd.kill_on_drop(true);

    let mut child = cmd.spawn().map_err(AppError::Io)?;

    // Keep Child so we can kill it on timeout; collect stdio after wait.
    let stdout_pipe = child.stdout.take();
    let stderr_pipe = child.stderr.take();

    match tokio::time::timeout(timeout, child.wait()).await {
        Ok(Ok(status)) => {
            use tokio::io::AsyncReadExt;
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            if let Some(mut out) = stdout_pipe {
                let _ = out.read_to_end(&mut stdout).await;
            }
            if let Some(mut err) = stderr_pipe {
                let _ = err.read_to_end(&mut stderr).await;
            }
            Ok(std::process::Output {
                status,
                stdout,
                stderr,
            })
        }
        Ok(Err(e)) => Err(AppError::Io(e)),
        Err(_elapsed) => {
            let _ = child.start_kill();
            let _ = child.wait().await;

            if let Some(name) = docker_container_name {
                let _ = Command::new("docker")
                    .args(["rm", "-f", name])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .kill_on_drop(true)
                    .status()
                    .await;
            }

            Err(AppError::InfrastructureError {
                provider_id: provider_id.to_string(),
                detail: timeout_detail.to_string(),
                help_link: None,
            })
        }
    }
}

fn apply_docker_resource_limits(docker_cmd: &mut Command, config: &SandboxConfig) {
    // Sensible defaults so a runaway skill cannot exhaust the host.
    let mem = config.memory_limit_mb.unwrap_or(512);
    let cpu = config.cpu_limit.unwrap_or(1.0);
    docker_cmd.arg("-m").arg(format!("{}m", mem));
    docker_cmd.arg("--cpus").arg(cpu.to_string());
    docker_cmd.arg("--pids-limit").arg("256");
    docker_cmd.arg("--security-opt").arg("no-new-privileges:true");
}

fn new_container_name() -> String {
    format!(
        "tadpole-skill-{}",
        uuid::Uuid::new_v4().simple()
    )
}

/// Executes a skill command within the configured sandbox.
pub async fn execute_sandboxed(
    command_str: &str,
    args_json: &str,
    workspace_root: &Path,
    config: &SandboxConfig,
) -> Result<String, AppError> {
    let mut parts = command_str.split_whitespace();
    let program = parts
        .next()
        .ok_or_else(|| AppError::BadRequest("Empty command".to_string()))?;
    let args: Vec<&str> = parts.collect();

    if config.use_docker {
        tracing::info!(
            "[Sandbox] Spawning Docker container for execution: {}",
            command_str
        );

        if which::which("docker").is_err() {
            return Err(AppError::InfrastructureError {
                provider_id: "docker".to_string(),
                detail: "Docker CLI binary is not installed or not found on PATH.".to_string(),
                help_link: Some("https://docs.docker.com/get-docker/".to_string()),
            });
        }

        let workspace_str = workspace_root.to_string_lossy().to_string();
        let container_name = new_container_name();
        let mut docker_cmd = Command::new("docker");
        docker_cmd
            .arg("run")
            .arg("--rm")
            .arg("--name")
            .arg(&container_name)
            .arg("--cap-drop=ALL")
            .arg("--network=none")
            .arg("-v")
            .arg(format!("{}:/workspace", workspace_str))
            .arg("-w")
            .arg("/workspace")
            .arg("-e")
            .arg(format!("TADPOLE_SKILL_ARGS={}", args_json));

        apply_docker_resource_limits(&mut docker_cmd, config);

        docker_cmd.arg("python:3.10-slim");
        docker_cmd.arg(program);
        for arg in args {
            docker_cmd.arg(arg);
        }

        let output = output_with_timeout(
            docker_cmd,
            SANDBOX_TIMEOUT,
            "sandboxed_python",
            "Docker execution timed out after 60s (container force-removed)",
            Some(&container_name),
        )
        .await?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        if output.status.success() {
            Ok(stdout)
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            Err(AppError::InfrastructureError {
                provider_id: "sandboxed_python".to_string(),
                detail: format!("Docker execution failed: {}\nstdout: {}", stderr, stdout),
                help_link: None,
            })
        }
    } else if config.use_wasm || program.ends_with(".wasm") {
        tracing::info!(
            "[Sandbox] Spawning Wasmtime sandbox for execution: {}",
            command_str
        );

        if which::which("wasmtime").is_err() {
            return Err(AppError::InfrastructureError {
                provider_id: "wasmtime".to_string(),
                detail: "Wasmtime executable was not found on PATH. Install wasmtime or verify system PATH to execute WebAssembly skills.".to_string(),
                help_link: Some("https://wasmtime.dev/".to_string()),
            });
        }

        let mut wasm_cmd = Command::new("wasmtime");
        wasm_cmd
            .arg("run")
            .arg("--dir")
            .arg(workspace_root)
            .arg(program);

        for arg in args {
            wasm_cmd.arg(arg);
        }

        wasm_cmd.env("TADPOLE_SKILL_ARGS", args_json);
        wasm_cmd.current_dir(workspace_root);

        let output = output_with_timeout(
            wasm_cmd,
            SANDBOX_TIMEOUT,
            "wasmtime",
            "Wasm execution timed out after 60s (process killed)",
            None,
        )
        .await?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        if output.status.success() {
            Ok(stdout)
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            Err(AppError::InfrastructureError {
                provider_id: "wasmtime".to_string(),
                detail: format!("Wasm execution failed: {}\nstdout: {}", stderr, stdout),
                help_link: None,
            })
        }
    } else {
        if !config.allow_host_fallback {
            tracing::error!(
                "[Sandbox] Unsandboxed host execution denied for command: {}. Sandboxing is enforced: enable Docker (USE_SANDBOX_DOCKER=true), Wasm (USE_SANDBOX_WASM=true), or explicitly permit host execution (ALLOW_HOST_SKILL_EXECUTION=true).",
                command_str
            );
            return Err(AppError::Forbidden(
                "Unsandboxed skill execution on host is disabled by default. Configure USE_SANDBOX_DOCKER=true, USE_SANDBOX_WASM=true, or explicitly set ALLOW_HOST_SKILL_EXECUTION=true.".to_string()
            ));
        }

        tracing::warn!(
            "[Sandbox] Falling back to standard execution for command: {}",
            command_str
        );

        let mut cmd = Command::new(program);
        for arg in args {
            cmd.arg(arg);
        }
        cmd.env("TADPOLE_SKILL_ARGS", args_json);
        cmd.current_dir(workspace_root);

        let output = output_with_timeout(
            cmd,
            SANDBOX_TIMEOUT,
            "legacy_skill",
            "Legacy execution timed out after 60s (process killed)",
            None,
        )
        .await?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        if output.status.success() {
            Ok(stdout)
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            Err(AppError::InfrastructureError {
                provider_id: "legacy_skill".to_string(),
                detail: format!("Legacy execution failed: {}\nstdout: {}", stderr, stdout),
                help_link: None,
            })
        }
    }
}

/// Executes a shell command within the configured sandbox (Docker, Wasm, or gated host fallback).
pub async fn execute_sandboxed_shell(
    command_str: &str,
    workspace_root: &Path,
    config: &SandboxConfig,
) -> Result<String, AppError> {
    if command_str.trim().is_empty() {
        return Err(AppError::BadRequest("Empty shell command".to_string()));
    }

    if config.use_docker {
        tracing::info!(
            "[Sandbox] Spawning Docker container for shell execution: {}",
            command_str
        );

        if which::which("docker").is_err() {
            return Err(AppError::InfrastructureError {
                provider_id: "docker".to_string(),
                detail: "Docker CLI binary is not installed or not found on PATH.".to_string(),
                help_link: Some("https://docs.docker.com/get-docker/".to_string()),
            });
        }

        let workspace_str = workspace_root.to_string_lossy().to_string();
        let container_name = new_container_name();
        let mut docker_cmd = Command::new("docker");
        docker_cmd
            .arg("run")
            .arg("--rm")
            .arg("--name")
            .arg(&container_name)
            .arg("--cap-drop=ALL")
            .arg("--network=none")
            .arg("-v")
            .arg(format!("{}:/workspace", workspace_str))
            .arg("-w")
            .arg("/workspace");

        apply_docker_resource_limits(&mut docker_cmd, config);

        docker_cmd.arg("tadpole-os:latest");
        docker_cmd.arg("sh").arg("-c").arg(command_str);

        let output = output_with_timeout(
            docker_cmd,
            SANDBOX_TIMEOUT,
            "sandboxed_shell",
            "Docker shell execution timed out after 60s (container force-removed)",
            Some(&container_name),
        )
        .await?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        if output.status.success() {
            Ok(stdout)
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            Err(AppError::InfrastructureError {
                provider_id: "sandboxed_shell".to_string(),
                detail: format!(
                    "Docker shell execution failed: {}\nstdout: {}",
                    stderr, stdout
                ),
                help_link: None,
            })
        }
    } else {
        if !config.allow_host_fallback {
            tracing::error!(
                "[Sandbox] Unsandboxed host shell execution denied for: {}. Sandboxing is enforced: enable Docker (USE_SANDBOX_DOCKER=true) or explicitly permit host execution (ALLOW_HOST_SKILL_EXECUTION=true).",
                command_str
            );
            return Err(AppError::Forbidden(
                "Unsandboxed shell execution on host is disabled by default. Configure USE_SANDBOX_DOCKER=true, or explicitly set ALLOW_HOST_SKILL_EXECUTION=true.".to_string()
            ));
        }

        tracing::warn!(
            "[Sandbox] Falling back to host shell execution for: {}",
            command_str
        );
        let shell = if cfg!(windows) { "powershell" } else { "sh" };
        let flag = if cfg!(windows) { "-Command" } else { "-c" };

        let mut cmd = Command::new(shell);
        cmd.arg(flag).arg(command_str);
        cmd.current_dir(workspace_root);

        let output = output_with_timeout(
            cmd,
            SANDBOX_TIMEOUT,
            "host_shell",
            "Host shell execution timed out after 60s (process killed)",
            None,
        )
        .await?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        if output.status.success() {
            Ok(stdout)
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            Err(AppError::InfrastructureError {
                provider_id: "host_shell".to_string(),
                detail: format!(
                    "Host shell execution failed: {}\nstdout: {}",
                    stderr, stdout
                ),
                help_link: None,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_unsandboxed_execution_blocked_by_default() {
        let config = SandboxConfig {
            use_docker: false,
            use_wasm: false,
            allow_host_fallback: false,
            cpu_limit: None,
            memory_limit_mb: None,
        };
        let res = execute_sandboxed("echo hello", "{}", Path::new("."), &config).await;
        assert!(res.is_err());
        match res.unwrap_err() {
            AppError::Forbidden(msg) => {
                assert!(msg.contains("Unsandboxed skill execution"));
            }
            other => panic!("Expected AppError::Forbidden, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_unsandboxed_shell_blocked_by_default() {
        let config = SandboxConfig {
            use_docker: false,
            use_wasm: false,
            allow_host_fallback: false,
            cpu_limit: None,
            memory_limit_mb: None,
        };
        let res = execute_sandboxed_shell("echo hello", Path::new("."), &config).await;
        assert!(res.is_err());
        match res.unwrap_err() {
            AppError::Forbidden(msg) => {
                assert!(msg.contains("Unsandboxed shell execution on host is disabled by default"));
            }
            other => panic!("Expected AppError::Forbidden, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_empty_command_rejected() {
        let config = SandboxConfig::default();
        let res = execute_sandboxed("", "{}", Path::new("."), &config).await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn test_empty_shell_command_rejected() {
        let config = SandboxConfig::default();
        let res = execute_sandboxed_shell("", Path::new("."), &config).await;
        assert!(res.is_err());
    }

    #[test]
    fn test_container_name_is_unique_prefix() {
        let a = new_container_name();
        let b = new_container_name();
        assert!(a.starts_with("tadpole-skill-"));
        assert_ne!(a, b);
    }
}

// Metadata: [sandbox]

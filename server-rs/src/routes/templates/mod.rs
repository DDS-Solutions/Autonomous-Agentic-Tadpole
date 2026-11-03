//! @docs ARCHITECTURE:Networking
//!
//! ### AI Assist Note
//! **Templates Facade**: Multi-agent swarm starter kits management subsystem.
//! Orchestrates template catalog proxying, cryptographically verified repository cloning,
//! atomic asset deployment, and audited uninstallation.
//! - **System Identity**: TadpoleOS/1.1.59
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Upstream registry failures, SSRF validation rejections, or malformed template configurations.
//! - **Telemetry Link**: Search `[templates]` in tracing logs.
//! - **Trace Scope**: `server-rs::routes::templates`

pub mod catalog;
pub mod install;
pub mod models;
pub mod principal;
pub mod security;
#[cfg(test)]
pub mod tests;
pub mod uninstall;

// Re-export models
pub use models::{
    InstallReceipt, InstallTemplateRequest, InstallTemplateResponse, InstalledTemplateSummary,
    UninstallTemplateResponse,
};

// Re-export principal
pub use principal::TemplatePrincipal;

// Re-export security utilities
pub use security::{
    is_private_or_local_ip, parse_expected_skill_hash, redact_url, validate_install_request,
    validate_install_request_async, validate_mcp_server_config, verify_skill_sha256,
    RESERVED_AGENT_IDS,
};

// Re-export installation engine & constants
pub use install::{
    clone_template_repository, copy_markdown_assets, install_template, merge_template_mcp_config,
    preflight_validate_agents, preflight_validate_skills, GIT_CLONE_TIMEOUT,
    MAX_AGENT_CONFIG_BYTES, MAX_SKILL_SCRIPT_BYTES,
};

// Re-export uninstallation engine
pub use uninstall::{
    remove_validated_files_with_digest, resolve_installed_template_folder, uninstall_template,
};

// Re-export catalog proxy & cache
pub use catalog::{
    get_template_catalog, list_installed_templates, CatalogCache, CATALOG_CACHE, CATALOG_TTL,
    MAX_CATALOG_BODY_BYTES, REGISTRY_UPSTREAM_URL,
};

// Metadata: [templates]


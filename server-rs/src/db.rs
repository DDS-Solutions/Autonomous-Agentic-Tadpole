//! @docs ARCHITECTURE:Persistence
//!
//! ### AI Assist Note
//! **Engine Persistence (Database Layer)**: Orchestrates the lifecycle
//! of the **SQLite** connection pool and schema management for the
//! Tadpole OS engine. Features **WAL (Write-Ahead Logging)** and
//! `synchronous=NORMAL`: optimized for the high-throughput state
//! updates required by autonomous agent swarms. Implements **Automated
//! Migrations** via `sqlx`: ensures the schema is always consistent 
//! and up-to-date with the `migrations/` directory before system 
//! boot (DB-01).
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Database locked (`SQLITE_BUSY`) during
//!   bursty write operations, migration checksum mismatches due to
//!   manual tampering, or path permission errors on the
//!   `DATABASE_URL` target.
//! - **Recovery Protocol (DB-01)**:
//!   1. If `SQLITE_BUSY` persists, verify no rogue `sqlite3` processes
//!      hold a lock.
//!   2. If checksum fail, check `Hotfix Reconciler` logic in `db.rs`
//!      and ensure the `_sqlx_migrations` entry matches the
//!      `sqlx::migrate!` binary hash.
//!   3. For I/O errors, ensure the `.tmp/` or data directory has
//!      `0755` permissions for the engine process.
//! - **Telemetry Link**: Search for `[Database]` or `[SQLx]` in
//!   `tracing` logs for query performance and migration status.
//! - **Trace Scope**: `server-rs::db`
use anyhow::Result;
use sqlx::{sqlite::SqliteConnectOptions, SqlitePool};
use std::str::FromStr;



/// Initializes the SQLite database pool and executes pending migrations.
///
/// Sets high-performance defaults (WAL mode, busy timeout) and ensures that the
/// backend schema is in sync with the `migrations/` directory.
pub async fn init_db(database_url: &str) -> Result<SqlitePool> {
    let options = SqliteConnectOptions::from_str(database_url)?
        .create_if_missing(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .pragma("synchronous", "NORMAL") // Relax strict fsync for WAL speed
        .pragma("cache_size", "-64000") // Use 64MB of memory for the page cache
        .pragma("temp_store", "memory") // Keep temp tables in RAM
        .pragma("mmap_size", "268435456") // 256MB memory-mapped I/O
        .busy_timeout(std::time::Duration::from_secs(30)); // Increased for swarm concurrency

    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(25)
        .min_connections(3)
        .acquire_timeout(std::time::Duration::from_secs(8))
        .connect_with(options)
        .await?;

    // Run migrations automatically at startup.
    // SEC: This ensures the schema is always consistent and up-to-date.
    let migrator = sqlx::migrate!("./migrations");



    if let Err(e) = migrator.run(&pool).await {
        tracing::error!("❌ Critical failure during database migrations: {}", e);
        return Err(anyhow::anyhow!("Database migration failure: {}", e));
    }

    tracing::info!("✅ Database migrations applied successfully");

    // Seed baseline entities if the DB is fresh and NOT explicitly skipping
    // skip_seed is handled by checking a thread-local or environment variable for flexibility
    let skip_seed = std::env::var("SKIP_DB_SEED").map(|v| v == "true").unwrap_or(false);

    if !skip_seed {
        seed_baseline_agents(&pool).await?;
        seed_baseline_providers().await?;
        seed_baseline_workflows().await?;
        seed_baseline_mcp_config().await?;
    }

    Ok(pool)
}

/// Ensures default agents defined in `data/agents.json` exist in the system.
async fn seed_baseline_agents(pool: &SqlitePool) -> Result<()> {
    tracing::info!("🌱 [Database] Synchronizing baseline agents from bundle...");

    // 2. Resolve data file path
    let resource_root = std::env::var("RESOURCE_ROOT").unwrap_or_else(|_| ".".to_string());
    let agents_json_path = find_bundled_file(&resource_root, "data/agents.json");

    if let Some(path) = agents_json_path {
        tracing::info!("📂 [Database] Syncing baseline agents at {:?}", path);

        let content = tokio::fs::read_to_string(&path).await?;
        let agents: Vec<serde_json::Value> = serde_json::from_str(&content)?;

        let mut tx = pool.begin().await?;

        for agent_val in agents {
            let id = agent_val["id"].as_str().unwrap_or_default();
            if id.is_empty() { continue; }

            let name = agent_val["name"].as_str().unwrap_or("Unknown");
            let role = agent_val["role"].as_str().unwrap_or("Specialist");
            let dept = agent_val["department"].as_str().unwrap_or("Swarm Core");
            let desc = agent_val["description"].as_str().unwrap_or("");
            let model_id = agent_val["model_config"]["modelId"].as_str()
                .or_else(|| agent_val["model_id"].as_str())
                .or_else(|| agent_val["model"].as_str());
            let provider = agent_val["model_config"]["provider"].as_str()
                .or_else(|| agent_val["provider"].as_str())
                .unwrap_or("google");
            let theme = agent_val["theme_color"].as_str().unwrap_or("#4fd1c5");
            let skills = serde_json::to_string(&agent_val["skills"]).unwrap_or_else(|_| "[]".to_string());
            let workflows = serde_json::to_string(&agent_val["workflows"]).unwrap_or_else(|_| "[]".to_string());

            sqlx::query(
                "INSERT INTO agents (id, name, role, department, description, status, provider, model_id, theme_color, metadata, skills, workflows, mcp_tools, active_model_slot, category)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                 ON CONFLICT(id) DO UPDATE SET
                    name = excluded.name,
                    role = excluded.role,
                    department = excluded.department,
                    description = excluded.description,
                    provider = excluded.provider,
                    model_id = excluded.model_id,
                    theme_color = excluded.theme_color,
                    skills = excluded.skills,
                    workflows = excluded.workflows"
            )
            .bind(id)
            .bind(name)
            .bind(role)
            .bind(dept)
            .bind(desc)
            .bind("idle")
            .bind(provider)
            .bind(model_id)
            .bind(theme)
            .bind("{}")
            .bind(skills)
            .bind(workflows)
            .bind("[]") // mcp_tools
            .bind(1)
            .bind("user")
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
    } else {
        tracing::warn!("⚠️ [Database] Sync file 'agents.json' not found in bundle; falling back...");
        let agent_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM agents")
            .fetch_one(pool)
            .await?;
        if agent_count == 0 {
            seed_minimal_alpha(pool).await?;
        }
    }

    Ok(())
}

async fn seed_minimal_alpha(pool: &SqlitePool) -> Result<()> {
    tracing::info!("🌱 Seeding minimal Alpha agent...");
    sqlx::query(
        "INSERT OR IGNORE INTO agents (id, name, role, department, description, status, provider, model_id, theme_color, metadata, skills, workflows, mcp_tools, active_model_slot, category)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
    )
    .bind("1")
    .bind("Alpha")
    .bind("Agent of Nine")
    .bind("Swarm Core")
    .bind("The primary intelligence node of the Tadpole OS network.")
    .bind("idle")
    .bind("google")
    .bind("gemini-1.5-flash")
    .bind("#4fd1c5")
    .bind("{}")
    .bind("[]")
    .bind("[]")
    .bind("[]")
    .bind(1)
    .bind("user")
    .execute(pool)
    .await?;
    Ok(())
}

/// Ensures default provider configs and model catalogs are available in the data directory.
async fn seed_baseline_providers() -> Result<()> {
    let resource_root = std::env::var("RESOURCE_ROOT").unwrap_or_else(|_| ".".to_string());
    let base_dir = std::env::current_dir().map_err(|e| anyhow::anyhow!("Failed to get current directory: {}", e))?;
    let data_dir = base_dir.join("data");

    // Ensure data dir exists
    if !data_dir.exists() {
        tokio::fs::create_dir_all(&data_dir).await?;
    }

    let files_to_seed = [
        "infra_providers.json",
        "infra_models.json",
        "routines.json"
    ];

    for filename in files_to_seed {
        let dest_path = data_dir.join(filename);
        if !dest_path.exists() {
            if let Some(src_path) = find_bundled_file(&resource_root, &format!("data/{}", filename)) {
                tracing::info!("🌱 [System] Seeding {} from {:?}...", filename, src_path);
                tokio::fs::copy(&src_path, &dest_path).await?;
            }
        }
    }

    Ok(())
}

/// Ensures default workflows (directives) are loaded from the bundle.
async fn seed_baseline_workflows() -> Result<()> {
    let resource_root = std::env::var("RESOURCE_ROOT").unwrap_or_else(|_| ".".to_string());
    let base_dir = std::env::current_dir().map_err(|e| anyhow::anyhow!("Failed to get current directory: {}", e))?;
    let directives_dir = base_dir.join("directives");

    if !directives_dir.exists() {
        tokio::fs::create_dir_all(&directives_dir).await?;
    }

    // Copy from bundled data/workflows to directives/
    let bundled_workflows_dir = find_bundled_file(&resource_root, "data/workflows");

    if let Some(src_dir) = bundled_workflows_dir {
        let mut entries = tokio::fs::read_dir(&src_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("md") {
                if let Some(filename) = path.file_name() {
                    let dest_path = directives_dir.join(filename);
                    if !dest_path.exists() {
                        tracing::info!("🌱 [System] Seeding workflow {:?} from {:?}...", filename, path);
                        tokio::fs::copy(&path, &dest_path).await?;
                    }
                }
            }
        }
    }

    Ok(())
}

/// Ensures default MCP configuration is available.
async fn seed_baseline_mcp_config() -> Result<()> {
    let resource_root = std::env::var("RESOURCE_ROOT").unwrap_or_else(|_| ".".to_string());
    let base_dir = std::env::current_dir().map_err(|e| anyhow::anyhow!("Failed to get current directory: {}", e))?;
    let agent_dir = base_dir.join(".agent");

    if !agent_dir.exists() {
        tokio::fs::create_dir_all(&agent_dir).await?;
    }

    let mcp_filename = "mcp_config.json";
    let dest_path = agent_dir.join(mcp_filename);

    if !dest_path.exists() {
        if let Some(src_path) = find_bundled_file(&resource_root, &format!(".agent/{}", mcp_filename)) {
            tracing::info!("🌱 [System] Seeding MCP configuration from {:?}...", src_path);
            tokio::fs::copy(&src_path, &dest_path).await?;
        }
    }

    Ok(())
}

/// Helper to find a bundled file, handling Tauri v2's '_up_' prefix if needed.
fn find_bundled_file(resource_root: &str, relative_path: &str) -> Option<std::path::PathBuf> {
    let root = std::path::Path::new(resource_root);

    // 1. Try direct path (e.g., resources/data/agents.json)
    let direct = root.join(relative_path);
    if direct.exists() {
        return Some(direct);
    }

    // 2. Try _up_ path (e.g., resources/_up_/data/agents.json)
    let up_path = root.join("_up_").join(relative_path);
    if up_path.exists() {
        return Some(up_path);
    }

    // 3. Last ditch: check if current dir has it (usually only for dev)
    let dev_path = std::path::Path::new(".").join(relative_path);
    if dev_path.exists() {
        return Some(dev_path);
    }

    None
}



// Metadata: [db]

// Metadata: [db]

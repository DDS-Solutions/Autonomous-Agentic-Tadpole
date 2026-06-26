//! @docs ARCHITECTURE:DatabaseEngine
//!
//! ### AI Assist Note
//! **Database Backup & Integrity Check Test Suite**: Verifies transactional
//! VACUUM-based hot backup and integrity_check safety limits.
//!
//! ### 🔍 Debugging & Observability
//! - **Trace Scope**: `server-rs::routes::backup_restore_tests`

#[cfg(test)]
mod tests {
    use crate::db::{run_backup, check_integrity, init_db};
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_db_backup_and_integrity_check() {
        // 1. Initialize a temporary source DB in a real file to support VACUUM INTO
        let tmp_dir = tempdir().expect("Failed to create temp dir");
        let db_path = tmp_dir.path().join("source.db");
        let database_url = format!("sqlite:{}", db_path.to_str().unwrap());

        // Initialize source database and apply migrations
        std::env::set_var("SKIP_DB_SEED", "true");
        let pool = init_db(&database_url).await.expect("Failed to init source DB");

        // Seed some test data
        // Note: agents.metadata is NOT NULL — supply an empty JSON object.
        sqlx::query(
            "INSERT INTO agents (id, name, role, department, description, status, theme_color, metadata, skills, workflows, active_model_slot) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )
            .bind("test-agent-99")
            .bind("BackupAgent")
            .bind("Backup Tester")
            .bind("QA")
            .bind("Verifies hot backup capabilities")
            .bind("idle")
            .bind("#ff0000")
            .bind("{}")
            .bind("[]")
            .bind("[]")
            .bind(1)
            .execute(&pool)
            .await
            .expect("Failed to insert test agent");

        // 2. Perform hot backup to another file
        let backup_path = tmp_dir.path().join("backup.db");
        let backup_str = backup_path.to_str().unwrap();

        run_backup(&pool, backup_str).await.expect("Failed to execute run_backup");

        // 3. Verify backup file exists and check integrity on it
        assert!(backup_path.exists());

        let backup_url = format!("sqlite:{}", backup_str);
        let backup_pool = sqlx::SqlitePool::connect(&backup_url).await.expect("Failed to connect to backup DB");

        let integrity = check_integrity(&backup_pool).await.expect("Failed to run check_integrity");
        assert_eq!(integrity, "ok");

        // 4. Verify backed-up data matches
        let name: String = sqlx::query_scalar("SELECT name FROM agents WHERE id = 'test-agent-99'")
            .fetch_one(&backup_pool)
            .await
            .expect("Failed to query backup DB for seeded agent");
        assert_eq!(name, "BackupAgent");
    }
}

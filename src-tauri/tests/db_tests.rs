use openvideoai_lib::db::DbManager;
use tempfile::tempdir;

#[test]
fn test_db_initialization_and_wal_mode() {
    let dir = tempdir().expect("failed to create tempdir");
    let db_path = dir.path().join("test_openvideoai.db");

    let db_manager = DbManager::new(db_path.clone()).expect("failed to initialize db");

    // Verify journal mode is WAL
    let journal_mode = db_manager
        .get_journal_mode()
        .expect("failed to get journal_mode");
    assert_eq!(journal_mode.to_uppercase(), "WAL");
}

#[test]
fn test_db_migrations_are_idempotent() {
    let dir = tempdir().expect("failed to create tempdir");
    let db_path = dir.path().join("test_openvideoai.db");

    let db_manager = DbManager::new(db_path).expect("failed to initialize db");

    // Initial run of migrations
    let res1 = db_manager.run_migrations();
    assert!(res1.is_ok(), "First migration run failed: {:?}", res1.err());

    let count1 = db_manager
        .get_migration_count()
        .expect("failed to count migrations");
    assert!(count1 > 0, "Expected migrations to have been applied");

    // Second run must be completely idempotent
    let res2 = db_manager.run_migrations();
    assert!(
        res2.is_ok(),
        "Second migration run failed: {:?}",
        res2.err()
    );

    let count2 = db_manager
        .get_migration_count()
        .expect("failed to count migrations");
    assert_eq!(count1, count2, "Migration count changed on second run");
}

#[test]
fn test_db_health_query() {
    let dir = tempdir().expect("failed to create tempdir");
    let db_path = dir.path().join("test_openvideoai.db");

    let db_manager = DbManager::new(db_path.clone()).expect("failed to initialize db");
    db_manager.run_migrations().expect("migrations failed");

    let health = db_manager.check_health().expect("health check failed");
    assert!(health.ok, "Expected db health ok to be true");
    assert_eq!(health.path, db_path.to_string_lossy().to_string());
}

#[test]
fn test_migration_3_schema_and_foreign_keys() {
    let dir = tempdir().expect("failed to create tempdir");
    let db_path = dir.path().join("test_migration_3.db");

    let db_manager = DbManager::new(db_path).expect("failed to initialize db");
    db_manager.run_migrations().expect("migrations failed");

    assert_eq!(
        db_manager.get_migration_count().expect("migration count"),
        3
    );

    let conn = db_manager.conn().expect("conn failed");

    // Test insert job with idempotency key
    conn.execute(
        "INSERT INTO jobs (id, job_type, status, priority, idempotency_key, payload, created_at, updated_at)
         VALUES ('job-1', 'test_task', 'queued', 10, 'key-123', '{\"test\":true}', datetime('now'), datetime('now'));",
        [],
    ).expect("insert job failed");

    // Duplicate idempotency key must fail
    let duplicate_res = conn.execute(
        "INSERT INTO jobs (id, job_type, status, priority, idempotency_key, payload, created_at, updated_at)
         VALUES ('job-2', 'test_task', 'queued', 5, 'key-123', '{\"test\":false}', datetime('now'), datetime('now'));",
        [],
    );
    assert!(
        duplicate_res.is_err(),
        "Duplicate idempotency key should violate UNIQUE constraint"
    );

    // Insert checkpoint
    conn.execute(
        "INSERT INTO job_checkpoints (id, job_id, stage, checkpoint_data, created_at)
         VALUES ('cp-1', 'job-1', 'extraction', '{\"progress\":50}', datetime('now'));",
        [],
    )
    .expect("insert checkpoint failed");

    // Checkpoint with non-existent job must fail foreign key
    let invalid_cp = conn.execute(
        "INSERT INTO job_checkpoints (id, job_id, stage, checkpoint_data, created_at)
         VALUES ('cp-2', 'non-existent-job', 'stage1', '{}', datetime('now'));",
        [],
    );
    assert!(
        invalid_cp.is_err(),
        "Foreign key constraint on job_id must be enforced"
    );

    // Insert domain event
    conn.execute(
        "INSERT INTO domain_events (id, event_type, version, job_id, payload, created_at)
         VALUES ('evt-1', 'JobCreated', 1, 'job-1', '{\"name\":\"test\"}', datetime('now'));",
        [],
    )
    .expect("insert domain event failed");
}

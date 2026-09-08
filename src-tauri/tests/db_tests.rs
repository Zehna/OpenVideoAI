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

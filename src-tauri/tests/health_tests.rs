use openvideoai_lib::db::DbManager;
use openvideoai_lib::health::HealthService;
use std::sync::Arc;
use tempfile::tempdir;

#[test]
fn test_health_snapshot_aggregation() {
    let dir = tempdir().expect("failed to create tempdir");
    let db_path = dir.path().join("health_test.db");

    let db_manager = Arc::new(DbManager::new(db_path).expect("failed to init db"));
    db_manager
        .run_migrations()
        .expect("failed to run migrations");

    let health_service = HealthService::new(db_manager);
    let snapshot = health_service.get_snapshot();

    // Verify snapshot fields
    assert_eq!(snapshot.app_version, env!("CARGO_PKG_VERSION"));
    assert!(snapshot.database.ok);
    assert!(!snapshot.database.path.is_empty());

    // Tools should reflect host environment
    // ffmpeg and ffprobe are detected on this machine
    assert!(snapshot.ffmpeg.available);
    assert!(snapshot.ffprobe.available);
}

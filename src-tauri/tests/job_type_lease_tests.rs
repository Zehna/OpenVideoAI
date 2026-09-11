use openvideoai_lib::db::migrations::run_all_migrations;
use openvideoai_lib::db::DbManager;
use openvideoai_lib::jobs::manager::JobManager;
use openvideoai_lib::jobs::model::{JobState, JobSubmission};
use rusqlite::Connection;
use std::sync::Arc;
use tempfile::tempdir;

#[test]
fn test_type_filtered_atomic_leasing_only_acquires_matching_job_type() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut conn = Connection::open(&db_path).unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    run_all_migrations(&mut conn).unwrap();

    let db_manager = Arc::new(DbManager::new(db_path).unwrap());
    let job_manager = Arc::new(JobManager::new(db_manager));

    // 1. Submit Job A of type "test.other"
    let sub_a = JobSubmission {
        job_type: "test.other".to_string(),
        priority: Some(100), // Higher priority!
        idempotency_key: Some("test:other:1".to_string()),
        project_id: None,
        payload: "{}".to_string(),
        max_retries: Some(3),
        payload_version: Some(1),
    };
    let job_a = job_manager.submit_job(sub_a).unwrap();

    // 2. Submit Job B of type "render.timeline"
    let sub_b = JobSubmission {
        job_type: "render.timeline".to_string(),
        priority: Some(10), // Lower priority!
        idempotency_key: Some("render:timeline:1".to_string()),
        project_id: None,
        payload: "{}".to_string(),
        max_retries: Some(3),
        payload_version: Some(1),
    };
    let job_b = job_manager.submit_job(sub_b).unwrap();

    // 3. Render worker requests job of type "render.timeline"
    let leased_opt = job_manager
        .acquire_next_job_for_type("render.timeline", 60)
        .unwrap();
    let leased = leased_opt.expect("Should lease job B");

    // Invariant: Leased job must be Job B
    assert_eq!(leased.id, job_b.id);
    assert_eq!(leased.job_type, "render.timeline");
    assert_eq!(leased.status, JobState::Running);

    // Invariant: Job A must remain untouched in 'queued' state despite higher priority
    let fresh_a = job_manager.get_job(&job_a.id).unwrap().unwrap();
    assert_eq!(fresh_a.status, JobState::Queued);
    assert_eq!(fresh_a.leased_by, None);
}

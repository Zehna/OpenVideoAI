use openvideoai_lib::db::DbManager;
use openvideoai_lib::jobs::manager::JobManager;
use openvideoai_lib::jobs::model::{JobState, JobSubmission};
use std::sync::Arc;
use tempfile::tempdir;

fn setup_test_manager() -> (tempfile::TempDir, JobManager) {
    let dir = tempdir().expect("failed to create tempdir");
    let db_path = dir.path().join("job_recovery_test.db");
    let db = Arc::new(DbManager::new(db_path).expect("failed to init db"));
    db.run_migrations().expect("failed to run migrations");
    let manager = JobManager::new(db);
    (dir, manager)
}

#[test]
fn test_reap_expired_leases_recovers_crashed_worker_job() {
    let (_dir, manager) = setup_test_manager();

    let job = manager
        .submit_job(JobSubmission {
            job_type: "heavy_processing".to_string(),
            payload: "{}".to_string(),
            priority: Some(10),
            idempotency_key: None,
            project_id: None,
            max_retries: Some(2),
        })
        .expect("submit failed");

    // Acquire lease for only 1 second
    let leased = manager.acquire_next_job(1).expect("acquire failed");
    assert!(leased.is_some());
    assert_eq!(leased.unwrap().id, job.id);

    // Sleep 1.5 seconds to let lease expire
    std::thread::sleep(std::time::Duration::from_millis(1500));

    // Reap expired leases
    let reaped_count = manager.reap_expired_leases().expect("reap failed");
    assert_eq!(reaped_count, 1, "Expected 1 expired job to be reaped");

    let updated = manager.get_job(&job.id).unwrap().unwrap();
    assert_eq!(updated.status, JobState::Retryable);
    assert_eq!(updated.retry_count, 1);
    assert!(updated.leased_by.is_none());

    // New worker should now be able to acquire this recovered job
    let reacquired = manager.acquire_next_job(30).unwrap();
    assert!(reacquired.is_some());
    assert_eq!(reacquired.unwrap().id, job.id);
}

#[test]
fn test_reap_expired_leases_exhausts_to_dead_letter() {
    let (_dir, manager) = setup_test_manager();

    let job = manager
        .submit_job(JobSubmission {
            job_type: "fatal_crash_task".to_string(),
            payload: "{}".to_string(),
            priority: Some(0),
            idempotency_key: None,
            project_id: None,
            max_retries: Some(1), // only 1 retry allowed
        })
        .expect("submit failed");

    // First lease & expiration
    let _ = manager.acquire_next_job(1).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1200));
    let r1 = manager.reap_expired_leases().unwrap();
    assert_eq!(r1, 1);

    // Second lease & expiration (retry_count = 1 == max_retries)
    let _ = manager.acquire_next_job(1).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1200));
    let r2 = manager.reap_expired_leases().unwrap();
    assert_eq!(r2, 1);

    let dead = manager.get_job(&job.id).unwrap().unwrap();
    assert_eq!(dead.status, JobState::DeadLetter);
    assert!(dead.error.unwrap().contains("Lease expired"));
    assert!(dead.completed_at.is_some());
}

#[test]
fn test_active_lease_is_not_reaped() {
    let (_dir, manager) = setup_test_manager();

    let job = manager
        .submit_job(JobSubmission {
            job_type: "healthy_task".to_string(),
            payload: "{}".to_string(),
            priority: Some(0),
            idempotency_key: None,
            project_id: None,
            max_retries: Some(3),
        })
        .unwrap();

    // Acquire lease for 60 seconds
    let _ = manager.acquire_next_job(60).unwrap();

    // Reaping now should find 0 expired leases
    let reaped = manager.reap_expired_leases().unwrap();
    assert_eq!(reaped, 0);

    let current = manager.get_job(&job.id).unwrap().unwrap();
    assert_eq!(current.status, JobState::Running);
}

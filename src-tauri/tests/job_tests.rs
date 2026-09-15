use openvideoai_lib::db::DbManager;
use openvideoai_lib::jobs::model::{JobState, JobSubmission};
use openvideoai_lib::jobs::repository::JobRepository;
use std::sync::Arc;
use tempfile::tempdir;

fn setup_test_db() -> (tempfile::TempDir, Arc<DbManager>) {
    let dir = tempdir().expect("failed to create tempdir");
    let db_path = dir.path().join("jobs_test.db");
    let db = Arc::new(DbManager::new(db_path).expect("failed to init db"));
    db.run_migrations().expect("failed to run migrations");
    (dir, db)
}

#[test]
fn test_enqueue_and_get_job() {
    let (_dir, db) = setup_test_db();
    let conn = db.conn().unwrap();

    let submission = JobSubmission {
        job_type: "proxy_generation".to_string(),
        payload: r#"{"source":"video.mp4"}"#.to_string(),
        payload_version: None,
        priority: Some(5),
        idempotency_key: None,
        project_id: None,
        max_retries: Some(3),
    };

    let job = JobRepository::enqueue(&conn, submission).expect("enqueue failed");
    assert_eq!(job.job_type, "proxy_generation");
    assert_eq!(job.status, JobState::Queued);
    assert_eq!(job.priority, 5);
    assert_eq!(job.retry_count, 0);

    let retrieved = JobRepository::get_by_id(&conn, &job.id)
        .expect("get failed")
        .expect("job not found");
    assert_eq!(retrieved.id, job.id);
    assert_eq!(retrieved.payload, r#"{"source":"video.mp4"}"#);
}

#[test]
fn test_idempotent_enqueue_returns_existing() {
    let (_dir, db) = setup_test_db();
    let conn = db.conn().unwrap();

    let sub1 = JobSubmission {
        job_type: "transcribe".to_string(),
        payload: r#"{"file":"audio.mp3"}"#.to_string(),
        payload_version: None,
        priority: Some(0),
        idempotency_key: Some("unique-key-abc".to_string()),
        project_id: None,
        max_retries: Some(2),
    };

    let sub2 = JobSubmission {
        job_type: "transcribe".to_string(),
        payload: r#"{"file":"different.mp3"}"#.to_string(),
        payload_version: None,
        priority: Some(10),
        idempotency_key: Some("unique-key-abc".to_string()),
        project_id: None,
        max_retries: Some(2),
    };

    let job1 = JobRepository::enqueue(&conn, sub1).expect("enqueue 1 failed");
    let job2 = JobRepository::enqueue(&conn, sub2).expect("enqueue 2 failed");

    // Must return the exact same job ID and original payload
    assert_eq!(job1.id, job2.id);
    assert_eq!(job2.payload, r#"{"file":"audio.mp3"}"#);
}

#[test]
fn test_priority_ordering_on_acquire_lease() {
    let (_dir, db) = setup_test_db();
    let conn = db.conn().unwrap();

    // Enqueue 3 jobs with different priorities
    JobRepository::enqueue(
        &conn,
        JobSubmission {
            job_type: "task_low".to_string(),
            payload: "{}".to_string(),
            payload_version: None,
            priority: Some(0),
            idempotency_key: None,
            project_id: None,
            max_retries: Some(3),
        },
    )
    .unwrap();

    JobRepository::enqueue(
        &conn,
        JobSubmission {
            job_type: "task_high".to_string(),
            payload: "{}".to_string(),
            payload_version: None,
            priority: Some(100),
            idempotency_key: None,
            project_id: None,
            max_retries: Some(3),
        },
    )
    .unwrap();

    JobRepository::enqueue(
        &conn,
        JobSubmission {
            job_type: "task_medium".to_string(),
            payload: "{}".to_string(),
            payload_version: None,
            priority: Some(50),
            idempotency_key: None,
            project_id: None,
            max_retries: Some(3),
        },
    )
    .unwrap();

    // First lease must be high priority (100)
    let lease1 = JobRepository::acquire_lease(&conn, "worker-1", 30)
        .unwrap()
        .expect("lease 1");
    assert_eq!(lease1.job_type, "task_high");
    assert_eq!(lease1.status, JobState::Running);
    assert_eq!(lease1.leased_by.as_deref(), Some("worker-1"));

    // Second lease must be medium priority (50)
    let lease2 = JobRepository::acquire_lease(&conn, "worker-2", 30)
        .unwrap()
        .expect("lease 2");
    assert_eq!(lease2.job_type, "task_medium");

    // Third lease must be low priority (0)
    let lease3 = JobRepository::acquire_lease(&conn, "worker-1", 30)
        .unwrap()
        .expect("lease 3");
    assert_eq!(lease3.job_type, "task_low");

    // No more queued jobs
    let lease4 = JobRepository::acquire_lease(&conn, "worker-1", 30).unwrap();
    assert!(lease4.is_none());
}

#[test]
fn test_heartbeat_extends_lease() {
    let (_dir, db) = setup_test_db();
    let conn = db.conn().unwrap();

    let job = JobRepository::enqueue(
        &conn,
        JobSubmission {
            job_type: "render".to_string(),
            payload: "{}".to_string(),
            payload_version: None,
            priority: Some(0),
            idempotency_key: None,
            project_id: None,
            max_retries: Some(1),
        },
    )
    .unwrap();

    let leased = JobRepository::acquire_lease(&conn, "worker-alpha", 10)
        .unwrap()
        .unwrap();
    let original_expiry = leased.lease_expires_at.expect("has expiry");

    // Extend heartbeat by 60 seconds
    let extended = JobRepository::heartbeat(&conn, &job.id, "worker-alpha", 60).unwrap();
    assert!(extended, "heartbeat should return true for valid worker");

    let updated = JobRepository::get_by_id(&conn, &job.id).unwrap().unwrap();
    assert!(updated.lease_expires_at.unwrap() > original_expiry);

    // Mismatched worker cannot heartbeat
    let wrong_worker = JobRepository::heartbeat(&conn, &job.id, "worker-beta", 60).unwrap();
    assert!(!wrong_worker);
}

#[test]
fn test_checkpoint_persistence_and_retrieval() {
    let (_dir, db) = setup_test_db();
    let conn = db.conn().unwrap();

    let job = JobRepository::enqueue(
        &conn,
        JobSubmission {
            job_type: "multi_stage".to_string(),
            payload: "{}".to_string(),
            payload_version: None,
            priority: Some(0),
            idempotency_key: None,
            project_id: None,
            max_retries: Some(3),
        },
    )
    .unwrap();

    JobRepository::record_checkpoint(&conn, &job.id, "audio_extracted", r#"{"duration":12.5}"#)
        .expect("cp 1");
    JobRepository::record_checkpoint(&conn, &job.id, "speech_transcribed", r#"{"words":48}"#)
        .expect("cp 2");

    let cps = JobRepository::get_checkpoints(&conn, &job.id).expect("get checkpoints");
    assert_eq!(cps.len(), 2);
    assert_eq!(cps[0].stage, "audio_extracted");
    assert_eq!(cps[1].stage, "speech_transcribed");
}

#[test]
fn test_job_retry_transition_to_dead_letter() {
    let (_dir, db) = setup_test_db();
    let conn = db.conn().unwrap();

    let job = JobRepository::enqueue(
        &conn,
        JobSubmission {
            job_type: "failing_task".to_string(),
            payload: "{}".to_string(),
            payload_version: None,
            priority: Some(0),
            idempotency_key: None,
            project_id: None,
            max_retries: Some(2), // allow 2 retries
        },
    )
    .unwrap();

    // 1st failure -> retryable (retry_count = 1)
    let _ = JobRepository::acquire_lease(&conn, "worker-1", 10).unwrap();
    let f1 = JobRepository::fail_job(&conn, &job.id, "Transient network timeout", true).unwrap();
    assert_eq!(f1.status, JobState::Retryable);
    assert_eq!(f1.retry_count, 1);
    assert!(f1.leased_by.is_none());

    // 2nd failure -> retryable (retry_count = 2)
    let _ = JobRepository::acquire_lease(&conn, "worker-1", 10).unwrap();
    let f2 = JobRepository::fail_job(&conn, &job.id, "Socket hangup", true).unwrap();
    assert_eq!(f2.status, JobState::Retryable);
    assert_eq!(f2.retry_count, 2);

    // 3rd failure -> exceeds max_retries (2) -> dead_letter
    let _ = JobRepository::acquire_lease(&conn, "worker-1", 10).unwrap();
    let f3 = JobRepository::fail_job(&conn, &job.id, "Fatal out of memory", true).unwrap();
    assert_eq!(f3.status, JobState::DeadLetter);
    assert_eq!(f3.retry_count, 2);
}

#[test]
fn test_job_cancellation() {
    let (_dir, db) = setup_test_db();
    let conn = db.conn().unwrap();

    let job = JobRepository::enqueue(
        &conn,
        JobSubmission {
            job_type: "cancel_me".to_string(),
            payload: "{}".to_string(),
            payload_version: None,
            priority: Some(0),
            idempotency_key: None,
            project_id: None,
            max_retries: Some(1),
        },
    )
    .unwrap();

    let cancelled = JobRepository::cancel_job(&conn, &job.id).unwrap();
    assert_eq!(cancelled.status, JobState::Cancelled);
    assert!(cancelled.completed_at.is_some());

    // After cancellation, acquire_lease must return None
    let lease = JobRepository::acquire_lease(&conn, "worker-1", 10).unwrap();
    assert!(lease.is_none());
}

#[test]
fn test_job_fail_non_retryable_sets_failed_status() {
    let (_dir, db) = setup_test_db();
    let conn = db.conn().unwrap();

    let job = JobRepository::enqueue(
        &conn,
        JobSubmission {
            job_type: "terminal_fail_task".to_string(),
            payload: "{}".to_string(),
            payload_version: None,
            priority: Some(0),
            idempotency_key: None,
            project_id: None,
            max_retries: Some(3),
        },
    )
    .unwrap();

    // Acquire lease so job is running
    let _ = JobRepository::acquire_lease(&conn, "worker-1", 10).unwrap();

    // Fail job with allow_retry = false
    let failed = JobRepository::fail_job(
        &conn,
        &job.id,
        "Non-retryable: Model not supported or auth missing",
        false,
    )
    .unwrap();

    // Must transition to Failed, NOT DeadLetter!
    assert_eq!(failed.status, JobState::Failed);
    // Retry count must NOT be incremented (stays 0)
    assert_eq!(failed.retry_count, 0);
    assert!(failed.leased_by.is_none());
    assert!(failed.lease_token.is_none());
    assert!(failed.lease_expires_at.is_none());
    assert!(failed.completed_at.is_some());
}

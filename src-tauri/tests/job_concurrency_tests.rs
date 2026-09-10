use chrono::{Duration, Utc};
use openvideoai_lib::db::DbManager;
use openvideoai_lib::events::bus::EventBus;
use openvideoai_lib::events::model::DomainEvent;
use openvideoai_lib::jobs::manager::JobManager;
use openvideoai_lib::jobs::model::{JobState, JobSubmission};
use openvideoai_lib::jobs::repository::{JobError, JobRepository};
use serde_json::json;
use std::sync::Arc;
use std::thread;
use tempfile::tempdir;

fn setup_test_system() -> (tempfile::TempDir, Arc<DbManager>, Arc<EventBus>) {
    let dir = tempdir().expect("failed to create tempdir");
    let db_path = dir.path().join("concurrency_test.db");
    let db = Arc::new(DbManager::new(db_path).expect("failed to init db"));
    db.run_migrations().expect("failed to run migrations");
    let event_bus = Arc::new(EventBus::new(db.clone()));
    (dir, db, event_bus)
}

#[test]
fn test_concurrent_idempotent_submissions() {
    let (_dir, db, event_bus) = setup_test_system();
    let num_threads = 10;
    let idempotency_key = "idemp-concurrent-race-123".to_string();

    let mut handles = Vec::new();

    for i in 0..num_threads {
        let db_clone = db.clone();
        let eb_clone = event_bus.clone();
        let key_clone = idempotency_key.clone();

        handles.push(thread::spawn(move || {
            let manager = JobManager::with_event_bus(db_clone, eb_clone);
            manager.submit_job(JobSubmission {
                job_type: "concurrent_probe".to_string(),
                payload: json!({ "thread": i, "data": "probe_sample" }).to_string(),
                payload_version: Some(1),
                priority: Some(50),
                idempotency_key: Some(key_clone),
                project_id: None,
                max_retries: Some(3),
            })
        }));
    }

    let mut job_ids = Vec::new();
    for handle in handles {
        let res = handle.join().expect("thread panicked");
        let job = res.expect("submission should succeed");
        job_ids.push(job.id);
    }

    assert_eq!(job_ids.len(), num_threads);
    let first_id = &job_ids[0];
    for id in &job_ids {
        assert_eq!(id, first_id, "All threads must receive the same job ID");
    }

    // Verify exactly 1 job exists in database
    let conn = db.conn().expect("get conn");
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM jobs WHERE idempotency_key = ?1",
            rusqlite::params![idempotency_key],
            |row| row.get(0),
        )
        .expect("count jobs");
    assert_eq!(count, 1, "Exactly one job must be persisted in SQLite");

    // Verify exactly 1 JobQueued event was persisted
    let event_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM domain_events WHERE event_type = 'JobQueued' AND job_id = ?1",
            rusqlite::params![first_id],
            |row| row.get(0),
        )
        .expect("count events");
    assert_eq!(
        event_count, 1,
        "Exactly one JobQueued event must be persisted"
    );
}

#[test]
fn test_concurrent_atomic_leasing() {
    let (_dir, db, event_bus) = setup_test_system();
    let manager = JobManager::with_event_bus(db.clone(), event_bus.clone());

    // Submit single runnable job
    let job = manager
        .submit_job(JobSubmission {
            job_type: "contended_task".to_string(),
            payload: json!({ "contended": true }).to_string(),
            payload_version: Some(1),
            priority: Some(10),
            idempotency_key: None,
            project_id: None,
            max_retries: Some(2),
        })
        .expect("submit job");

    let num_workers = 10;
    let mut handles = Vec::new();

    for i in 0..num_workers {
        let db_clone = db.clone();
        let eb_clone = event_bus.clone();
        let worker_name = format!("worker-{}", i);

        handles.push(thread::spawn(move || {
            let worker_mgr = JobManager::with_worker_and_event_bus(db_clone, eb_clone, worker_name);
            worker_mgr.acquire_next_job(30)
        }));
    }

    let mut successful_leases = Vec::new();
    let mut none_leases = 0;

    for handle in handles {
        let res = handle.join().expect("worker panicked");
        match res.expect("acquire should not fail with db error") {
            Some(leased_job) => successful_leases.push(leased_job),
            None => none_leases += 1,
        }
    }

    assert_eq!(
        successful_leases.len(),
        1,
        "Exactly one worker must successfully lease the runnable job"
    );
    assert_eq!(none_leases, num_workers - 1);
    assert_eq!(successful_leases[0].id, job.id);
    assert_eq!(successful_leases[0].status, JobState::Running);
    assert!(successful_leases[0].lease_token.is_some());
}

#[test]
fn test_stale_lease_rejection_after_reassignment() {
    let (_dir, db, event_bus) = setup_test_system();
    let manager_alpha = JobManager::with_worker_and_event_bus(
        db.clone(),
        event_bus.clone(),
        "worker-alpha".to_string(),
    );
    let manager_beta = JobManager::with_worker_and_event_bus(
        db.clone(),
        event_bus.clone(),
        "worker-beta".to_string(),
    );

    let job = manager_alpha
        .submit_job(JobSubmission {
            job_type: "stale_lease_task".to_string(),
            payload: "{}".to_string(),
            payload_version: Some(1),
            priority: Some(5),
            idempotency_key: None,
            project_id: None,
            max_retries: Some(3),
        })
        .expect("submit job");

    // 1. Worker Alpha acquires lease (Attempt 1)
    let acquired_alpha = manager_alpha
        .acquire_next_job(10)
        .expect("acquire alpha")
        .expect("job available");
    assert_eq!(acquired_alpha.id, job.id);
    assert_eq!(acquired_alpha.leased_by.as_deref(), Some("worker-alpha"));
    let token_alpha = acquired_alpha.lease_token.expect("has lease token");

    // 2. Simulate Worker Alpha crash / timeout by expiring lease in the past
    let past_time = (Utc::now() - Duration::seconds(120)).to_rfc3339();
    {
        let conn = db.conn().expect("get conn");
        conn.execute(
            "UPDATE jobs SET lease_expires_at = ?1 WHERE id = ?2",
            rusqlite::params![past_time, job.id],
        )
        .expect("expire lease");
    }

    // 3. Lease reaper re-queues expired job as retryable
    let reaped = manager_alpha.reap_expired_leases().expect("reap");
    assert_eq!(reaped, 1);

    // 4. Worker Beta acquires lease (Attempt 2)
    let acquired_beta = manager_beta
        .acquire_next_job(30)
        .expect("acquire beta")
        .expect("job available for beta");
    assert_eq!(acquired_beta.id, job.id);
    assert_eq!(acquired_beta.leased_by.as_deref(), Some("worker-beta"));
    let token_beta = acquired_beta.lease_token.expect("has lease token");
    assert_ne!(
        token_alpha, token_beta,
        "Subsequent lease attempts must generate distinct lease tokens"
    );

    // 5. Worker Alpha wakes up late and attempts operations with token_alpha -> ALL MUST BE REJECTED
    // A. Heartbeat
    let hb = manager_alpha
        .heartbeat_with_token(&job.id, Some(&token_alpha), 60)
        .expect("heartbeat call");
    assert!(!hb, "Stale worker heartbeat must fail");

    // B. Record Checkpoint
    let cp_res = manager_alpha.record_checkpoint_with_token(
        &job.id,
        Some(&token_alpha),
        "stale_stage",
        "{}",
    );
    match cp_res {
        Err(JobError::StaleLease(_)) => {}
        other => panic!("Expected StaleLease error, got {:?}", other),
    }

    // C. Complete Job
    let comp_res = manager_alpha.complete_job_with_token(&job.id, Some(&token_alpha), None);
    match comp_res {
        Err(JobError::StaleLease(_)) => {}
        other => panic!("Expected StaleLease error, got {:?}", other),
    }

    // D. Fail Job
    let fail_res =
        manager_alpha.fail_job_with_token(&job.id, Some(&token_alpha), "alpha_fail", false);
    match fail_res {
        Err(JobError::StaleLease(_)) => {}
        other => panic!("Expected StaleLease error, got {:?}", other),
    }

    // 6. Worker Beta performs legitimate operations with token_beta -> MUST SUCCEED
    let cp_beta = manager_beta
        .record_checkpoint_with_token(&job.id, Some(&token_beta), "valid_stage", "{\"step\":1}")
        .expect("valid checkpoint from beta");
    assert_eq!(cp_beta.stage, "valid_stage");

    let completed_beta = manager_beta
        .complete_job_with_token(
            &job.id,
            Some(&token_beta),
            Some("{\"output\":\"done\"}".to_string()),
        )
        .expect("valid complete from beta");
    assert_eq!(completed_beta.status, JobState::Completed);

    // Checkpoints table should only contain beta's checkpoint
    let checkpoints = manager_beta.get_checkpoints(&job.id).expect("get cps");
    assert_eq!(checkpoints.len(), 1);
    assert_eq!(checkpoints[0].stage, "valid_stage");
}

#[test]
fn test_terminal_state_race_cancellation_vs_completion() {
    let (_dir, db, event_bus) = setup_test_system();
    let manager = JobManager::with_event_bus(db.clone(), event_bus.clone());

    let job = manager
        .submit_job(JobSubmission {
            job_type: "cancellation_race_task".to_string(),
            payload: "{}".to_string(),
            payload_version: Some(1),
            priority: Some(1),
            idempotency_key: None,
            project_id: None,
            max_retries: Some(1),
        })
        .expect("submit");

    let leased = manager
        .acquire_next_job(30)
        .expect("acquire")
        .expect("available");
    let lease_token = leased.lease_token.expect("has lease token");

    // User cancels the job while worker is working
    let cancelled = manager.cancel_job(&job.id).expect("cancel");
    assert_eq!(cancelled.status, JobState::Cancelled);

    // Late worker attempts to complete the job
    let comp_res = manager.complete_job_with_token(
        &job.id,
        Some(&lease_token),
        Some("{\"result\":true}".to_string()),
    );
    match comp_res {
        Err(JobError::TerminalState(_)) => {}
        other => panic!(
            "Expected TerminalState error for late completion on cancelled job, got {:?}",
            other
        ),
    }

    // Status remains Cancelled
    let current = manager.get_job(&job.id).expect("get").expect("exists");
    assert_eq!(current.status, JobState::Cancelled);
    assert!(current.result.is_none());
}

#[test]
fn test_transactional_event_persistence_and_rollback() {
    let (_dir, db, _eb) = setup_test_system();

    // Verify manual transaction rollback leaves zero trace in jobs or domain_events
    let mut conn = db.conn().expect("get conn");
    {
        let tx = conn.transaction().expect("begin tx");

        let submission = JobSubmission {
            job_type: "abort_task".to_string(),
            payload: "{}".to_string(),
            payload_version: Some(1),
            priority: Some(0),
            idempotency_key: Some("idemp-will-abort".to_string()),
            project_id: None,
            max_retries: Some(1),
        };

        let (job, is_new) =
            JobRepository::enqueue_with_status(&tx, submission).expect("enqueue inside tx");
        assert!(is_new);

        let evt = DomainEvent::new(
            "JobQueued".to_string(),
            None,
            Some(job.id.clone()),
            json!({ "aborted": true }),
        );
        EventBus::record_event_tx(&tx, &evt).expect("record event inside tx");

        // Explicitly rollback transaction
        tx.rollback().expect("rollback");
    }

    // Verify nothing exists in SQLite
    let job_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM jobs WHERE idempotency_key = 'idemp-will-abort'",
            [],
            |r| r.get(0),
        )
        .expect("query jobs");
    assert_eq!(job_count, 0, "No job row should exist after rollback");

    let event_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM domain_events WHERE payload LIKE '%aborted%'",
            [],
            |r| r.get(0),
        )
        .expect("query events");
    assert_eq!(event_count, 0, "No event row should exist after rollback");
}

#[test]
fn test_payload_version_persistence_and_defaults() {
    let (_dir, db, event_bus) = setup_test_system();
    let manager = JobManager::with_event_bus(db, event_bus);

    // 1. Default payload_version when None is supplied -> must be 1
    let job_default = manager
        .submit_job(JobSubmission {
            job_type: "default_version_task".to_string(),
            payload: "{\"field\":1}".to_string(),
            payload_version: None,
            priority: Some(0),
            idempotency_key: None,
            project_id: None,
            max_retries: Some(1),
        })
        .expect("submit default");
    assert_eq!(job_default.payload_version, 1);

    // 2. Explicit payload_version -> must be preserved
    let job_v2 = manager
        .submit_job(JobSubmission {
            job_type: "v2_schema_task".to_string(),
            payload: "{\"version\":2}".to_string(),
            payload_version: Some(2),
            priority: Some(0),
            idempotency_key: None,
            project_id: None,
            max_retries: Some(1),
        })
        .expect("submit v2");
    assert_eq!(job_v2.payload_version, 2);

    let retrieved = manager.get_job(&job_v2.id).expect("get").expect("exists");
    assert_eq!(retrieved.payload_version, 2);
}

#[test]
fn test_repeated_lease_expiry_cannot_loop_past_max_retries() {
    let (_dir, db, event_bus) = setup_test_system();
    let manager = JobManager::with_worker_and_event_bus(
        db.clone(),
        event_bus.clone(),
        "worker-test".to_string(),
    );

    // Job with max_retries = 2 (can attempt 1 initial + 2 retries = 3 attempts total)
    let job = manager
        .submit_job(JobSubmission {
            job_type: "finite_retry_task".to_string(),
            payload: "{}".to_string(),
            payload_version: Some(1),
            priority: Some(10),
            idempotency_key: None,
            project_id: None,
            max_retries: Some(2),
        })
        .expect("submit");

    // Attempt 1: Acquire lease (for 1 second)
    let leased_1 = manager
        .acquire_next_job(1)
        .expect("acquire 1")
        .expect("leased 1");
    assert_eq!(leased_1.id, job.id);
    assert_eq!(leased_1.retry_count, 0);

    // Sleep to expire Attempt 1
    std::thread::sleep(std::time::Duration::from_millis(1200));

    // Direct acquire_lease without reaper MUST return None (cannot bypass reaper/retry counting)
    {
        let conn = db.conn().expect("conn");
        let direct = openvideoai_lib::jobs::repository::JobRepository::acquire_lease(
            &conn,
            "worker-test",
            10,
        )
        .expect("acquire");
        assert!(
            direct.is_none(),
            "Expired running job cannot be directly acquired without reaper"
        );
    }

    // Reaper reaps Attempt 1 -> retry_count becomes 1, status = retryable
    let reaped_1 = manager.reap_expired_leases().expect("reap 1");
    assert_eq!(reaped_1, 1);
    let job_after_1 = manager.get_job(&job.id).unwrap().unwrap();
    assert_eq!(job_after_1.status, JobState::Retryable);
    assert_eq!(job_after_1.retry_count, 1);

    // Attempt 2: Acquire lease (for 1 second)
    let leased_2 = manager
        .acquire_next_job(1)
        .expect("acquire 2")
        .expect("leased 2");
    assert_eq!(leased_2.id, job.id);
    assert_eq!(leased_2.retry_count, 1);

    // Sleep to expire Attempt 2
    std::thread::sleep(std::time::Duration::from_millis(1200));

    // Direct acquire_lease again MUST return None
    {
        let conn = db.conn().expect("conn");
        let direct = openvideoai_lib::jobs::repository::JobRepository::acquire_lease(
            &conn,
            "worker-test",
            10,
        )
        .expect("acquire");
        assert!(direct.is_none());
    }

    // Reaper reaps Attempt 2 -> retry_count becomes 2, status = retryable
    let reaped_2 = manager.reap_expired_leases().expect("reap 2");
    assert_eq!(reaped_2, 1);
    let job_after_2 = manager.get_job(&job.id).unwrap().unwrap();
    assert_eq!(job_after_2.status, JobState::Retryable);
    assert_eq!(job_after_2.retry_count, 2);

    // Attempt 3: Acquire lease (for 1 second)
    let leased_3 = manager
        .acquire_next_job(1)
        .expect("acquire 3")
        .expect("leased 3");
    assert_eq!(leased_3.id, job.id);
    assert_eq!(leased_3.retry_count, 2);

    // Sleep to expire Attempt 3
    std::thread::sleep(std::time::Duration::from_millis(1200));

    // Direct acquire_lease again MUST return None
    {
        let conn = db.conn().expect("conn");
        let direct = openvideoai_lib::jobs::repository::JobRepository::acquire_lease(
            &conn,
            "worker-test",
            10,
        )
        .expect("acquire");
        assert!(direct.is_none());
    }

    // Reaper reaps Attempt 3 -> retry_count (2) == max_retries (2) -> DEAD LETTER!
    let reaped_3 = manager.reap_expired_leases().expect("reap 3");
    assert_eq!(reaped_3, 1);
    let job_final = manager.get_job(&job.id).unwrap().unwrap();
    assert_eq!(
        job_final.status,
        JobState::DeadLetter,
        "Job must transition to dead_letter once max_retries is reached"
    );
    assert_eq!(job_final.retry_count, 2);
    assert!(job_final.completed_at.is_some());

    // Subsequent acquisitions must find NO jobs (cannot loop infinitely)
    let next = manager.acquire_next_job(10).expect("acquire next");
    assert!(
        next.is_none(),
        "Dead letter job must never be acquired again"
    );
}

#[test]
fn test_expired_lease_token_fencing_without_reaper() {
    let (_dir, db, event_bus) = setup_test_system();
    let manager = JobManager::with_worker_and_event_bus(
        db.clone(),
        event_bus.clone(),
        "worker-alpha".to_string(),
    );

    let job = manager
        .submit_job(JobSubmission {
            job_type: "lease_fence_task".to_string(),
            payload: "{}".to_string(),
            payload_version: Some(1),
            priority: Some(5),
            idempotency_key: None,
            project_id: None,
            max_retries: Some(3),
        })
        .expect("submit");

    // Worker Alpha acquires lease for 1 second
    let leased = manager
        .acquire_next_job(1)
        .expect("acquire")
        .expect("leased");
    let token = leased.lease_token.expect("lease token");

    // Wait for lease to expire (1.2 seconds)
    std::thread::sleep(std::time::Duration::from_millis(1200));

    // CRITICAL: We do NOT run the reaper! Status in DB is still 'running', and lease_token matches.
    // However, lease_expires_at has passed. All operations by Worker Alpha MUST be rejected!

    // 1. Heartbeat must fail and not extend the lease
    let hb_result = manager
        .heartbeat_with_token(&job.id, Some(&token), 60)
        .expect("heartbeat");
    assert!(
        !hb_result,
        "Heartbeat must return false when lease has expired"
    );

    // 2. Checkpoint must be rejected
    let cp_result = manager.record_checkpoint_with_token(&job.id, Some(&token), "stage_1", "{}");
    assert!(
        matches!(
            cp_result,
            Err(openvideoai_lib::jobs::JobError::StaleLease(_))
        ),
        "Checkpoint with expired lease must return StaleLease: {:?}",
        cp_result
    );

    // 3. Complete must be rejected
    let complete_result =
        manager.complete_job_with_token(&job.id, Some(&token), Some("{}".to_string()));
    assert!(
        matches!(
            complete_result,
            Err(openvideoai_lib::jobs::JobError::StaleLease(_))
        ),
        "Completion with expired lease must return StaleLease: {:?}",
        complete_result
    );

    // 4. Fail must be rejected
    let fail_result = manager.fail_job_with_token(&job.id, Some(&token), "Late crash", true);
    assert!(
        matches!(
            fail_result,
            Err(openvideoai_lib::jobs::JobError::StaleLease(_))
        ),
        "Failure with expired lease must return StaleLease: {:?}",
        fail_result
    );

    // 5. Verify the job was NOT silently revived or completed
    let current_job = manager.get_job(&job.id).unwrap().unwrap();
    assert_eq!(
        current_job.status,
        JobState::Running,
        "Status must remain unmodified by expired worker"
    );
    assert!(current_job.completed_at.is_none());
}

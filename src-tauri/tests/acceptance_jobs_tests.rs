use chrono::{Duration, Utc};
use openvideoai_lib::db::DbManager;
use openvideoai_lib::events::bus::EventBus;
use openvideoai_lib::jobs::manager::JobManager;
use openvideoai_lib::jobs::model::{JobState, JobSubmission};
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tempfile::tempdir;

#[test]
fn test_phase2_durable_jobs_events_and_crash_recovery_acceptance() {
    let dir = tempdir().expect("failed to create tempdir");
    let db_path = dir.path().join("phase2_acceptance.db");

    // 1. Initialize DB and run all migrations (including Migration 3)
    let db_manager = Arc::new(DbManager::new(db_path.clone()).expect("init db"));
    db_manager.run_migrations().expect("run migrations 1, 2, 3");

    // 2. Initialize EventBus and subscribe test collector
    let event_bus = Arc::new(EventBus::new(db_manager.clone()));
    let received_event_count = Arc::new(AtomicUsize::new(0));
    let counter_clone = received_event_count.clone();

    event_bus.subscribe(Arc::new(move |_evt| {
        counter_clone.fetch_add(1, Ordering::SeqCst);
    }));

    // 3. Initialize JobManager with specific worker identity and shared event bus
    let job_manager = Arc::new(JobManager::with_worker_and_event_bus(
        db_manager.clone(),
        event_bus.clone(),
        "worker-alpha".to_string(),
    ));

    // 4. Test Idempotency & Deduplication
    let sub1 = JobSubmission {
        job_type: "video_probe".to_string(),
        payload: json!({ "file": "clip_4k.mp4" }).to_string(),
        payload_version: Some(1),
        priority: Some(50),
        idempotency_key: Some("idemp-unique-key-101".to_string()),
        project_id: None,
        max_retries: Some(1),
    };

    let job1 = job_manager
        .submit_job(sub1)
        .expect("first submit should succeed");
    assert_eq!(job1.job_type, "video_probe");
    assert_eq!(job1.status, JobState::Queued);
    assert_eq!(job1.priority, 50);

    // Re-submit identical idempotency key
    let sub1_dup = JobSubmission {
        job_type: "video_probe".to_string(),
        payload: json!({ "file": "different_payload_ignored.mp4" }).to_string(),
        payload_version: None,
        priority: Some(99),
        idempotency_key: Some("idemp-unique-key-101".to_string()),
        project_id: None,
        max_retries: Some(5),
    };

    let job1_dedup = job_manager
        .submit_job(sub1_dup)
        .expect("dedup submit should succeed");
    assert_eq!(
        job1_dedup.id, job1.id,
        "Idempotent submission must return the exact existing job"
    );
    assert_eq!(job1_dedup.priority, 50, "Priority must remain unchanged");

    // Verify exactly 1 job in repository
    let all_jobs = job_manager.list_jobs(None, None, 50).expect("list jobs");
    assert_eq!(all_jobs.len(), 1);

    // 5. Submit higher and lower priority jobs to test priority dequeue order
    let sub_high = JobSubmission {
        job_type: "audio_transcribe".to_string(),
        payload: json!({ "audio": "interview.wav" }).to_string(),
        payload_version: None,
        priority: Some(100),
        idempotency_key: Some("idemp-high-100".to_string()),
        project_id: None,
        max_retries: Some(3),
    };
    let job_high = job_manager
        .submit_job(sub_high)
        .expect("submit high priority job");

    let sub_low = JobSubmission {
        job_type: "export_summary".to_string(),
        payload: json!({ "summary": true }).to_string(),
        payload_version: None,
        priority: Some(10),
        idempotency_key: Some("idemp-low-10".to_string()),
        project_id: None,
        max_retries: Some(1),
    };
    let _job_low = job_manager
        .submit_job(sub_low)
        .expect("submit low priority job");

    // 6. Worker Alpha acquires lease - MUST acquire highest priority job first (Priority 100)
    let acquired1 = job_manager
        .acquire_next_job(30)
        .expect("acquire next job")
        .expect("a job should be available");

    assert_eq!(
        acquired1.id, job_high.id,
        "Worker must acquire highest priority job (100) first"
    );
    assert_eq!(acquired1.status, JobState::Running);
    assert_eq!(acquired1.leased_by.as_deref(), Some("worker-alpha"));
    assert!(acquired1.lease_expires_at.is_some());
    assert!(acquired1.started_at.is_some());

    // 7. Worker Alpha extends heartbeat
    let original_lease = acquired1.lease_expires_at.clone().unwrap();
    let extended = job_manager
        .heartbeat(&acquired1.id, 60)
        .expect("heartbeat should succeed");
    assert!(extended, "Heartbeat extension must return true");

    let refreshed_job = job_manager
        .get_job(&acquired1.id)
        .expect("get job")
        .expect("job exists");
    assert!(
        refreshed_job.lease_expires_at.unwrap() > original_lease,
        "Heartbeat must extend lease expiration"
    );

    // 8. Worker Alpha records checkpoints during execution
    let cp1 = job_manager
        .record_checkpoint(
            &acquired1.id,
            "preprocess_audio",
            &json!({ "progress": 25, "normalized": true }).to_string(),
        )
        .expect("record cp1");
    assert_eq!(cp1.stage, "preprocess_audio");

    let cp2 = job_manager
        .record_checkpoint(
            &acquired1.id,
            "whisper_inference",
            &json!({ "progress": 75, "tokens": 120 }).to_string(),
        )
        .expect("record cp2");
    assert_eq!(cp2.stage, "whisper_inference");

    let checkpoints = job_manager
        .get_checkpoints(&acquired1.id)
        .expect("get checkpoints");
    assert_eq!(checkpoints.len(), 2);
    assert_eq!(checkpoints[0].stage, "preprocess_audio");
    assert_eq!(checkpoints[1].stage, "whisper_inference");

    // 9. Worker Alpha completes job successfully
    let completed = job_manager
        .complete_job(
            &acquired1.id,
            Some(json!({ "text": "Welcome to OpenVideoAI", "confidence": 0.98 }).to_string()),
        )
        .expect("complete job");
    assert_eq!(completed.status, JobState::Completed);
    assert!(completed.result.is_some());
    assert!(completed.completed_at.is_some());

    // 10. Worker Beta acquires next highest priority (Job 1, Priority 50)
    let worker_beta_manager =
        JobManager::with_worker_id(db_manager.clone(), "worker-beta".to_string());
    let acquired2 = worker_beta_manager
        .acquire_next_job(30)
        .expect("acquire next")
        .expect("job 1 should be available");
    assert_eq!(acquired2.id, job1.id);
    assert_eq!(acquired2.leased_by.as_deref(), Some("worker-beta"));

    // 11. Simulate Worker Beta crash by expiring its lease in the past
    let past_time = (Utc::now() - Duration::seconds(60)).to_rfc3339();
    {
        let conn = db_manager.conn().expect("get conn");
        conn.execute(
            "UPDATE jobs SET lease_expires_at = ?1 WHERE id = ?2",
            rusqlite::params![past_time, acquired2.id],
        )
        .expect("simulate worker crash");
    }

    // 12. Crash Recovery Engine: reap expired lease (Retry 1/2)
    let reaped_count = job_manager.reap_expired_leases().expect("reap leases");
    assert_eq!(reaped_count, 1, "Should reap exactly 1 crashed worker job");

    let recovered_job = job_manager
        .get_job(&acquired2.id)
        .expect("get job")
        .expect("job exists");
    assert_eq!(
        recovered_job.status,
        JobState::Retryable,
        "Job with retries remaining should be re-queued as retryable"
    );
    assert_eq!(recovered_job.retry_count, 1);
    assert!(recovered_job.leased_by.is_none());

    // 13. Simulate repeated worker crash to exhaust retries (Max retries = 2)
    let worker_gamma_manager =
        JobManager::with_worker_id(db_manager.clone(), "worker-gamma".to_string());
    let acquired2_again = worker_gamma_manager
        .acquire_next_job(30)
        .expect("acquire again")
        .expect("job available");

    // Crash again
    {
        let conn = db_manager.conn().expect("get conn");
        conn.execute(
            "UPDATE jobs SET lease_expires_at = ?1 WHERE id = ?2",
            rusqlite::params![past_time, acquired2_again.id],
        )
        .expect("simulate second worker crash");
    }

    // Reap again (Retries exhausted -> DeadLetter)
    let reaped_count2 = job_manager
        .reap_expired_leases()
        .expect("reap leases again");
    assert_eq!(reaped_count2, 1);

    let dead_letter_job = job_manager
        .get_job(&acquired2.id)
        .expect("get dead letter job")
        .expect("job exists");
    assert_eq!(
        dead_letter_job.status,
        JobState::DeadLetter,
        "Job that exhausts max retries must transition to DeadLetter"
    );
    assert_eq!(dead_letter_job.retry_count, 1);

    // 14. Verify Event Bus persisted all events to SQLite and notified subscriber
    let all_events = event_bus
        .list_events(None, None, 50)
        .expect("list all events");
    assert!(
        all_events.len() >= 2,
        "All published domain events must be persisted to SQLite"
    );
    assert!(
        received_event_count.load(Ordering::SeqCst) >= 2,
        "In-memory subscribers must be notified"
    );

    let completed_events = event_bus
        .list_events(Some("JobCompleted"), None, 50)
        .expect("filter by event type");
    assert_eq!(completed_events.len(), 1);
    assert_eq!(completed_events[0].event_type, "JobCompleted");

    // 15. Verify Persistence across DB Reopen
    drop(worker_gamma_manager);
    drop(worker_beta_manager);
    drop(job_manager);
    drop(event_bus);
    drop(db_manager);

    // Reopen database connection
    let db_manager2 = Arc::new(DbManager::new(db_path).expect("reopen db"));
    let job_manager2 = JobManager::new(db_manager2.clone());
    let event_bus2 = EventBus::new(db_manager2);

    let reloaded_completed = job_manager2
        .get_job(&acquired1.id)
        .expect("get completed job")
        .expect("job exists");
    assert_eq!(reloaded_completed.status, JobState::Completed);

    let reloaded_dead = job_manager2
        .get_job(&acquired2.id)
        .expect("get dead job")
        .expect("job exists");
    assert_eq!(reloaded_dead.status, JobState::DeadLetter);

    let reloaded_checkpoints = job_manager2
        .get_checkpoints(&acquired1.id)
        .expect("get checkpoints");
    assert_eq!(reloaded_checkpoints.len(), 2);

    let reloaded_events = event_bus2
        .list_events(None, None, 50)
        .expect("get persisted events");
    assert!(reloaded_events.len() >= 2);
}

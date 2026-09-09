use openvideoai_lib::db::DbManager;
use openvideoai_lib::events::bus::EventBus;
use openvideoai_lib::events::model::DomainEvent;
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tempfile::tempdir;

fn setup_test_event_bus() -> (tempfile::TempDir, EventBus) {
    let dir = tempdir().expect("failed to create tempdir");
    let db_path = dir.path().join("events_test.db");
    let db = Arc::new(DbManager::new(db_path).expect("failed to init db"));
    db.run_migrations().expect("failed to run migrations");
    let bus = EventBus::new(db);
    (dir, bus)
}

#[test]
fn test_publish_and_persist_domain_event() {
    let (_dir, bus) = setup_test_event_bus();

    let event = DomainEvent::new(
        "JobQueued".to_string(),
        Some("proj-123".to_string()),
        Some("job-abc".to_string()),
        json!({"jobType": "proxy_gen", "priority": 10}),
    );

    bus.publish(event.clone()).expect("publish failed");

    let events = bus.list_events(None, None, 10).expect("list failed");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_id, event.event_id);
    assert_eq!(events[0].event_type, "JobQueued");
    assert_eq!(events[0].project_id.as_deref(), Some("proj-123"));
    assert_eq!(events[0].job_id.as_deref(), Some("job-abc"));
    assert_eq!(events[0].payload["jobType"], "proxy_gen");
}

#[test]
fn test_event_bus_notifies_subscribers() {
    let (_dir, bus) = setup_test_event_bus();

    let call_count = Arc::new(AtomicUsize::new(0));
    let counter_clone = call_count.clone();

    bus.subscribe(Arc::new(move |_evt| {
        counter_clone.fetch_add(1, Ordering::SeqCst);
    }));

    bus.publish(DomainEvent::new(
        "JobStarted".to_string(),
        None,
        Some("job-1".to_string()),
        json!({"worker": "worker-1"}),
    ))
    .unwrap();

    bus.publish(DomainEvent::new(
        "JobCompleted".to_string(),
        None,
        Some("job-1".to_string()),
        json!({"durationMs": 500}),
    ))
    .unwrap();

    assert_eq!(call_count.load(Ordering::SeqCst), 2);
}

#[test]
fn test_event_filtering_by_type_and_job() {
    let (_dir, bus) = setup_test_event_bus();

    bus.publish(DomainEvent::new(
        "JobQueued".to_string(),
        None,
        Some("job-1".to_string()),
        json!({}),
    ))
    .unwrap();

    bus.publish(DomainEvent::new(
        "JobQueued".to_string(),
        None,
        Some("job-2".to_string()),
        json!({}),
    ))
    .unwrap();

    bus.publish(DomainEvent::new(
        "JobCompleted".to_string(),
        None,
        Some("job-1".to_string()),
        json!({}),
    ))
    .unwrap();

    // Filter by event_type
    let queued = bus.list_events(Some("JobQueued"), None, 10).unwrap();
    assert_eq!(queued.len(), 2);

    // Filter by job_id
    let job1_evts = bus.list_events(None, Some("job-1"), 10).unwrap();
    assert_eq!(job1_evts.len(), 2);

    // Filter by both
    let job1_completed = bus
        .list_events(Some("JobCompleted"), Some("job-1"), 10)
        .unwrap();
    assert_eq!(job1_completed.len(), 1);
}

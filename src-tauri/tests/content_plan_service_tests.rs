use std::sync::{Arc, Mutex};
use tempfile::tempdir;

use openvideoai_lib::content_planning::error::ContentPlanningError;
use openvideoai_lib::content_planning::model::*;
use openvideoai_lib::content_planning::service::ContentPlanService;
use openvideoai_lib::db::DbManager;
use openvideoai_lib::events::bus::EventBus;
use openvideoai_lib::projects::model::Project;
use openvideoai_lib::projects::repository::ProjectRepository;

fn setup_service() -> (
    tempfile::TempDir,
    Arc<DbManager>,
    Arc<EventBus>,
    ContentPlanService,
) {
    let dir = tempdir().expect("failed to create tempdir");
    let db_path = dir.path().join("content_plan_service_test.db");
    let db = Arc::new(DbManager::new(db_path).expect("failed to init db"));
    db.run_migrations().expect("failed to run migrations");

    let project = Project {
        id: "p_1".to_string(),
        name: "Test Project".to_string(),
        description: Some("Description".to_string()),
        created_at: "2026-09-12T10:00:00Z".to_string(),
        updated_at: "2026-09-12T10:00:00Z".to_string(),
    };
    {
        let conn = db.conn().unwrap();
        ProjectRepository::create(&conn, &project).unwrap();
    }

    let event_bus = Arc::new(EventBus::new(db.clone()));
    let service = ContentPlanService::new(db.clone(), event_bus.clone());
    (dir, db, event_bus, service)
}

fn sample_source(source_id: &str, snapshot: &str) -> PlanningSource {
    PlanningSource {
        source_id: source_id.to_string(),
        source_type: "document".to_string(),
        title: format!("Source {source_id}"),
        author: Some("Author".to_string()),
        url: Some("https://example.com".to_string()),
        published_at: Some("2026-09-01T00:00:00Z".to_string()),
        locator: Some("p.1".to_string()),
        content_snapshot: snapshot.to_string(),
        content_hash: format!("hash_{snapshot}"),
    }
}

fn sample_plan(id: &str, project_id: &str, revision: i64) -> ContentPlan {
    ContentPlan {
        id: id.to_string(),
        project_id: project_id.to_string(),
        revision,
        status: PlanStatus::Draft,
        title: format!("Plan {id}"),
        summary: "Plan summary".to_string(),
        language: "en".to_string(),
        tone: "informative".to_string(),
        target_duration_ms: 60_000,
        estimated_duration_ms: 60_000,
        full_narration: "Narration for scene 1".to_string(),
        scenes: vec![ContentScene {
            id: "scene_1".to_string(),
            order: 1,
            title: "Scene 1".to_string(),
            narration: "Narration for scene 1".to_string(),
            duration_estimate_ms: 60_000,
            visual_description: "Visual description for scene 1".to_string(),
            media_intent: vec![MediaIntent::StockVideo],
            stock_search_queries: vec!["topic".to_string()],
            image_prompt: None,
            video_prompt: None,
            claims: vec![Claim {
                id: "claim_1".to_string(),
                text: "Narration for scene 1".to_string(),
                source_refs: vec![SourceRef {
                    source_id: "src_1".to_string(),
                    locator: Some("p.1".to_string()),
                    excerpt_hash: Some("hash_snap".to_string()),
                    note: None,
                }],
                grounding_status: GroundingStatus::Supported,
                grounding_valid: true,
                note: None,
            }],
            source_refs: vec![],
        }],
        source_refs: vec![],
        generation_metadata: Some(GenerationMetadata {
            provider: "mock".to_string(),
            model: "mock-planner-v1".to_string(),
            primary_model: Some("mock-planner-v1".to_string()),
            fallback_used: Some(false),
            planner_profile_version: 1,
            prompt_template_version: 1,
            generated_at: "2026-09-12T10:00:00Z".to_string(),
            usage: GenerationUsage::default(),
            repaired_calls: None,
        }),
        created_at: "2026-09-12T10:00:00Z".to_string(),
        updated_at: "2026-09-12T10:00:00Z".to_string(),
    }
}

#[test]
fn test_publish_generated_revision_initial() {
    let (_dir, _db, event_bus, service) = setup_service();
    let events_received = Arc::new(Mutex::new(Vec::new()));
    let events_cb = events_received.clone();
    event_bus.subscribe(Arc::new(move |evt| {
        events_cb.lock().unwrap().push(evt.clone());
    }));

    let plan = sample_plan("cp_1", "p_1", 1);
    let sources = vec![sample_source("src_1", "Snapshot 1")];

    let published = service
        .publish_generated_revision(
            "cp_1",
            None,
            plan,
            sources,
            "generate_initial",
            "planner_worker",
            Some("job_100"),
        )
        .expect("publish initial failed");

    assert_eq!(published.id, "cp_1");
    assert_eq!(published.revision, 1);
    assert_eq!(published.status, PlanStatus::Draft);
    assert_eq!(published.project_id, "p_1");

    // Verify retrieval via service
    let current = service
        .get_plan("cp_1")
        .unwrap()
        .expect("plan should exist");
    assert_eq!(current.id, "cp_1");
    assert_eq!(current.revision, 1);
    assert_eq!(current.status, PlanStatus::Draft);

    let rev1 = service
        .get_plan_revision("cp_1", 1)
        .unwrap()
        .expect("rev 1 should exist");
    assert_eq!(rev1.revision, 1);

    // Verify domain event persisted
    let persisted_events = event_bus
        .list_events(Some("content_plan.created"), None, 10)
        .unwrap();
    assert_eq!(persisted_events.len(), 1);
    assert_eq!(persisted_events[0].event_type, "content_plan.created");
    assert_eq!(persisted_events[0].project_id.as_deref(), Some("p_1"));
    assert_eq!(persisted_events[0].job_id.as_deref(), Some("job_100"));
    assert_eq!(persisted_events[0].payload["planId"], "cp_1");
    assert_eq!(persisted_events[0].payload["revision"], 1);
    assert_eq!(
        persisted_events[0].payload["operationType"],
        "generate_initial"
    );
    assert_eq!(persisted_events[0].payload["status"], "draft");

    // Verify broadcast received
    let broadcasted = events_received.lock().unwrap();
    assert_eq!(broadcasted.len(), 1);
    assert_eq!(broadcasted[0].event_type, "content_plan.created");
}

#[test]
fn test_save_manual_revision_draft() {
    let (_dir, _db, event_bus, service) = setup_service();
    let plan = sample_plan("cp_1", "p_1", 1);
    let sources = vec![sample_source("src_1", "Snapshot 1")];
    service
        .publish_generated_revision(
            "cp_1",
            None,
            plan,
            sources,
            "generate_initial",
            "worker",
            None,
        )
        .unwrap();

    let mut edited = service.get_plan("cp_1").unwrap().unwrap();
    edited.scenes[0].narration = "User manually edited narration".to_string();

    let saved = service
        .save_manual_revision("cp_1", 1, edited)
        .expect("manual save failed");

    assert_eq!(saved.revision, 2);
    assert_eq!(saved.status, PlanStatus::Draft);
    assert_eq!(
        saved.scenes[0].claims[0].grounding_status,
        GroundingStatus::Modified
    );
    assert!(!saved.scenes[0].claims[0].grounding_valid);

    // Verify current is revision 2
    let current = service.get_plan("cp_1").unwrap().unwrap();
    assert_eq!(current.revision, 2);

    // Verify rev 1 still exists intact
    let rev1 = service.get_plan_revision("cp_1", 1).unwrap().unwrap();
    assert_eq!(rev1.revision, 1);
    assert_eq!(
        rev1.scenes[0].claims[0].grounding_status,
        GroundingStatus::Supported
    );

    // Verify domain event
    let events = event_bus
        .list_events(Some("content_plan.revision_created"), None, 10)
        .unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].payload["planId"], "cp_1");
    assert_eq!(events[0].payload["revision"], 2);
    assert_eq!(events[0].payload["parentRevision"], 1);
    assert_eq!(events[0].payload["operationType"], "manual_edit");
    assert_eq!(events[0].payload["status"], "draft");
}

#[test]
fn test_save_manual_revision_conflict() {
    let (_dir, _db, _eb, service) = setup_service();
    let plan = sample_plan("cp_1", "p_1", 1);
    let sources = vec![sample_source("src_1", "Snapshot 1")];
    service
        .publish_generated_revision(
            "cp_1",
            None,
            plan,
            sources,
            "generate_initial",
            "worker",
            None,
        )
        .unwrap();

    let edited = service.get_plan("cp_1").unwrap().unwrap();

    // Pass stale expected revision 99 when actual is 1
    let err = service
        .save_manual_revision("cp_1", 99, edited)
        .expect_err("should fail on stale expected_revision");

    match err {
        ContentPlanningError::ContentPlanRevisionConflict {
            plan_id,
            expected,
            actual,
        } => {
            assert_eq!(plan_id, "cp_1");
            assert_eq!(expected, 99);
            assert_eq!(actual, 1);
        }
        other => panic!("expected ContentPlanRevisionConflict, got {other:?}"),
    }
}

#[test]
fn test_save_manual_revision_approved_plan() {
    let (_dir, _db, _eb, service) = setup_service();
    let plan = sample_plan("cp_1", "p_1", 1);
    let sources = vec![sample_source("src_1", "Snapshot 1")];
    service
        .publish_generated_revision(
            "cp_1",
            None,
            plan,
            sources,
            "generate_initial",
            "worker",
            None,
        )
        .unwrap();

    // Approve plan revision 1
    service.approve("cp_1", 1).expect("approve failed");
    let approved = service.get_plan("cp_1").unwrap().unwrap();
    assert_eq!(approved.status, PlanStatus::Approved);

    // Save manual revision over approved plan
    let edited = approved.clone();
    let saved = service
        .save_manual_revision("cp_1", 1, edited)
        .expect("save manual over approved failed");

    assert_eq!(saved.revision, 2);
    assert_eq!(saved.status, PlanStatus::Draft);

    let current = service.get_plan("cp_1").unwrap().unwrap();
    assert_eq!(current.revision, 2);
    assert_eq!(current.status, PlanStatus::Draft);

    // Verify summary keeps approved_revision = Some(1)
    let summaries = service.list_project_plans("p_1", false).unwrap();
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].current_revision, 2);
    assert_eq!(summaries[0].status, PlanStatus::Draft);
    assert_eq!(summaries[0].approved_revision, Some(1));
    assert!(summaries[0].approved_at.is_some());
}

#[test]
fn test_save_manual_revision_archived_plan() {
    let (_dir, _db, _eb, service) = setup_service();
    let plan = sample_plan("cp_1", "p_1", 1);
    let sources = vec![sample_source("src_1", "Snapshot 1")];
    service
        .publish_generated_revision(
            "cp_1",
            None,
            plan,
            sources,
            "generate_initial",
            "worker",
            None,
        )
        .unwrap();

    service.archive("cp_1", 1).expect("archive failed");

    let edited = sample_plan("cp_1", "p_1", 1);
    let err = service
        .save_manual_revision("cp_1", 1, edited)
        .expect_err("should reject editing archived plan");

    match err {
        ContentPlanningError::ContentPlanArchived(msg) => {
            assert!(msg.contains("archived"));
        }
        other => panic!("expected ContentPlanArchived, got {other:?}"),
    }
}

#[test]
fn test_approve_success() {
    let (_dir, _db, event_bus, service) = setup_service();
    let plan = sample_plan("cp_1", "p_1", 1);
    let sources = vec![sample_source("src_1", "Snapshot 1")];
    service
        .publish_generated_revision(
            "cp_1",
            None,
            plan,
            sources,
            "generate_initial",
            "worker",
            None,
        )
        .unwrap();

    let summary = service.approve("cp_1", 1).expect("approve failed");
    assert_eq!(summary.id, "cp_1");
    assert_eq!(summary.status, PlanStatus::Approved);
    assert_eq!(summary.approved_revision, Some(1));
    assert!(summary.approved_at.is_some());

    let current = service.get_plan("cp_1").unwrap().unwrap();
    assert_eq!(current.status, PlanStatus::Approved);

    // Verify domain event
    let events = event_bus
        .list_events(Some("content_plan.approved"), None, 10)
        .unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].payload["planId"], "cp_1");
    assert_eq!(events[0].payload["approvedRevision"], 1);
    assert!(events[0].payload["approvedAt"].is_string());
}

#[test]
fn test_approve_out_of_tolerance() {
    let (_dir, _db, _eb, service) = setup_service();
    let mut plan = sample_plan("cp_1", "p_1", 1);
    plan.target_duration_ms = 60_000;
    // Set scene duration to 100_000 ms (> 15% tolerance of 60_000)
    plan.scenes[0].duration_estimate_ms = 100_000;
    let sources = vec![sample_source("src_1", "Snapshot 1")];

    service
        .publish_generated_revision(
            "cp_1",
            None,
            plan,
            sources,
            "generate_initial",
            "worker",
            None,
        )
        .unwrap();

    let err = service
        .approve("cp_1", 1)
        .expect_err("should fail when duration is out of tolerance");

    match err {
        ContentPlanningError::ContentPlanValidationFailed(msg) => {
            assert!(msg.contains("tolerance") || msg.contains("outside acceptable tolerance"));
        }
        other => panic!("expected ContentPlanValidationFailed, got {other:?}"),
    }
}

#[test]
fn test_approve_conflict() {
    let (_dir, _db, _eb, service) = setup_service();
    let plan = sample_plan("cp_1", "p_1", 1);
    let sources = vec![sample_source("src_1", "Snapshot 1")];
    service
        .publish_generated_revision(
            "cp_1",
            None,
            plan,
            sources,
            "generate_initial",
            "worker",
            None,
        )
        .unwrap();

    let err = service
        .approve("cp_1", 99)
        .expect_err("should fail on stale expected_revision");

    match err {
        ContentPlanningError::ContentPlanRevisionConflict {
            plan_id,
            expected,
            actual,
        } => {
            assert_eq!(plan_id, "cp_1");
            assert_eq!(expected, 99);
            assert_eq!(actual, 1);
        }
        other => panic!("expected ContentPlanRevisionConflict, got {other:?}"),
    }
}

#[test]
fn test_archive_success() {
    let (_dir, _db, event_bus, service) = setup_service();
    let plan = sample_plan("cp_1", "p_1", 1);
    let sources = vec![sample_source("src_1", "Snapshot 1")];
    service
        .publish_generated_revision(
            "cp_1",
            None,
            plan,
            sources,
            "generate_initial",
            "worker",
            None,
        )
        .unwrap();

    let summary = service.archive("cp_1", 1).expect("archive failed");
    assert_eq!(summary.id, "cp_1");
    assert_eq!(summary.status, PlanStatus::Archived);

    let current = service.get_plan("cp_1").unwrap().unwrap();
    assert_eq!(current.status, PlanStatus::Archived);

    // Verify domain event
    let events = event_bus
        .list_events(Some("content_plan.archived"), None, 10)
        .unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].payload["planId"], "cp_1");
    assert_eq!(events[0].payload["revision"], 1);
    assert!(events[0].payload["archivedAt"].is_string());
}

#[test]
fn test_archive_conflict() {
    let (_dir, _db, _eb, service) = setup_service();
    let plan = sample_plan("cp_1", "p_1", 1);
    let sources = vec![sample_source("src_1", "Snapshot 1")];
    service
        .publish_generated_revision(
            "cp_1",
            None,
            plan,
            sources,
            "generate_initial",
            "worker",
            None,
        )
        .unwrap();

    let err = service
        .archive("cp_1", 42)
        .expect_err("should fail on stale expected_revision");

    match err {
        ContentPlanningError::ContentPlanRevisionConflict {
            plan_id,
            expected,
            actual,
        } => {
            assert_eq!(plan_id, "cp_1");
            assert_eq!(expected, 42);
            assert_eq!(actual, 1);
        }
        other => panic!("expected ContentPlanRevisionConflict, got {other:?}"),
    }
}

#[test]
fn test_publish_generated_regeneration() {
    let (_dir, _db, event_bus, service) = setup_service();
    let plan = sample_plan("cp_1", "p_1", 1);
    let sources = vec![sample_source("src_1", "Snapshot 1")];
    service
        .publish_generated_revision(
            "cp_1",
            None,
            plan,
            sources.clone(),
            "generate_initial",
            "planner_worker",
            Some("job_1"),
        )
        .unwrap();

    let mut candidate2 = sample_plan("cp_1", "p_1", 1);
    candidate2.title = "Regenerated Title".to_string();

    let published2 = service
        .publish_generated_revision(
            "cp_1",
            Some(1),
            candidate2,
            sources,
            "regenerate",
            "planner_worker",
            Some("job_2"),
        )
        .expect("regeneration publish failed");

    assert_eq!(published2.revision, 2);
    assert_eq!(published2.title, "Regenerated Title");

    let current = service.get_plan("cp_1").unwrap().unwrap();
    assert_eq!(current.revision, 2);
    assert_eq!(current.title, "Regenerated Title");

    // Verify history
    let history = service.list_history("cp_1").unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].revision, 1);
    assert_eq!(history[1].revision, 2);
    assert_eq!(history[1].parent_revision, Some(1));
    assert_eq!(history[1].operation_type, "regenerate");

    // Verify event
    let events = event_bus
        .list_events(Some("content_plan.revision_created"), None, 10)
        .unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].job_id.as_deref(), Some("job_2"));
    assert_eq!(events[0].payload["planId"], "cp_1");
    assert_eq!(events[0].payload["revision"], 2);
    assert_eq!(events[0].payload["parentRevision"], 1);
    assert_eq!(events[0].payload["operationType"], "regenerate");
}

#[test]
fn test_transactional_rollback() {
    let (_dir, db, _eb, service) = setup_service();
    let plan = sample_plan("cp_1", "p_1", 1);
    let sources = vec![sample_source("src_1", "Snapshot 1")];
    service
        .publish_generated_revision(
            "cp_1",
            None,
            plan,
            sources.clone(),
            "generate_initial",
            "planner_worker",
            None,
        )
        .unwrap();

    // Install trigger that fails event recording specifically for job_id = 'fail_job'
    {
        let conn = db.conn().unwrap();
        conn.execute_batch(
            "CREATE TRIGGER fail_event_trigger
             BEFORE INSERT ON domain_events
             WHEN NEW.job_id = 'fail_job'
             BEGIN
                 SELECT RAISE(ABORT, 'simulated domain event failure');
             END;",
        )
        .unwrap();
    }

    let mut candidate2 = sample_plan("cp_1", "p_1", 1);
    candidate2.title = "Failed Regeneration".to_string();

    let err = service
        .publish_generated_revision(
            "cp_1",
            Some(1),
            candidate2,
            sources,
            "regenerate",
            "planner_worker",
            Some("fail_job"),
        )
        .expect_err("should fail due to domain event trigger abort");

    match err {
        ContentPlanningError::ContentPlanPersistFailed(msg) => {
            assert!(msg.contains("simulated domain event failure"));
        }
        other => panic!("expected ContentPlanPersistFailed, got {other:?}"),
    }

    // Verify rollback: current plan still rev 1, rev 2 does not exist, history length 1
    let current = service.get_plan("cp_1").unwrap().unwrap();
    assert_eq!(current.revision, 1);
    assert_eq!(current.title, "Plan cp_1");

    let rev2 = service.get_plan_revision("cp_1", 2).unwrap();
    assert!(rev2.is_none(), "revision 2 must not exist due to rollback");

    let history = service.list_history("cp_1").unwrap();
    assert_eq!(history.len(), 1);
}

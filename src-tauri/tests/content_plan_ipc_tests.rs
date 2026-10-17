use std::sync::Arc;
use tempfile::tempdir;

use openvideoai_lib::content_planning::commands::*;
use openvideoai_lib::content_planning::jobs::{
    PlanningOperation, RegenerationJobParams, CONTENT_PLAN_JOB_TYPE,
};
use openvideoai_lib::content_planning::model::{
    Claim, ContentPlan, ContentScene, GenerationOptions, GroundingStatus, InputType,
    PlanningRequest, PlanningSource, SourceRef,
};
use openvideoai_lib::content_planning::service::ContentPlanService;
use openvideoai_lib::db::DbManager;
use openvideoai_lib::events::bus::EventBus;
use openvideoai_lib::jobs::manager::JobManager;
use openvideoai_lib::jobs::model::JobState;
use openvideoai_lib::projects::model::Project;
use openvideoai_lib::projects::repository::ProjectRepository;

fn setup_ipc_test() -> (
    tempfile::TempDir,
    Arc<DbManager>,
    Arc<JobManager>,
    Arc<ContentPlanService>,
) {
    let dir = tempdir().expect("tempdir failed");
    let db_path = dir.path().join("ipc_test.db");
    let db = Arc::new(DbManager::new(db_path).expect("db failed"));
    db.run_migrations().expect("migrations failed");

    let project = Project {
        id: "proj-1".to_string(),
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
    let job_mgr = Arc::new(JobManager::new(db.clone()));
    let plan_service = Arc::new(ContentPlanService::new(db.clone(), event_bus));

    (dir, db, job_mgr, plan_service)
}

fn sample_source(id: &str) -> PlanningSource {
    PlanningSource {
        source_id: id.to_string(),
        source_type: "document".to_string(),
        title: format!("Source {}", id),
        author: Some("Author".to_string()),
        url: Some("https://example.com".to_string()),
        published_at: Some("2026-09-12T00:00:00Z".to_string()),
        locator: Some("p. 1".to_string()),
        content_snapshot: "Snapshot content text".to_string(),
        content_hash: "abcd1234hash".to_string(),
    }
}

fn sample_plan(id: &str, project_id: &str, revision: i64) -> ContentPlan {
    ContentPlan {
        id: id.to_string(),
        project_id: project_id.to_string(),
        revision,
        status: openvideoai_lib::content_planning::model::PlanStatus::Draft,
        title: format!("Plan {id}"),
        summary: "Plan summary".to_string(),
        language: "en".to_string(),
        tone: "informative".to_string(),
        target_duration_ms: 10_000,
        estimated_duration_ms: 10_000,
        full_narration: "Narration for scene 1".to_string(),
        scenes: vec![ContentScene {
            id: "scene_1".to_string(),
            order: 1,
            title: "Scene 1".to_string(),
            narration: "Narration for scene 1".to_string(),
            duration_estimate_ms: 10_000,
            visual_description: "Visual description for scene 1".to_string(),
            media_intent: vec![openvideoai_lib::content_planning::model::MediaIntent::StockVideo],
            stock_search_queries: vec!["topic".to_string()],
            image_prompt: None,
            video_prompt: None,
            claims: vec![Claim {
                id: "claim_1".to_string(),
                text: "Narration for scene 1".to_string(),
                source_refs: vec![SourceRef {
                    source_id: "src-1".to_string(),
                    locator: Some("p.1".to_string()),
                    excerpt_hash: Some("abcd1234hash".to_string()),
                    note: None,
                }],
                grounding_status: GroundingStatus::Supported,
                grounding_valid: true,
                note: None,
            }],
            source_refs: vec![],
        }],
        source_refs: vec![],
        generation_metadata: None,
        created_at: "2026-09-12T10:00:00Z".to_string(),
        updated_at: "2026-09-12T10:00:00Z".to_string(),
    }
}

#[test]
fn test_ipc_submit_content_plan_job() {
    let (_dir, _db, job_mgr, _plan_service) = setup_ipc_test();
    let req = PlanningRequest {
        project_id: "proj-1".to_string(),
        input_type: InputType::Topic,
        input_text: "AI Video Production".to_string(),
        target_duration_ms: 30000,
        language: "en".to_string(),
        tone: "informative".to_string(),
        source_context: vec![sample_source("src-1")],
        generation_options: GenerationOptions {},
    };

    let job = cmd_submit_content_plan_job(&job_mgr, "proj-1".to_string(), req, None, None).unwrap();
    assert_eq!(job.job_type, CONTENT_PLAN_JOB_TYPE);
    assert_eq!(job.project_id.as_deref(), Some("proj-1"));
    assert_eq!(job.status, JobState::Queued);
}

#[test]
fn test_ipc_get_and_list_content_plans() {
    let (_dir, _db, _job_mgr, plan_service) = setup_ipc_test();
    let plan = sample_plan("plan-1", "proj-1", 1);
    let sources = vec![sample_source("src-1")];
    let published = plan_service
        .publish_generated_revision("plan-1", None, plan, sources, "generate", "test-user", None)
        .unwrap();

    let fetched = cmd_get_content_plan(&plan_service, published.id.clone())
        .unwrap()
        .expect("should find plan");
    assert_eq!(fetched.id, published.id);
    assert_eq!(fetched.revision, 1);

    let list = cmd_list_project_content_plans(&plan_service, "proj-1".to_string(), None).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, published.id);
}

#[test]
fn test_ipc_save_content_plan_revision() {
    let (_dir, _db, _job_mgr, plan_service) = setup_ipc_test();
    let plan = sample_plan("plan-1", "proj-1", 1);
    let sources = vec![sample_source("src-1")];
    let published = plan_service
        .publish_generated_revision("plan-1", None, plan, sources, "generate", "test-user", None)
        .unwrap();

    let mut updated = published.clone();
    updated.title = "Updated Plan Title".to_string();

    let saved =
        cmd_save_content_plan_revision(&plan_service, published.id.clone(), 1, updated).unwrap();
    assert_eq!(saved.revision, 2);
    assert_eq!(saved.title, "Updated Plan Title");
}

#[test]
fn test_ipc_save_content_plan_revision_conflict() {
    let (_dir, _db, _job_mgr, plan_service) = setup_ipc_test();
    let plan = sample_plan("plan-1", "proj-1", 1);
    let sources = vec![sample_source("src-1")];
    let published = plan_service
        .publish_generated_revision("plan-1", None, plan, sources, "generate", "test-user", None)
        .unwrap();

    let updated = published.clone();
    // Stale expected_revision: 99
    let err = cmd_save_content_plan_revision(&plan_service, published.id.clone(), 99, updated);
    assert!(err.is_err(), "Must reject with conflict");
    assert!(err.unwrap_err().contains("content_plan_revision_conflict"));
}

#[test]
fn test_ipc_submit_regeneration_job() {
    let (_dir, _db, job_mgr, plan_service) = setup_ipc_test();
    let plan = sample_plan("plan-1", "proj-1", 1);
    let sources = vec![sample_source("src-1")];
    let published = plan_service
        .publish_generated_revision(
            "plan-1",
            None,
            plan,
            sources.clone(),
            "generate",
            "test-user",
            None,
        )
        .unwrap();

    let req = PlanningRequest {
        project_id: "proj-1".to_string(),
        input_type: InputType::Topic,
        input_text: "AI Video Production".to_string(),
        target_duration_ms: 30000,
        language: "en".to_string(),
        tone: "informative".to_string(),
        source_context: sources.clone(),
        generation_options: GenerationOptions {},
    };

    let params = RegenerationJobParams {
        project_id: "proj-1".to_string(),
        plan_id: published.id.clone(),
        base_revision: 1,
        operation: PlanningOperation::RegeneratePlan,
        scene_id: None,
        user_instruction: Some("Make tone more upbeat".to_string()),
        planning_request: req,
        source_context: sources,
        provider: None,
        provider_config_ref: None,
    };

    let job = cmd_submit_content_plan_regeneration(&job_mgr, params).unwrap();
    assert_eq!(job.job_type, CONTENT_PLAN_JOB_TYPE);
    assert_eq!(job.status, JobState::Queued);
}

#[test]
fn test_ipc_approve_content_plan() {
    let (_dir, _db, _job_mgr, plan_service) = setup_ipc_test();
    let plan = sample_plan("plan-1", "proj-1", 1);
    let sources = vec![sample_source("src-1")];
    let published = plan_service
        .publish_generated_revision("plan-1", None, plan, sources, "generate", "test-user", None)
        .unwrap();

    let approved = cmd_approve_content_plan(&plan_service, published.id.clone(), 1).unwrap();
    assert_eq!(
        approved.status,
        openvideoai_lib::content_planning::model::PlanStatus::Approved
    );
    assert_eq!(approved.approved_revision, Some(1));
}

#[test]
fn test_ipc_archive_content_plan() {
    let (_dir, _db, _job_mgr, plan_service) = setup_ipc_test();
    let plan = sample_plan("plan-1", "proj-1", 1);
    let sources = vec![sample_source("src-1")];
    let published = plan_service
        .publish_generated_revision("plan-1", None, plan, sources, "generate", "test-user", None)
        .unwrap();

    let archived = cmd_archive_content_plan(&plan_service, published.id.clone(), 1).unwrap();
    assert_eq!(
        archived.status,
        openvideoai_lib::content_planning::model::PlanStatus::Archived
    );
}

#[test]
fn test_ipc_get_content_plan_history() {
    let (_dir, _db, _job_mgr, plan_service) = setup_ipc_test();
    let plan = sample_plan("plan-1", "proj-1", 1);
    let sources = vec![sample_source("src-1")];
    let published = plan_service
        .publish_generated_revision("plan-1", None, plan, sources, "generate", "test-user", None)
        .unwrap();

    // Revision 2
    let mut updated = published.clone();
    updated.title = "Revision 2 Title".to_string();
    let _rev2 = plan_service
        .save_manual_revision(&published.id, 1, updated)
        .unwrap();

    let history = cmd_get_content_plan_history(&plan_service, published.id.clone()).unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].revision, 1);
    assert_eq!(history[0].operation_type, "create");
    assert_eq!(history[0].created_by, "test-user");
    assert_eq!(history[1].revision, 2);
    assert_eq!(history[1].operation_type, "manual_edit");
}

#[test]
fn test_ipc_get_content_plan_revision() {
    let (_dir, _db, _job_mgr, plan_service) = setup_ipc_test();
    let plan = sample_plan("plan-1", "proj-1", 1);
    let sources = vec![sample_source("src-1")];
    let published = plan_service
        .publish_generated_revision("plan-1", None, plan, sources, "generate", "test-user", None)
        .unwrap();

    // Revision 2
    let mut updated = published.clone();
    updated.title = "Revision 2 Title".to_string();
    let _rev2 = plan_service
        .save_manual_revision(&published.id, 1, updated)
        .unwrap();

    // Get Revision 1
    let fetched_rev1 = cmd_get_content_plan_revision(&plan_service, published.id.clone(), 1)
        .unwrap()
        .expect("revision 1 should exist");
    assert_eq!(fetched_rev1.revision, 1);
    assert_eq!(fetched_rev1.title, "Plan plan-1");

    // Get Revision 2
    let fetched_rev2 = cmd_get_content_plan_revision(&plan_service, published.id.clone(), 2)
        .unwrap()
        .expect("revision 2 should exist");
    assert_eq!(fetched_rev2.revision, 2);
    assert_eq!(fetched_rev2.title, "Revision 2 Title");
}

#[test]
fn test_ipc_get_planner_provider_status_no_secret_leak() {
    let status = cmd_get_planner_provider_status().unwrap();
    assert_eq!(status.provider, "gemini");
    // Ensure no secret leak in serialized representation
    let json = serde_json::to_string(&status).unwrap();
    assert!(!json.to_lowercase().contains("key"));
    assert!(!json.to_lowercase().contains("secret"));
}

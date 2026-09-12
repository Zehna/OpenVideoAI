use openvideoai_lib::content_planning::jobs::{
    compute_context_hash, compute_input_hash, compute_instruction_hash, generate_idempotency_key,
    regenerate_narration_idempotency_key, regenerate_plan_idempotency_key,
    regenerate_scene_idempotency_key, submit_generate_job, submit_regeneration_job,
    ContentPlanJobPayload, PlanningOperation, RegenerationJobParams, CONTENT_PLAN_JOB_TYPE,
};
use openvideoai_lib::content_planning::model::{
    GenerationOptions, InputType, PlanningRequest, PlanningSource,
};
use openvideoai_lib::db::DbManager;
use openvideoai_lib::jobs::manager::JobManager;
use openvideoai_lib::projects::model::Project;
use openvideoai_lib::projects::repository::ProjectRepository;
use std::sync::Arc;
use tempfile::tempdir;

fn setup_test_context() -> (tempfile::TempDir, Arc<DbManager>, Arc<JobManager>) {
    let dir = tempdir().expect("tempdir failed");
    let db_path = dir.path().join("jobs_test.db");
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

    let job_mgr = Arc::new(JobManager::new(db.clone()));
    (dir, db, job_mgr)
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

fn sample_request(project_id: &str) -> PlanningRequest {
    PlanningRequest {
        project_id: project_id.to_string(),
        input_type: InputType::Topic,
        input_text: "The Future of AI in Video Editing".to_string(),
        target_duration_ms: 60000,
        language: "en".to_string(),
        tone: "informative".to_string(),
        source_context: vec![sample_source("src-1")],
        generation_options: GenerationOptions {},
    }
}

#[test]
fn test_job_submission_generate_contract() {
    let (_dir, _db, job_mgr) = setup_test_context();
    let req = sample_request("proj-1");

    let job = submit_generate_job(&job_mgr, "proj-1", req.clone(), None, None)
        .expect("submit generate job should succeed");

    assert_eq!(job.job_type, CONTENT_PLAN_JOB_TYPE);
    assert_eq!(job.job_type, "content.plan");
    assert_eq!(job.payload_version, 1);
    assert_eq!(job.max_retries, 3);
    assert_eq!(job.priority, 10);
    assert_eq!(job.project_id.as_deref(), Some("proj-1"));

    let payload: ContentPlanJobPayload = serde_json::from_str(&job.payload)
        .expect("payload must deserialize to ContentPlanJobPayload");
    assert_eq!(payload.schema_version, 1);
    assert_eq!(payload.project_id, "proj-1");
    assert_eq!(payload.operation, PlanningOperation::Generate);
    assert_eq!(payload.planning_request, req);
    assert_eq!(payload.source_context, req.source_context);
    assert_eq!(payload.provider, "gemini");
    assert_eq!(payload.provider_config_ref, "gemini-default");
    assert_eq!(payload.planner_profile_version, 1);

    let key = job.idempotency_key.expect("idempotency key must exist");
    assert!(
        key.starts_with("content.plan:generate:proj-1:"),
        "Key was {}",
        key
    );
}

#[test]
fn test_job_submission_regeneration_plan_contract() {
    let (_dir, _db, job_mgr) = setup_test_context();
    let req = sample_request("proj-1");

    let params = RegenerationJobParams {
        project_id: "proj-1".to_string(),
        plan_id: "plan-123".to_string(),
        base_revision: 2,
        operation: PlanningOperation::RegeneratePlan,
        scene_id: None,
        user_instruction: Some("Make it more dramatic".to_string()),
        planning_request: req.clone(),
        source_context: req.source_context.clone(),
        provider: None,
        provider_config_ref: None,
    };

    let job =
        submit_regeneration_job(&job_mgr, params).expect("submit regeneration plan should succeed");

    assert_eq!(job.job_type, CONTENT_PLAN_JOB_TYPE);
    assert_eq!(job.payload_version, 1);
    assert_eq!(job.max_retries, 3);
    assert_eq!(job.priority, 10);

    let key = job.idempotency_key.expect("key must exist");
    assert!(
        key.starts_with("content.plan:regenerate_plan:plan-123:2:"),
        "Key was {}",
        key
    );

    let payload: ContentPlanJobPayload =
        serde_json::from_str(&job.payload).expect("valid payload JSON");
    assert_eq!(payload.operation, PlanningOperation::RegeneratePlan);
    assert_eq!(payload.base_plan_id.as_deref(), Some("plan-123"));
    assert_eq!(payload.base_revision, Some(2));
    assert_eq!(payload.scene_id, None);
    assert_eq!(
        payload.user_instruction.as_deref(),
        Some("Make it more dramatic")
    );
}

#[test]
fn test_job_submission_regeneration_scene_contract() {
    let (_dir, _db, job_mgr) = setup_test_context();
    let req = sample_request("proj-1");

    let params = RegenerationJobParams {
        project_id: "proj-1".to_string(),
        plan_id: "plan-123".to_string(),
        base_revision: 3,
        operation: PlanningOperation::RegenerateScene,
        scene_id: Some("scene-2".to_string()),
        user_instruction: Some("Add hook to opening".to_string()),
        planning_request: req.clone(),
        source_context: req.source_context.clone(),
        provider: None,
        provider_config_ref: None,
    };

    let job = submit_regeneration_job(&job_mgr, params)
        .expect("submit regeneration scene should succeed");

    assert_eq!(job.job_type, "content.plan");
    let key = job.idempotency_key.expect("key must exist");
    assert!(
        key.starts_with("content.plan:regenerate_scene:plan-123:3:scene-2:"),
        "Key was {}",
        key
    );

    let payload: ContentPlanJobPayload = serde_json::from_str(&job.payload).unwrap();
    assert_eq!(payload.operation, PlanningOperation::RegenerateScene);
    assert_eq!(payload.scene_id.as_deref(), Some("scene-2"));
}

#[test]
fn test_job_submission_regeneration_narration_contract() {
    let (_dir, _db, job_mgr) = setup_test_context();
    let req = sample_request("proj-1");

    let params = RegenerationJobParams {
        project_id: "proj-1".to_string(),
        plan_id: "plan-123".to_string(),
        base_revision: 4,
        operation: PlanningOperation::RegenerateNarration,
        scene_id: Some("scene-3".to_string()),
        user_instruction: None,
        planning_request: req.clone(),
        source_context: req.source_context.clone(),
        provider: None,
        provider_config_ref: None,
    };

    let job = submit_regeneration_job(&job_mgr, params)
        .expect("submit regeneration narration should succeed");

    assert_eq!(job.job_type, "content.plan");
    let key = job.idempotency_key.expect("key must exist");
    assert!(
        key.starts_with("content.plan:regenerate_narration:plan-123:4:scene-3:"),
        "Key was {}",
        key
    );

    let payload: ContentPlanJobPayload = serde_json::from_str(&job.payload).unwrap();
    assert_eq!(payload.operation, PlanningOperation::RegenerateNarration);
    assert_eq!(payload.scene_id.as_deref(), Some("scene-3"));
    assert_eq!(payload.user_instruction, None);
}

#[test]
fn test_idempotency_identical_requests_converge() {
    let (_dir, _db, job_mgr) = setup_test_context();
    let req = sample_request("proj-1");

    let job1 = submit_generate_job(&job_mgr, "proj-1", req.clone(), None, None).unwrap();
    let job2 = submit_generate_job(&job_mgr, "proj-1", req.clone(), None, None).unwrap();

    assert_eq!(
        job1.id, job2.id,
        "Identical generate requests must converge to the same job"
    );
}

#[test]
fn test_idempotency_differs_on_base_revision() {
    let (_dir, _db, job_mgr) = setup_test_context();
    let req = sample_request("proj-1");

    let mut params1 = RegenerationJobParams {
        project_id: "proj-1".to_string(),
        plan_id: "plan-1".to_string(),
        base_revision: 1,
        operation: PlanningOperation::RegeneratePlan,
        scene_id: None,
        user_instruction: None,
        planning_request: req.clone(),
        source_context: req.source_context.clone(),
        provider: None,
        provider_config_ref: None,
    };

    let job1 = submit_regeneration_job(&job_mgr, params1.clone()).unwrap();
    params1.base_revision = 2;
    let job2 = submit_regeneration_job(&job_mgr, params1).unwrap();

    assert_ne!(
        job1.id, job2.id,
        "Different base revision must produce different job"
    );
    assert_ne!(job1.idempotency_key, job2.idempotency_key);
}

#[test]
fn test_idempotency_differs_on_scene_id() {
    let (_dir, _db, job_mgr) = setup_test_context();
    let req = sample_request("proj-1");

    let mut params = RegenerationJobParams {
        project_id: "proj-1".to_string(),
        plan_id: "plan-1".to_string(),
        base_revision: 1,
        operation: PlanningOperation::RegenerateScene,
        scene_id: Some("scene-a".to_string()),
        user_instruction: None,
        planning_request: req.clone(),
        source_context: req.source_context.clone(),
        provider: None,
        provider_config_ref: None,
    };

    let job1 = submit_regeneration_job(&job_mgr, params.clone()).unwrap();
    params.scene_id = Some("scene-b".to_string());
    let job2 = submit_regeneration_job(&job_mgr, params).unwrap();

    assert_ne!(job1.id, job2.id);
    assert_ne!(job1.idempotency_key, job2.idempotency_key);
}

#[test]
fn test_idempotency_differs_on_instruction() {
    let (_dir, _db, job_mgr) = setup_test_context();
    let req = sample_request("proj-1");

    let mut params = RegenerationJobParams {
        project_id: "proj-1".to_string(),
        plan_id: "plan-1".to_string(),
        base_revision: 1,
        operation: PlanningOperation::RegeneratePlan,
        scene_id: None,
        user_instruction: Some("Instruction Alpha".to_string()),
        planning_request: req.clone(),
        source_context: req.source_context.clone(),
        provider: None,
        provider_config_ref: None,
    };

    let job1 = submit_regeneration_job(&job_mgr, params.clone()).unwrap();
    params.user_instruction = Some("Instruction Beta".to_string());
    let job2 = submit_regeneration_job(&job_mgr, params).unwrap();

    assert_ne!(job1.id, job2.id);
    assert_ne!(job1.idempotency_key, job2.idempotency_key);
}

#[test]
fn test_idempotency_differs_on_input_text() {
    let (_dir, _db, job_mgr) = setup_test_context();
    let mut req1 = sample_request("proj-1");
    req1.input_text = "Topic A".to_string();

    let mut req2 = sample_request("proj-1");
    req2.input_text = "Topic B".to_string();

    let job1 = submit_generate_job(&job_mgr, "proj-1", req1, None, None).unwrap();
    let job2 = submit_generate_job(&job_mgr, "proj-1", req2, None, None).unwrap();

    assert_ne!(job1.id, job2.id);
    assert_ne!(job1.idempotency_key, job2.idempotency_key);
}

#[test]
fn test_idempotency_differs_on_source_context() {
    let (_dir, _db, job_mgr) = setup_test_context();
    let mut req1 = sample_request("proj-1");
    req1.source_context = vec![sample_source("src-1")];

    let mut req2 = sample_request("proj-1");
    req2.source_context = vec![sample_source("src-1"), sample_source("src-2")];

    let job1 = submit_generate_job(&job_mgr, "proj-1", req1, None, None).unwrap();
    let job2 = submit_generate_job(&job_mgr, "proj-1", req2, None, None).unwrap();

    assert_ne!(job1.id, job2.id);
    assert_ne!(job1.idempotency_key, job2.idempotency_key);
}

#[test]
fn test_no_secrets_in_payload_or_key() {
    let (_dir, _db, job_mgr) = setup_test_context();
    let req = sample_request("proj-1");

    let job = submit_generate_job(&job_mgr, "proj-1", req, None, None).unwrap();
    assert!(
        !job.payload.contains("AIza"),
        "No API key prefix in payload"
    );
    assert!(!job.payload.contains("api_key"), "No api_key in payload");
    let key = job.idempotency_key.unwrap();
    assert!(!key.contains("AIza"), "No API key prefix in key");
    assert!(!key.contains("api_key"), "No api_key in key");
}

#[test]
fn test_regeneration_validation_errors() {
    let (_dir, _db, job_mgr) = setup_test_context();
    let req = sample_request("proj-1");

    // Missing scene_id on RegenerateScene
    let params_scene = RegenerationJobParams {
        project_id: "proj-1".to_string(),
        plan_id: "plan-1".to_string(),
        base_revision: 1,
        operation: PlanningOperation::RegenerateScene,
        scene_id: None,
        user_instruction: None,
        planning_request: req.clone(),
        source_context: req.source_context.clone(),
        provider: None,
        provider_config_ref: None,
    };
    assert!(submit_regeneration_job(&job_mgr, params_scene).is_err());

    // Missing scene_id on RegenerateNarration
    let params_narration = RegenerationJobParams {
        project_id: "proj-1".to_string(),
        plan_id: "plan-1".to_string(),
        base_revision: 1,
        operation: PlanningOperation::RegenerateNarration,
        scene_id: Some("".to_string()),
        user_instruction: None,
        planning_request: req.clone(),
        source_context: req.source_context.clone(),
        provider: None,
        provider_config_ref: None,
    };
    assert!(submit_regeneration_job(&job_mgr, params_narration).is_err());

    // Invalid operation Generate passed to submit_regeneration_job
    let params_invalid_op = RegenerationJobParams {
        project_id: "proj-1".to_string(),
        plan_id: "plan-1".to_string(),
        base_revision: 1,
        operation: PlanningOperation::Generate,
        scene_id: None,
        user_instruction: None,
        planning_request: req.clone(),
        source_context: req.source_context.clone(),
        provider: None,
        provider_config_ref: None,
    };
    assert!(submit_regeneration_job(&job_mgr, params_invalid_op).is_err());

    // Non-positive base revision
    let params_bad_rev = RegenerationJobParams {
        project_id: "proj-1".to_string(),
        plan_id: "plan-1".to_string(),
        base_revision: 0,
        operation: PlanningOperation::RegeneratePlan,
        scene_id: None,
        user_instruction: None,
        planning_request: req,
        source_context: vec![],
        provider: None,
        provider_config_ref: None,
    };
    assert!(submit_regeneration_job(&job_mgr, params_bad_rev).is_err());
}

#[test]
fn test_canonical_hash_helpers() {
    let req = sample_request("proj-1");
    let input_hash1 = compute_input_hash(&req);
    let input_hash2 = compute_input_hash(&req);
    assert_eq!(input_hash1, input_hash2, "Hashing must be deterministic");

    let context_hash1 = compute_context_hash(&req.source_context);
    let context_hash2 = compute_context_hash(&req.source_context);
    assert_eq!(
        context_hash1, context_hash2,
        "Hashing context must be deterministic"
    );

    let inst_hash1 = compute_instruction_hash(Some("hello world"));
    let inst_hash2 = compute_instruction_hash(Some("  hello world  "));
    assert_eq!(
        inst_hash1, inst_hash2,
        "Instruction hash must trim whitespace"
    );

    let gen_key = generate_idempotency_key("proj-1", &input_hash1, &context_hash1, 1);
    assert_eq!(
        gen_key,
        format!(
            "content.plan:generate:proj-1:{}:{}:1",
            input_hash1, context_hash1
        )
    );

    let plan_key = regenerate_plan_idempotency_key("plan-1", 2, &inst_hash1, 1);
    assert_eq!(
        plan_key,
        format!("content.plan:regenerate_plan:plan-1:2:{}:1", inst_hash1)
    );

    let scene_key = regenerate_scene_idempotency_key("plan-1", 2, "scene-1", &inst_hash1, 1);
    assert_eq!(
        scene_key,
        format!(
            "content.plan:regenerate_scene:plan-1:2:scene-1:{}:1",
            inst_hash1
        )
    );

    let narration_key =
        regenerate_narration_idempotency_key("plan-1", 2, "scene-1", &inst_hash1, 1);
    assert_eq!(
        narration_key,
        format!(
            "content.plan:regenerate_narration:plan-1:2:scene-1:{}:1",
            inst_hash1
        )
    );
}

use std::sync::Arc;
use tempfile::tempdir;

use openvideoai_lib::content_planning::commands::cmd_get_planner_provider_status;
use openvideoai_lib::content_planning::jobs::{
    submit_generate_job, submit_regeneration_job, PlanningOperation, RegenerationJobParams,
};
use openvideoai_lib::content_planning::model::*;
use openvideoai_lib::content_planning::provider::config::ProviderConfigResolver;
use openvideoai_lib::content_planning::provider::gemini::classify_http_status_and_body;
use openvideoai_lib::content_planning::service::ContentPlanService;
use openvideoai_lib::db::DbManager;
use openvideoai_lib::events::bus::EventBus;
use openvideoai_lib::jobs::manager::JobManager;
use openvideoai_lib::projects::model::Project;
use openvideoai_lib::projects::repository::ProjectRepository;

const SENTINEL_SECRET: &str = "phase4-secret-sentinel-998877665544332211";

fn setup_test_env() -> (
    tempfile::TempDir,
    Arc<DbManager>,
    Arc<EventBus>,
    Arc<JobManager>,
    ContentPlanService,
) {
    let dir = tempdir().expect("failed to create tempdir");
    let db_path = dir.path().join("content_plan_security_test.db");
    let db = Arc::new(DbManager::new(db_path).expect("failed to init db"));
    db.run_migrations().expect("failed to run migrations");

    let project = Project {
        id: "proj_sec_1".to_string(),
        name: "Security Test Project".to_string(),
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
    let service = ContentPlanService::new(db.clone(), event_bus.clone());

    (dir, db, event_bus, job_mgr, service)
}

fn sample_source(id: &str) -> PlanningSource {
    PlanningSource {
        source_id: id.to_string(),
        source_type: "article".to_string(),
        title: format!("Source {id}"),
        author: Some("Author".to_string()),
        url: Some("https://example.com/sec".to_string()),
        published_at: Some("2026-09-12T00:00:00Z".to_string()),
        locator: Some("sec. 1".to_string()),
        content_snapshot: "Clean factual snapshot".to_string(),
        content_hash: "hash123".to_string(),
    }
}

fn sample_request(project_id: &str) -> PlanningRequest {
    PlanningRequest {
        project_id: project_id.to_string(),
        input_type: InputType::Topic,
        input_text: "Security and Privacy in Media".to_string(),
        target_duration_ms: 30000,
        language: "en".to_string(),
        tone: "informative".to_string(),
        generation_options: GenerationOptions::default(),
        source_context: vec![sample_source("s1")],
    }
}

fn sample_plan(project_id: &str) -> ContentPlan {
    ContentPlan {
        id: "sec_plan_1".to_string(),
        project_id: project_id.to_string(),
        revision: 1,
        status: PlanStatus::Draft,
        title: "Security and Privacy in Media".to_string(),
        summary: "A video about digital privacy and security.".to_string(),
        language: "en".to_string(),
        tone: "informative".to_string(),
        target_duration_ms: 30000,
        estimated_duration_ms: 30000,
        full_narration: "Narration text here".to_string(),
        source_refs: vec![SourceRef {
            source_id: "s1".to_string(),
            locator: Some("sec. 1".to_string()),
            excerpt_hash: None,
            note: None,
        }],
        scenes: vec![ContentScene {
            id: "scene_1".to_string(),
            order: 1,
            title: "Intro".to_string(),
            duration_estimate_ms: 30000,
            narration: "Narration text here".to_string(),
            visual_description: "Visual here".to_string(),
            media_intent: vec![MediaIntent::StockVideo],
            stock_search_queries: vec!["security".to_string()],
            image_prompt: None,
            video_prompt: None,
            claims: vec![Claim {
                id: "c1".to_string(),
                text: "Security is vital.".to_string(),
                grounding_status: GroundingStatus::Supported,
                grounding_valid: true,
                source_refs: vec![SourceRef {
                    source_id: "s1".to_string(),
                    locator: Some("sec. 1".to_string()),
                    excerpt_hash: None,
                    note: None,
                }],
                note: None,
            }],
            source_refs: vec![SourceRef {
                source_id: "s1".to_string(),
                locator: Some("sec. 1".to_string()),
                excerpt_hash: None,
                note: None,
            }],
        }],
        generation_metadata: Some(GenerationMetadata {
            provider: "gemini-default".to_string(),
            model: "gemini-3.8-flash".to_string(),
            prompt_template_version: 1,
            planner_profile_version: 1,
            generated_at: "2026-09-12T10:00:00Z".to_string(),
            usage: GenerationUsage::default(),
            repaired_calls: None,
        }),
        created_at: "2026-09-12T10:00:00Z".to_string(),
        updated_at: "2026-09-12T10:00:00Z".to_string(),
    }
}

#[test]
fn test_security_sentinel_never_leaked_in_jobs() {
    std::env::set_var("GEMINI_API_KEY", SENTINEL_SECRET);
    let (_dir, db, _events, job_mgr, _service) = setup_test_env();
    let req = sample_request("proj_sec_1");

    // 1. Submit generate job
    let job1 = submit_generate_job(&job_mgr, "proj_sec_1", req.clone(), None, None).unwrap();
    assert!(
        !job1.payload.contains(SENTINEL_SECRET),
        "Sentinel in generate job payload"
    );
    if let Some(ref key) = job1.idempotency_key {
        assert!(
            !key.contains(SENTINEL_SECRET),
            "Sentinel in generate job idempotency key"
        );
    }

    // 2. Submit regeneration job (scene)
    let regen_params = RegenerationJobParams {
        project_id: "proj_sec_1".to_string(),
        plan_id: "sec_plan_1".to_string(),
        base_revision: 1,
        operation: PlanningOperation::RegenerateScene,
        scene_id: Some("scene_1".to_string()),
        user_instruction: Some("Make it punchier".to_string()),
        planning_request: req,
        source_context: vec![sample_source("s1")],
        provider: None,
        provider_config_ref: None,
    };
    let job2 = submit_regeneration_job(&job_mgr, regen_params).unwrap();
    assert!(
        !job2.payload.contains(SENTINEL_SECRET),
        "Sentinel in regen job payload"
    );
    if let Some(ref key) = job2.idempotency_key {
        assert!(
            !key.contains(SENTINEL_SECRET),
            "Sentinel in regen job idempotency key"
        );
    }

    // 3. Raw query SQLite jobs table to ensure no leak in any column
    let conn = db.conn().unwrap();
    let mut stmt = conn
        .prepare("SELECT id, payload, idempotency_key FROM jobs")
        .unwrap();
    let rows = stmt
        .query_map([], |row| {
            let id: String = row.get(0)?;
            let payload: String = row.get(1)?;
            let idemp: Option<String> = row.get(2)?;
            Ok((id, payload, idemp))
        })
        .unwrap();

    for r in rows {
        let (id, payload, idemp) = r.unwrap();
        assert!(!id.contains(SENTINEL_SECRET));
        assert!(!payload.contains(SENTINEL_SECRET));
        if let Some(k) = idemp {
            assert!(!k.contains(SENTINEL_SECRET));
        }
    }
}

#[test]
fn test_security_sentinel_never_leaked_in_revisions_and_metadata() {
    std::env::set_var("GEMINI_API_KEY", SENTINEL_SECRET);
    let (_dir, db, _events, _job_mgr, service) = setup_test_env();

    let plan = sample_plan("proj_sec_1");
    let sources = vec![sample_source("s1")];

    // Publish initial plan
    let published = service
        .publish_generated_revision(
            "sec_plan_1",
            None,
            plan.clone(),
            sources,
            "create",
            "test-user",
            None,
        )
        .unwrap();

    // Verify in-memory plan serialization
    let plan_json = serde_json::to_string(&published).unwrap();
    assert!(
        !plan_json.contains(SENTINEL_SECRET),
        "Sentinel leaked in ContentPlan serialization"
    );

    // Verify raw DB content
    let conn = db.conn().unwrap();
    let mut stmt = conn
        .prepare("SELECT content_json, generation_metadata_json FROM content_plan_revisions")
        .unwrap();
    let rows = stmt
        .query_map([], |row| {
            let c: String = row.get(0)?;
            let m: Option<String> = row.get(1)?;
            Ok((c, m))
        })
        .unwrap();

    for r in rows {
        let (content, meta) = r.unwrap();
        assert!(
            !content.contains(SENTINEL_SECRET),
            "Sentinel in revision content_json"
        );
        if let Some(m) = meta {
            assert!(
                !m.contains(SENTINEL_SECRET),
                "Sentinel in revision generation_metadata_json"
            );
        }
    }
}

#[test]
fn test_security_sentinel_never_leaked_in_domain_events() {
    std::env::set_var("GEMINI_API_KEY", SENTINEL_SECRET);
    let (_dir, db, _events, _job_mgr, service) = setup_test_env();

    let plan = sample_plan("proj_sec_1");
    let sources = vec![sample_source("s1")];

    // 1. Created event via initial publish
    let created_plan = service
        .publish_generated_revision(
            "sec_plan_1",
            None,
            plan,
            sources,
            "create",
            "test-user",
            None,
        )
        .unwrap();

    // 2. Revision created event via manual save
    let mut modified = created_plan.clone();
    modified.scenes[0].visual_description = "Updated description".to_string();
    let rev2 = service
        .save_manual_revision(&created_plan.id, created_plan.revision, modified)
        .unwrap();

    // 3. Approved event
    let approved = service.approve(&rev2.id, rev2.revision).unwrap();

    // 4. Archived event
    service
        .archive(&approved.id, approved.current_revision)
        .unwrap();

    // Check all emitted domain events from database
    let conn = db.conn().unwrap();
    let mut stmt = conn
        .prepare("SELECT event_type, payload FROM domain_events")
        .unwrap();
    let rows = stmt
        .query_map([], |row| {
            let event_type: String = row.get(0)?;
            let payload: String = row.get(1)?;
            Ok((event_type, payload))
        })
        .unwrap();

    let mut event_count = 0;
    for r in rows {
        let (etype, payload) = r.unwrap();
        event_count += 1;
        assert!(
            !payload.contains(SENTINEL_SECRET),
            "Sentinel found in event payload for event_type: {}",
            etype
        );
    }
    assert!(
        event_count >= 4,
        "Expected at least 4 domain events emitted, found {}",
        event_count
    );
}

#[test]
fn test_security_sentinel_never_leaked_in_provider_status_and_ipc() {
    std::env::set_var("GEMINI_API_KEY", SENTINEL_SECRET);

    // 1. Direct resolver status
    let status = ProviderConfigResolver::get_status();
    assert!(status.configured, "Provider should be configured");
    assert_eq!(status.provider, "gemini");

    let status_json = serde_json::to_string(&status).unwrap();
    assert!(
        !status_json.contains(SENTINEL_SECRET),
        "Sentinel found in ProviderConfigResolver::get_status serialization: {}",
        status_json
    );

    // 2. Tauri IPC command helper status
    let ipc_status = cmd_get_planner_provider_status().unwrap();
    let ipc_json = serde_json::to_string(&ipc_status).unwrap();
    assert!(
        !ipc_json.contains(SENTINEL_SECRET),
        "Sentinel found in cmd_get_planner_provider_status serialization: {}",
        ipc_json
    );

    // 3. Status Debug format
    let debug_str = format!("{status:?}");
    assert!(
        !debug_str.contains(SENTINEL_SECRET),
        "Sentinel found in PlannerProviderStatus Debug representation: {}",
        debug_str
    );
}

#[test]
fn test_security_sentinel_sanitized_in_provider_errors() {
    std::env::set_var("GEMINI_API_KEY", SENTINEL_SECRET);

    // Simulate raw upstream error containing the secret
    let raw_body = format!(
        r#"{{"error": {{"code": 400, "message": "API key {} is invalid", "status": "INVALID_ARGUMENT"}}}}"#,
        SENTINEL_SECRET
    );

    let provider_err = classify_http_status_and_body(400, &raw_body).unwrap_err();

    let err_display = format!("{provider_err}");
    let err_debug = format!("{provider_err:?}");

    assert!(
        !err_display.contains(SENTINEL_SECRET),
        "Sentinel leaked in ProviderError Display: {}",
        err_display
    );
    assert!(
        !err_debug.contains(SENTINEL_SECRET),
        "Sentinel leaked in ProviderError Debug: {}",
        err_debug
    );
}

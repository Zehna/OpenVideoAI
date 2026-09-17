use std::sync::Arc;
use std::time::Duration;
use tempfile::tempdir;

use openvideoai_lib::content_planning::jobs::{
    submit_generate_job, submit_regeneration_job, PlanningOperation, RegenerationJobParams,
    CONTENT_PLAN_JOB_TYPE,
};
use openvideoai_lib::content_planning::model::{
    GenerationOptions, InputType, PlanningRequest, PlanningSource,
};
use openvideoai_lib::content_planning::planner::ContentPlanner;
use openvideoai_lib::content_planning::provider::mock::{MockOutcome, MockProvider};
use openvideoai_lib::content_planning::provider::{
    AIProvider, ProviderError, ProviderRequest, ProviderResponse, DEFAULT_GEMINI_MODEL,
    FALLBACK_GEMINI_MODEL, PRIMARY_GEMINI_MODEL,
};
use openvideoai_lib::content_planning::service::ContentPlanService;
use openvideoai_lib::content_planning::worker::{
    calculate_retry_backoff, extract_error_code, is_transient_capacity_error,
    select_model_for_attempt, start_local_content_plan_worker, GeminiModelRoute,
    LocalContentPlanWorker, RecordingRetrySleeper, RetrySleeper,
};
use openvideoai_lib::db::DbManager;
use openvideoai_lib::events::bus::EventBus;
use openvideoai_lib::jobs::manager::JobManager;
use openvideoai_lib::jobs::model::{JobState, JobSubmission};
use openvideoai_lib::projects::model::Project;
use openvideoai_lib::projects::repository::ProjectRepository;

#[allow(clippy::type_complexity)]
fn setup_test_context(
    mock_outcome: MockOutcome,
) -> (
    tempfile::TempDir,
    Arc<DbManager>,
    Arc<JobManager>,
    Arc<ContentPlanService>,
    Arc<ContentPlanner<Arc<MockProvider>>>,
    Arc<LocalContentPlanWorker<Arc<MockProvider>>>,
) {
    let dir = tempdir().expect("tempdir failed");
    let db_path = dir.path().join("worker_test.db");
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
    let mock_provider = Arc::new(MockProvider::new(mock_outcome));
    let planner = Arc::new(ContentPlanner::new(mock_provider));

    let worker = Arc::new(LocalContentPlanWorker::new(
        "worker-test-1".to_string(),
        job_mgr.clone(),
        planner.clone(),
        plan_service.clone(),
    ));

    (dir, db, job_mgr, plan_service, planner, worker)
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
        input_text: "AI Video Revolution".to_string(),
        target_duration_ms: 30000,
        language: "en".to_string(),
        tone: "informative".to_string(),
        source_context: vec![sample_source("src-1")],
        generation_options: GenerationOptions {},
    }
}

#[test]
fn test_worker_leases_only_content_plan_jobs() {
    let (_dir, _db, job_mgr, _service, _planner, worker) = setup_test_context(MockOutcome::Success);

    // Submit a timeline render job
    let timeline_sub = JobSubmission {
        job_type: "render.timeline".to_string(),
        payload: "{}".to_string(),
        payload_version: Some(1),
        priority: Some(10),
        idempotency_key: Some("render:1".to_string()),
        project_id: Some("proj-1".to_string()),
        max_retries: Some(3),
    };
    let timeline_job = job_mgr.submit_job(timeline_sub).unwrap();

    // Content plan worker attempt to acquire next job should not lease timeline job
    let acquired = worker.acquire_job().unwrap();
    assert!(
        acquired.is_none(),
        "Worker must ignore non-content.plan jobs"
    );

    // Submit content plan job
    let req = sample_request("proj-1");
    let plan_job = submit_generate_job(&job_mgr, "proj-1", req, None, None).unwrap();

    // Content plan worker acquires it
    let acquired = worker
        .acquire_job()
        .unwrap()
        .expect("should acquire content.plan job");
    assert_eq!(acquired.id, plan_job.id);
    assert_eq!(acquired.job_type, CONTENT_PLAN_JOB_TYPE);

    let _ = timeline_job;
}

#[test]
fn test_worker_generate_flow_success() {
    let (_dir, _db, job_mgr, plan_service, _planner, worker) =
        setup_test_context(MockOutcome::Success);
    let req = sample_request("proj-1");
    let job = submit_generate_job(&job_mgr, "proj-1", req, None, None).unwrap();

    let leased_job = worker.acquire_job().unwrap().expect("should acquire");
    let result = worker.process_one_job(&leased_job);
    assert!(
        result.is_ok(),
        "process_one_job should succeed: {:?}",
        result
    );

    // Verify job status is Completed
    let final_job = job_mgr.get_job(&job.id).unwrap().unwrap();
    assert_eq!(final_job.status, JobState::Completed);

    // Verify all checkpoints exist
    let checkpoints = job_mgr.get_checkpoints(&job.id).unwrap();
    let stages: Vec<String> = checkpoints.into_iter().map(|c| c.stage).collect();
    assert_eq!(
        stages,
        vec![
            "validate_request",
            "prepare_context",
            "generate_outline",
            "validate_outline",
            "generate_content",
            "validate_content",
            "persist_revision",
            "finalize",
        ]
    );

    // Verify plan exists in DB as revision 1
    let plans = plan_service.list_project_plans("proj-1", false).unwrap();
    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0].current_revision, 1);
}

#[test]
fn test_worker_regenerate_scene_success() {
    let (_dir, _db, job_mgr, plan_service, _planner, worker) =
        setup_test_context(MockOutcome::Success);

    // 1. First generate revision 1
    let req = sample_request("proj-1");
    let _gen_job = submit_generate_job(&job_mgr, "proj-1", req.clone(), None, None).unwrap();
    let leased_gen = worker.acquire_job().unwrap().unwrap();
    worker.process_one_job(&leased_gen).unwrap();

    let plans = plan_service.list_project_plans("proj-1", false).unwrap();
    let plan_id = plans[0].id.clone();
    let rev1 = plan_service.get_plan(&plan_id).unwrap().unwrap();
    let target_scene_id = rev1.scenes[0].id.clone();

    // 2. Submit regenerate scene job
    let params = RegenerationJobParams {
        project_id: "proj-1".to_string(),
        plan_id: plan_id.clone(),
        base_revision: 1,
        operation: PlanningOperation::RegenerateScene,
        scene_id: Some(target_scene_id.clone()),
        user_instruction: Some("Make opening more punchy".to_string()),
        planning_request: req.clone(),
        source_context: req.source_context.clone(),
        provider: None,
        provider_config_ref: None,
    };
    let regen_job = submit_regeneration_job(&job_mgr, params).unwrap();

    let leased_regen = worker.acquire_job().unwrap().unwrap();
    assert_eq!(leased_regen.id, regen_job.id);
    let result = worker.process_one_job(&leased_regen);
    assert!(result.is_ok(), "Regen should succeed: {:?}", result);

    // Verify job completed and checkpoints recorded
    let final_job = job_mgr.get_job(&regen_job.id).unwrap().unwrap();
    assert_eq!(final_job.status, JobState::Completed);

    let checkpoints = job_mgr.get_checkpoints(&regen_job.id).unwrap();
    let stages: Vec<String> = checkpoints.into_iter().map(|c| c.stage).collect();
    assert_eq!(
        stages,
        vec![
            "validate_request",
            "load_base_revision",
            "prepare_scene_context",
            "generate_scene",
            "validate_scene",
            "persist_revision",
            "finalize",
        ]
    );

    // Verify plan revision is now 2
    let rev2 = plan_service.get_plan(&plan_id).unwrap().unwrap();
    assert_eq!(rev2.revision, 2);
}

#[test]
fn test_worker_regenerate_base_revision_conflict() {
    let (_dir, _db, job_mgr, plan_service, _planner, worker) =
        setup_test_context(MockOutcome::Success);

    // 1. Generate revision 1
    let req = sample_request("proj-1");
    let _gen_job = submit_generate_job(&job_mgr, "proj-1", req.clone(), None, None).unwrap();
    let leased_gen = worker.acquire_job().unwrap().unwrap();
    worker.process_one_job(&leased_gen).unwrap();

    let plans = plan_service.list_project_plans("proj-1", false).unwrap();
    let plan_id = plans[0].id.clone();

    // 2. Submit regenerate plan job based on revision 1
    let params = RegenerationJobParams {
        project_id: "proj-1".to_string(),
        plan_id: plan_id.clone(),
        base_revision: 1,
        operation: PlanningOperation::RegeneratePlan,
        scene_id: None,
        user_instruction: None,
        planning_request: req.clone(),
        source_context: req.source_context.clone(),
        provider: None,
        provider_config_ref: None,
    };
    let regen_job = submit_regeneration_job(&job_mgr, params).unwrap();

    // 3. Concurrently, someone saves a manual edit -> DB advances to revision 2!
    let mut edited = plan_service.get_plan(&plan_id).unwrap().unwrap();
    edited.title = "Manually Edited Title".to_string();
    plan_service
        .save_manual_revision(&plan_id, 1, edited)
        .unwrap();

    // 4. Worker now processes the queued regen job which had base_revision = 1
    let leased_regen = worker.acquire_job().unwrap().unwrap();
    let result = worker.process_one_job(&leased_regen);
    assert!(result.is_err(), "Must fail due to base revision conflict");

    // Job must be failed with allow_retry = false (Failed, retry_count = 0)
    let final_job = job_mgr.get_job(&regen_job.id).unwrap().unwrap();
    assert_eq!(final_job.status, JobState::Failed);
    assert_eq!(final_job.retry_count, 0);
    assert!(
        final_job.error.as_deref().unwrap().contains("conflict"),
        "Error was: {:?}",
        final_job.error
    );

    // Plan revision remains 2 (conflict was not published)
    let current = plan_service.get_plan(&plan_id).unwrap().unwrap();
    assert_eq!(current.revision, 2);
}

#[test]
fn test_worker_cancelled_before_provider_aborts() {
    let (_dir, _db, job_mgr, plan_service, _planner, worker) =
        setup_test_context(MockOutcome::Success);
    let req = sample_request("proj-1");
    let job = submit_generate_job(&job_mgr, "proj-1", req, None, None).unwrap();

    let leased_job = worker.acquire_job().unwrap().unwrap();

    // User cancels the job
    job_mgr.cancel_job(&job.id).unwrap();

    let result = worker.process_one_job(&leased_job);
    assert!(result.is_err(), "Must abort on cancelled job");

    // No plan should exist in DB
    let plans = plan_service.list_project_plans("proj-1", false).unwrap();
    assert_eq!(plans.len(), 0);
}

#[test]
fn test_worker_retryable_error() {
    let (_dir, _db, job_mgr, _service, _planner, worker) =
        setup_test_context(MockOutcome::RateLimited);
    let req = sample_request("proj-1");
    let job = submit_generate_job(&job_mgr, "proj-1", req, None, None).unwrap();

    let leased_job = worker.acquire_job().unwrap().unwrap();
    let result = worker.process_one_job(&leased_job);
    assert!(result.is_err());

    let final_job = job_mgr.get_job(&job.id).unwrap().unwrap();
    assert_eq!(final_job.status, JobState::Retryable);
}

#[test]
fn test_worker_non_retryable_auth_error() {
    let (_dir, _db, job_mgr, _service, _planner, worker) =
        setup_test_context(MockOutcome::AuthFailed);
    let req = sample_request("proj-1");
    let job = submit_generate_job(&job_mgr, "proj-1", req, None, None).unwrap();

    let leased_job = worker.acquire_job().unwrap().unwrap();
    let result = worker.process_one_job(&leased_job);
    assert!(result.is_err());

    let final_job = job_mgr.get_job(&job.id).unwrap().unwrap();
    assert_eq!(final_job.status, JobState::Failed);
    assert_eq!(final_job.retry_count, 0);
}

#[test]
fn test_worker_non_retryable_model_unavailable() {
    let (_dir, _db, job_mgr, _service, _planner, worker) =
        setup_test_context(MockOutcome::ModelUnavailable);
    let req = sample_request("proj-1");
    let job = submit_generate_job(&job_mgr, "proj-1", req, None, None).unwrap();

    let leased_job = worker.acquire_job().unwrap().unwrap();
    let result = worker.process_one_job(&leased_job);
    assert!(result.is_err());

    let final_job = job_mgr.get_job(&job.id).unwrap().unwrap();
    assert_eq!(final_job.status, JobState::Failed);
    assert_eq!(final_job.retry_count, 0);
    assert!(
        final_job.error.as_deref().unwrap().contains("model"),
        "Error was: {:?}",
        final_job.error
    );
}

#[tokio::test]
async fn test_worker_loop_and_cooperative_shutdown() {
    let (_dir, _db, job_mgr, plan_service, planner, _worker) =
        setup_test_context(MockOutcome::Success);

    let handle = start_local_content_plan_worker(
        "worker-loop-test".to_string(),
        job_mgr.clone(),
        planner,
        plan_service.clone(),
        Duration::from_millis(50),
    );

    // Submit a job
    let req = sample_request("proj-1");
    let job = submit_generate_job(&job_mgr, "proj-1", req, None, None).unwrap();

    // Wait until job is completed
    let mut completed = false;
    for _ in 0..100 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        if let Ok(Some(j)) = job_mgr.get_job(&job.id) {
            if j.status == JobState::Completed {
                completed = true;
                break;
            }
        }
    }
    assert!(completed, "Background worker must complete the job");

    // Verify plan published
    let plans = plan_service.list_project_plans("proj-1", false).unwrap();
    assert_eq!(plans.len(), 1);

    // Cooperative shutdown
    handle.shutdown().await;
}

struct CancellingTestProvider {
    job_mgr: Arc<JobManager>,
    job_id: std::sync::Mutex<Option<String>>,
}

impl AIProvider for CancellingTestProvider {
    fn name(&self) -> &'static str {
        "cancelling-mock"
    }

    fn generate_planning_draft(
        &self,
        _input: &ProviderRequest,
    ) -> Result<ProviderResponse, ProviderError> {
        if let Some(ref j_id) = *self.job_id.lock().unwrap() {
            let _ = self.job_mgr.cancel_job(j_id);
        }
        Ok(ProviderResponse {
            text: "Draft outline text".to_string(),
            provider: "cancelling-mock".to_string(),
            model: "mock-model".to_string(),
            usage: None,
        })
    }

    fn generate_content_plan(
        &self,
        _input: &ProviderRequest,
    ) -> Result<ProviderResponse, ProviderError> {
        panic!("Should not reach content plan expansion when cancelled during draft stage");
    }

    fn regenerate_plan(&self, _input: &ProviderRequest) -> Result<ProviderResponse, ProviderError> {
        unimplemented!()
    }

    fn regenerate_scene(
        &self,
        _input: &ProviderRequest,
    ) -> Result<ProviderResponse, ProviderError> {
        unimplemented!()
    }

    fn regenerate_narration(
        &self,
        _input: &ProviderRequest,
    ) -> Result<ProviderResponse, ProviderError> {
        unimplemented!()
    }
}

#[test]
fn test_worker_cancellation_during_execution_prevents_publication() {
    let dir = tempdir().expect("tempdir failed");
    let db_path = dir.path().join("cancelling_worker_test.db");
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
    let cancelling_provider = Arc::new(CancellingTestProvider {
        job_mgr: job_mgr.clone(),
        job_id: std::sync::Mutex::new(None),
    });
    let planner = Arc::new(ContentPlanner::new(cancelling_provider.clone()));
    let worker = LocalContentPlanWorker::new(
        "worker-cancel-test".to_string(),
        job_mgr.clone(),
        planner,
        plan_service.clone(),
    );

    let req = sample_request("proj-1");
    let job = submit_generate_job(&job_mgr, "proj-1", req, None, None).unwrap();
    *cancelling_provider.job_id.lock().unwrap() = Some(job.id.clone());

    let leased_job = worker.acquire_job().unwrap().unwrap();
    let result = worker.process_one_job(&leased_job);
    assert!(
        result.is_err(),
        "Worker must abort when cancelled mid-execution"
    );

    // Verify job remains in Cancelled state (not overwritten with DeadLetter/Failed)
    let final_job = job_mgr.get_job(&job.id).unwrap().unwrap();
    assert_eq!(final_job.status, JobState::Cancelled);

    // Verify zero plans published
    let plans = plan_service.list_project_plans("proj-1", false).unwrap();
    assert_eq!(plans.len(), 0);
}

struct StolenLeaseProvider {
    db: Arc<DbManager>,
    job_id: std::sync::Mutex<Option<String>>,
}

impl AIProvider for StolenLeaseProvider {
    fn name(&self) -> &'static str {
        "stolen-lease-mock"
    }

    fn generate_planning_draft(
        &self,
        _input: &ProviderRequest,
    ) -> Result<ProviderResponse, ProviderError> {
        // Change lease token directly in DB before persist to simulate stolen lease
        if let Some(ref j_id) = *self.job_id.lock().unwrap() {
            let conn = self.db.conn().unwrap();
            conn.execute(
                "UPDATE jobs SET lease_token = 'stolen-token-123' WHERE id = ?1",
                rusqlite::params![j_id],
            )
            .unwrap();
        }
        Ok(ProviderResponse {
            text: "Draft outline text".to_string(),
            provider: "stolen-lease-mock".to_string(),
            model: "mock-model".to_string(),
            usage: None,
        })
    }

    fn generate_content_plan(
        &self,
        _input: &ProviderRequest,
    ) -> Result<ProviderResponse, ProviderError> {
        let mock = MockProvider::new(MockOutcome::Success);
        mock.generate_content_plan(_input)
    }

    fn regenerate_plan(&self, _input: &ProviderRequest) -> Result<ProviderResponse, ProviderError> {
        unimplemented!()
    }

    fn regenerate_scene(
        &self,
        _input: &ProviderRequest,
    ) -> Result<ProviderResponse, ProviderError> {
        unimplemented!()
    }

    fn regenerate_narration(
        &self,
        _input: &ProviderRequest,
    ) -> Result<ProviderResponse, ProviderError> {
        unimplemented!()
    }
}

#[test]
fn test_worker_stolen_lease_before_persistence_prevents_publication() {
    let dir = tempdir().expect("tempdir failed");
    let db_path = dir.path().join("stolen_worker_test.db");
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
    let stolen_provider = Arc::new(StolenLeaseProvider {
        db: db.clone(),
        job_id: std::sync::Mutex::new(None),
    });
    let planner = Arc::new(ContentPlanner::new(stolen_provider.clone()));
    let worker = LocalContentPlanWorker::new(
        "worker-stolen-test".to_string(),
        job_mgr.clone(),
        planner,
        plan_service.clone(),
    );

    let req = sample_request("proj-1");
    let job = submit_generate_job(&job_mgr, "proj-1", req, None, None).unwrap();
    *stolen_provider.job_id.lock().unwrap() = Some(job.id.clone());

    let leased_job = worker.acquire_job().unwrap().unwrap();
    let result = worker.process_one_job(&leased_job);
    assert!(
        result.is_err(),
        "Worker must abort when lease token changed"
    );

    // Verify zero plans published
    let plans = plan_service.list_project_plans("proj-1", false).unwrap();
    assert_eq!(plans.len(), 0);
}

struct SlowMockProvider {
    inner: MockProvider,
    sleep_dur: Duration,
}

impl AIProvider for SlowMockProvider {
    fn name(&self) -> &'static str {
        "slow-mock"
    }

    fn generate_planning_draft(
        &self,
        input: &ProviderRequest,
    ) -> Result<ProviderResponse, ProviderError> {
        std::thread::sleep(self.sleep_dur);
        self.inner.generate_planning_draft(input)
    }

    fn generate_content_plan(
        &self,
        input: &ProviderRequest,
    ) -> Result<ProviderResponse, ProviderError> {
        std::thread::sleep(self.sleep_dur);
        self.inner.generate_content_plan(input)
    }

    fn regenerate_plan(&self, input: &ProviderRequest) -> Result<ProviderResponse, ProviderError> {
        self.inner.regenerate_plan(input)
    }

    fn regenerate_scene(&self, input: &ProviderRequest) -> Result<ProviderResponse, ProviderError> {
        self.inner.regenerate_scene(input)
    }

    fn regenerate_narration(
        &self,
        input: &ProviderRequest,
    ) -> Result<ProviderResponse, ProviderError> {
        self.inner.regenerate_narration(input)
    }
}

#[test]
fn test_worker_heartbeat_maintains_lease_during_long_execution() {
    let dir = tempdir().expect("tempdir failed");
    let db_path = dir.path().join("heartbeat_worker_test.db");
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
    let slow_provider = Arc::new(SlowMockProvider {
        inner: MockProvider::new(MockOutcome::Success),
        sleep_dur: Duration::from_millis(60),
    });
    let planner = Arc::new(ContentPlanner::new(slow_provider));
    // Fast heartbeat interval (20ms) so heartbeat fires multiple times during the 60ms sleep
    let worker = LocalContentPlanWorker::new(
        "worker-heartbeat-test".to_string(),
        job_mgr.clone(),
        planner,
        plan_service.clone(),
    )
    .with_heartbeat_interval(Duration::from_millis(20));

    let req = sample_request("proj-1");
    let job = submit_generate_job(&job_mgr, "proj-1", req, None, None).unwrap();

    let leased_job = worker.acquire_job().unwrap().unwrap();
    let result = worker.process_one_job(&leased_job);
    assert!(
        result.is_ok(),
        "Worker with heartbeats must succeed: {:?}",
        result
    );

    let final_job = job_mgr.get_job(&job.id).unwrap().unwrap();
    assert_eq!(final_job.status, JobState::Completed);

    let plans = plan_service.list_project_plans("proj-1", false).unwrap();
    assert_eq!(plans.len(), 1);
}

// ---------------------------------------------------------------------------
// Phase 4: 503 Retry Backoff & Jitter Tests (A to H)
// ---------------------------------------------------------------------------

#[test]
fn test_backoff_function_properties() {
    // retry_count <= 0 -> Duration::ZERO
    assert_eq!(calculate_retry_backoff("job-1", 0), Duration::ZERO);
    assert_eq!(calculate_retry_backoff("job-1", -1), Duration::ZERO);

    // retry_count == 1 -> >= 2000ms and <= 2500ms
    let d1 = calculate_retry_backoff("job-1", 1);
    assert!(
        d1 >= Duration::from_millis(2000) && d1 <= Duration::from_millis(2500),
        "retry 1 delay out of range: {:?}",
        d1
    );

    // retry_count == 2 -> >= 4000ms and <= 5000ms
    let d2 = calculate_retry_backoff("job-1", 2);
    assert!(
        d2 >= Duration::from_millis(4000) && d2 <= Duration::from_millis(5000),
        "retry 2 delay out of range: {:?}",
        d2
    );

    // retry_count == 3 -> >= 8000ms and <= 10000ms
    let d3 = calculate_retry_backoff("job-1", 3);
    assert!(
        d3 >= Duration::from_millis(8000) && d3 <= Duration::from_millis(10000),
        "retry 3 delay out of range: {:?}",
        d3
    );

    // retry_count > 3 -> never exceeds configured cap (10000ms)
    let d4 = calculate_retry_backoff("job-1", 4);
    assert!(
        d4 <= Duration::from_millis(10000),
        "retry 4 delay exceeds cap: {:?}",
        d4
    );
    assert!(d4 >= Duration::from_millis(8000));

    let d10 = calculate_retry_backoff("job-1", 10);
    assert!(
        d10 <= Duration::from_millis(10000),
        "retry 10 delay exceeds cap: {:?}",
        d10
    );

    // Deterministic: same job ID + retry count produces identical duration
    assert_eq!(
        calculate_retry_backoff("job-deterministic", 1),
        calculate_retry_backoff("job-deterministic", 1)
    );
    assert_eq!(
        calculate_retry_backoff("job-deterministic", 2),
        calculate_retry_backoff("job-deterministic", 2)
    );
}

#[test]
fn test_worker_first_attempt_has_zero_backoff() {
    let (_dir, _db, job_mgr, plan_service, planner, _worker) =
        setup_test_context(MockOutcome::Success);
    let sleeper = Arc::new(RecordingRetrySleeper::new());
    let worker = LocalContentPlanWorker::new(
        "worker-test-b".to_string(),
        job_mgr.clone(),
        planner,
        plan_service,
    )
    .with_sleeper(sleeper.clone());

    let req = sample_request("proj-1");
    let job = submit_generate_job(&job_mgr, "proj-1", req, None, None).unwrap();
    assert_eq!(job.retry_count, 0);

    let leased_job = worker.acquire_job().unwrap().unwrap();
    let result = worker.process_one_job(&leased_job);
    assert!(result.is_ok());

    // First attempt must NOT sleep
    assert_eq!(sleeper.total_slept(), Duration::ZERO);
}

#[test]
fn test_worker_transient_503_retries_with_backoff_and_succeeds() {
    let (_dir, _db, job_mgr, plan_service, _planner, _worker) =
        setup_test_context(MockOutcome::Success);

    // Mock outcome: 503 once, then succeeds on call 2
    let mock_provider = Arc::new(MockProvider::new(MockOutcome::TransientUnavailableOnce));
    let planner = Arc::new(ContentPlanner::new(mock_provider));
    let sleeper = Arc::new(RecordingRetrySleeper::new());
    let worker = LocalContentPlanWorker::new(
        "worker-test-c".to_string(),
        job_mgr.clone(),
        planner,
        plan_service.clone(),
    )
    .with_sleeper(sleeper.clone());

    let req = sample_request("proj-1");
    let job = submit_generate_job(&job_mgr, "proj-1", req, None, None).unwrap();

    // 1. First attempt: provider returns 503
    let leased_1 = worker.acquire_job().unwrap().unwrap();
    assert_eq!(leased_1.retry_count, 0);
    let res1 = worker.process_one_job(&leased_1);
    assert!(res1.is_err(), "First attempt must fail with 503");

    // Job transitions to retryable with retry_count = 1
    let j1 = job_mgr.get_job(&job.id).unwrap().unwrap();
    assert_eq!(j1.status, JobState::Retryable);
    assert_eq!(j1.retry_count, 1);
    assert_eq!(
        sleeper.total_slept(),
        Duration::ZERO,
        "First attempt had no delay"
    );

    // 2. Second attempt (retry 1): provider will succeed, but worker must observe backoff first
    let leased_2 = worker.acquire_job().unwrap().unwrap();
    assert_eq!(leased_2.retry_count, 1);
    let res2 = worker.process_one_job(&leased_2);
    assert!(res2.is_ok(), "Second attempt must succeed: {:?}", res2);

    // Job transitions to completed with retry_count = 1
    let j2 = job_mgr.get_job(&job.id).unwrap().unwrap();
    assert_eq!(j2.status, JobState::Completed);
    assert_eq!(j2.retry_count, 1);

    // Sleeper recorded backoff delay for retry 1 (between 2000ms and 2500ms)
    let slept = sleeper.total_slept();
    assert!(
        slept >= Duration::from_millis(2000) && slept <= Duration::from_millis(2500),
        "Second attempt slept out of expected retry 1 backoff range: {:?}",
        slept
    );

    // Content plan exists and revision is 1
    let plans = plan_service.list_project_plans("proj-1", false).unwrap();
    assert_eq!(plans.len(), 1);
}

#[test]
fn test_worker_repeated_transient_503_increases_backoff_delays() {
    let (_dir, _db, job_mgr, plan_service, _planner, _worker) =
        setup_test_context(MockOutcome::Unavailable);

    let mock_provider = Arc::new(MockProvider::new(MockOutcome::Unavailable));
    let planner = Arc::new(ContentPlanner::new(mock_provider));
    let sleeper = Arc::new(RecordingRetrySleeper::new());
    let worker = LocalContentPlanWorker::new(
        "worker-test-d".to_string(),
        job_mgr.clone(),
        planner,
        plan_service,
    )
    .with_sleeper(sleeper.clone());

    let req = sample_request("proj-1");
    let job = submit_generate_job(&job_mgr, "proj-1", req, None, None).unwrap();

    // Attempt 1 (retry_count 0): fails -> retry_count becomes 1
    let l1 = worker.acquire_job().unwrap().unwrap();
    let _ = worker.process_one_job(&l1);
    let j1 = job_mgr.get_job(&job.id).unwrap().unwrap();
    assert_eq!(j1.status, JobState::Retryable);
    assert_eq!(j1.retry_count, 1);
    let s1 = sleeper.total_slept();
    assert_eq!(s1, Duration::ZERO);

    // Attempt 2 (retry_count 1): fails -> retry_count becomes 2
    let l2 = worker.acquire_job().unwrap().unwrap();
    let _ = worker.process_one_job(&l2);
    let j2 = job_mgr.get_job(&job.id).unwrap().unwrap();
    assert_eq!(j2.status, JobState::Retryable);
    assert_eq!(j2.retry_count, 2);
    let s2 = sleeper.total_slept();
    assert!(s2 >= Duration::from_millis(2000) && s2 <= Duration::from_millis(2500));

    // Attempt 3 (retry_count 2): fails -> retry_count becomes 3
    let l3 = worker.acquire_job().unwrap().unwrap();
    let _ = worker.process_one_job(&l3);
    let j3 = job_mgr.get_job(&job.id).unwrap().unwrap();
    assert_eq!(j3.status, JobState::Retryable);
    assert_eq!(j3.retry_count, 3);
    let s3 = sleeper.total_slept() - s2;
    assert!(s3 >= Duration::from_millis(4000) && s3 <= Duration::from_millis(5000));
    assert!(s3 > s2, "Retry 2 delay must be greater than Retry 1 delay");

    // Attempt 4 (retry_count 3 == max_retries): fails -> status becomes dead_letter
    let l4 = worker.acquire_job().unwrap().unwrap();
    let _ = worker.process_one_job(&l4);
    let j4 = job_mgr.get_job(&job.id).unwrap().unwrap();
    assert_eq!(j4.status, JobState::DeadLetter);
    let s4 = sleeper.total_slept() - s2 - s3;
    assert!(s4 >= Duration::from_millis(8000) && s4 <= Duration::from_millis(10000));
    assert!(s4 > s3, "Retry 3 delay must be greater than Retry 2 delay");
}

#[test]
fn test_worker_auth_error_no_backoff_and_fails_immediately() {
    let (_dir, _db, job_mgr, plan_service, _planner, _worker) =
        setup_test_context(MockOutcome::AuthFailed);

    let mock_provider = Arc::new(MockProvider::new(MockOutcome::AuthFailed));
    let planner = Arc::new(ContentPlanner::new(mock_provider));
    let sleeper = Arc::new(RecordingRetrySleeper::new());
    let worker = LocalContentPlanWorker::new(
        "worker-test-e".to_string(),
        job_mgr.clone(),
        planner,
        plan_service,
    )
    .with_sleeper(sleeper.clone());

    let req = sample_request("proj-1");
    let job = submit_generate_job(&job_mgr, "proj-1", req, None, None).unwrap();

    let l = worker.acquire_job().unwrap().unwrap();
    let res = worker.process_one_job(&l);
    assert!(res.is_err());

    let final_job = job_mgr.get_job(&job.id).unwrap().unwrap();
    assert_eq!(final_job.status, JobState::Failed);
    assert_eq!(final_job.retry_count, 0);
    assert_eq!(sleeper.total_slept(), Duration::ZERO);
}

#[test]
fn test_worker_model_unavailable_no_backoff_and_fails_immediately() {
    let (_dir, _db, job_mgr, plan_service, _planner, _worker) =
        setup_test_context(MockOutcome::ModelUnavailable);

    let mock_provider = Arc::new(MockProvider::new(MockOutcome::ModelUnavailable));
    let planner = Arc::new(ContentPlanner::new(mock_provider));
    let sleeper = Arc::new(RecordingRetrySleeper::new());
    let worker = LocalContentPlanWorker::new(
        "worker-test-f".to_string(),
        job_mgr.clone(),
        planner,
        plan_service,
    )
    .with_sleeper(sleeper.clone());

    let req = sample_request("proj-1");
    let job = submit_generate_job(&job_mgr, "proj-1", req, None, None).unwrap();

    let l = worker.acquire_job().unwrap().unwrap();
    let res = worker.process_one_job(&l);
    assert!(res.is_err());

    let final_job = job_mgr.get_job(&job.id).unwrap().unwrap();
    assert_eq!(final_job.status, JobState::Failed);
    assert_eq!(final_job.retry_count, 0);
    assert_eq!(sleeper.total_slept(), Duration::ZERO);
}

struct CancellingSleeper {
    job_mgr: Arc<JobManager>,
    job_id: String,
}
impl RetrySleeper for CancellingSleeper {
    fn sleep(&self, _duration: Duration) {
        let _ = self.job_mgr.cancel_job(&self.job_id);
    }
}

#[test]
fn test_worker_cancellation_during_backoff_prevents_provider_call() {
    let (_dir, db, job_mgr, plan_service, _planner, _worker) =
        setup_test_context(MockOutcome::Success);

    let mock_provider = Arc::new(MockProvider::new(MockOutcome::Success));
    let planner = Arc::new(ContentPlanner::new(mock_provider.clone()));

    let req = sample_request("proj-1");
    let job = submit_generate_job(&job_mgr, "proj-1", req, None, None).unwrap();

    // Simulate job already on retry_count = 1
    {
        let conn = db.conn().unwrap();
        conn.execute(
            "UPDATE jobs SET status = 'retryable', retry_count = 1 WHERE id = ?1",
            rusqlite::params![job.id],
        )
        .unwrap();
    }

    let sleeper = Arc::new(CancellingSleeper {
        job_mgr: job_mgr.clone(),
        job_id: job.id.clone(),
    });

    let worker = LocalContentPlanWorker::new(
        "worker-test-g".to_string(),
        job_mgr.clone(),
        planner,
        plan_service.clone(),
    )
    .with_sleeper(sleeper);

    let l = worker.acquire_job().unwrap().unwrap();
    assert_eq!(l.retry_count, 1);
    let res = worker.process_one_job(&l);
    assert!(res.is_err());

    // Provider call count must be 0 (provider was never invoked!)
    assert_eq!(mock_provider.call_count(), 0);

    // Job status must remain Cancelled (not overwritten)
    let final_job = job_mgr.get_job(&job.id).unwrap().unwrap();
    assert_eq!(final_job.status, JobState::Cancelled);

    // Zero plans published
    let plans = plan_service.list_project_plans("proj-1", false).unwrap();
    assert_eq!(plans.len(), 0);
}

struct StealingLeaseSleeper {
    db: Arc<DbManager>,
    job_id: String,
}
impl RetrySleeper for StealingLeaseSleeper {
    fn sleep(&self, _duration: Duration) {
        let conn = self.db.conn().unwrap();
        conn.execute(
            "UPDATE jobs SET lease_token = 'stolen-token-xyz' WHERE id = ?1",
            rusqlite::params![self.job_id],
        )
        .unwrap();
    }
}

#[test]
fn test_worker_stale_lease_during_backoff_aborts_safely() {
    let (_dir, db, job_mgr, plan_service, _planner, _worker) =
        setup_test_context(MockOutcome::Success);

    let mock_provider = Arc::new(MockProvider::new(MockOutcome::Success));
    let planner = Arc::new(ContentPlanner::new(mock_provider.clone()));

    let req = sample_request("proj-1");
    let job = submit_generate_job(&job_mgr, "proj-1", req, None, None).unwrap();

    // Simulate job on retry_count = 1
    {
        let conn = db.conn().unwrap();
        conn.execute(
            "UPDATE jobs SET status = 'retryable', retry_count = 1 WHERE id = ?1",
            rusqlite::params![job.id],
        )
        .unwrap();
    }

    let sleeper = Arc::new(StealingLeaseSleeper {
        db: db.clone(),
        job_id: job.id.clone(),
    });

    let worker = LocalContentPlanWorker::new(
        "worker-test-h".to_string(),
        job_mgr.clone(),
        planner,
        plan_service.clone(),
    )
    .with_sleeper(sleeper);

    let l = worker.acquire_job().unwrap().unwrap();
    assert_eq!(l.retry_count, 1);
    let res = worker.process_one_job(&l);
    assert!(
        res.is_err(),
        "Worker must abort when lease is stolen during backoff"
    );

    // Provider call count must be 0
    assert_eq!(mock_provider.call_count(), 0);

    // Zero plans published
    let plans = plan_service.list_project_plans("proj-1", false).unwrap();
    assert_eq!(plans.len(), 0);
}

#[test]
fn test_fallback_constants() {
    assert_eq!(PRIMARY_GEMINI_MODEL, "gemini-3.8-flash");
    assert_eq!(FALLBACK_GEMINI_MODEL, "gemini-3.6-flash");
    assert_eq!(DEFAULT_GEMINI_MODEL, PRIMARY_GEMINI_MODEL);
}

#[test]
fn test_select_model_initial_attempt_uses_primary() {
    let route = select_model_for_attempt(0, 3, None);
    assert_eq!(route, GeminiModelRoute::Primary);
}

#[test]
fn test_select_model_intermediate_attempt_uses_primary() {
    let route1 = select_model_for_attempt(1, 3, Some("provider_unavailable: 503"));
    assert_eq!(route1, GeminiModelRoute::Primary);

    let route2 = select_model_for_attempt(2, 3, Some("provider_unavailable: 503"));
    assert_eq!(route2, GeminiModelRoute::Primary);
}

#[test]
fn test_select_model_final_attempt_with_503_uses_fallback() {
    let route_code = select_model_for_attempt(3, 3, Some("provider_unavailable"));
    assert_eq!(route_code, GeminiModelRoute::Fallback);

    let route_msg =
        select_model_for_attempt(3, 3, Some("provider_unavailable: High demand (HTTP 503)"));
    assert_eq!(route_msg, GeminiModelRoute::Fallback);
}

#[test]
fn test_select_model_final_attempt_with_rate_limit_uses_fallback() {
    let route = select_model_for_attempt(
        3,
        3,
        Some("provider_rate_limited: ResourceExhausted (HTTP 429)"),
    );
    assert_eq!(route, GeminiModelRoute::Fallback);
}

#[test]
fn test_select_model_final_attempt_with_timeout_uses_fallback() {
    let route = select_model_for_attempt(3, 3, Some("provider_timeout: Gateway Timeout"));
    assert_eq!(route, GeminiModelRoute::Fallback);
}

#[test]
fn test_select_model_final_attempt_with_non_retryable_error_uses_primary() {
    let route_auth = select_model_for_attempt(3, 3, Some("provider_auth_failed: invalid key"));
    assert_eq!(route_auth, GeminiModelRoute::Primary);

    let route_model = select_model_for_attempt(3, 3, Some("provider_model_not_found: 404"));
    assert_eq!(route_model, GeminiModelRoute::Primary);

    let route_struct =
        select_model_for_attempt(3, 3, Some("structured_output_invalid: parse error"));
    assert_eq!(route_struct, GeminiModelRoute::Primary);
}

#[test]
fn test_select_model_final_attempt_with_no_error_uses_primary() {
    let route = select_model_for_attempt(3, 3, None);
    assert_eq!(route, GeminiModelRoute::Primary);
}

#[test]
fn test_select_model_restart_safety_from_persisted_row() {
    let persisted_retry_count: i32 = 3;
    let persisted_max_retries: i32 = 3;
    let persisted_error_str = "provider_unavailable: High demand (HTTP 503)";

    let code = extract_error_code(Some(persisted_error_str));
    assert_eq!(code, Some("provider_unavailable"));
    assert!(is_transient_capacity_error(code.unwrap()));

    let route = select_model_for_attempt(
        persisted_retry_count,
        persisted_max_retries,
        Some(persisted_error_str),
    );
    assert_eq!(route, GeminiModelRoute::Fallback);
}

#[test]
fn test_worker_fallback_flow_primary_503_then_fallback_success() {
    let (_dir, _db, job_mgr, plan_service, _planner, worker) =
        setup_test_context(MockOutcome::PrimaryUnavailableThenFallbackSuccess);
    let req = sample_request("proj-1");
    let job = submit_generate_job(&job_mgr, "proj-1", req, None, None).unwrap();

    // Attempt 1: retry_count = 0 -> Primary fails 503 -> Retryable (retry_count becomes 1)
    let l1 = worker.acquire_job().unwrap().unwrap();
    assert_eq!(l1.retry_count, 0);
    assert!(worker.process_one_job(&l1).is_err());
    let j1 = job_mgr.get_job(&job.id).unwrap().unwrap();
    assert_eq!(j1.status, JobState::Retryable);
    assert_eq!(j1.retry_count, 1);

    // Attempt 2: retry_count = 1 -> Primary fails 503 -> Retryable (retry_count becomes 2)
    let l2 = worker.acquire_job().unwrap().unwrap();
    assert_eq!(l2.retry_count, 1);
    assert!(worker.process_one_job(&l2).is_err());
    let j2 = job_mgr.get_job(&job.id).unwrap().unwrap();
    assert_eq!(j2.status, JobState::Retryable);
    assert_eq!(j2.retry_count, 2);

    // Attempt 3: retry_count = 2 -> Primary fails 503 -> Retryable (retry_count becomes 3)
    let l3 = worker.acquire_job().unwrap().unwrap();
    assert_eq!(l3.retry_count, 2);
    assert!(worker.process_one_job(&l3).is_err());
    let j3 = job_mgr.get_job(&job.id).unwrap().unwrap();
    assert_eq!(j3.status, JobState::Retryable);
    assert_eq!(j3.retry_count, 3);

    // Attempt 4: retry_count = 3 (final attempt, retry_count == max_retries).
    // Worker routes to Fallback (gemini-3.6-flash). Fallback succeeds!
    let l4 = worker.acquire_job().unwrap().unwrap();
    assert_eq!(l4.retry_count, 3);
    let res = worker.process_one_job(&l4);
    assert!(res.is_ok(), "Fallback attempt should succeed: {:?}", res);

    let final_job = job_mgr.get_job(&job.id).unwrap().unwrap();
    assert_eq!(final_job.status, JobState::Completed);

    // Verify metadata on generated plan revision
    let plans = plan_service.list_project_plans("proj-1", false).unwrap();
    assert_eq!(plans.len(), 1);
    let plan = plan_service.get_plan(&plans[0].id).unwrap().unwrap();
    let gen_meta = plan
        .generation_metadata
        .expect("generation metadata must be present");
    assert_eq!(gen_meta.model, FALLBACK_GEMINI_MODEL);
    assert_eq!(
        gen_meta.primary_model.as_deref(),
        Some(PRIMARY_GEMINI_MODEL)
    );
    assert_eq!(gen_meta.fallback_used, Some(true));
}

#[test]
fn test_worker_fallback_flow_primary_success_metadata() {
    let (_dir, _db, job_mgr, plan_service, _planner, worker) =
        setup_test_context(MockOutcome::Success);
    let req = sample_request("proj-1");
    let job = submit_generate_job(&job_mgr, "proj-1", req, None, None).unwrap();

    let l1 = worker.acquire_job().unwrap().unwrap();
    assert_eq!(l1.retry_count, 0);
    let res = worker.process_one_job(&l1);
    assert!(res.is_ok(), "Primary attempt should succeed: {:?}", res);

    let final_job = job_mgr.get_job(&job.id).unwrap().unwrap();
    assert_eq!(final_job.status, JobState::Completed);

    let plans = plan_service.list_project_plans("proj-1", false).unwrap();
    assert_eq!(plans.len(), 1);
    let plan = plan_service.get_plan(&plans[0].id).unwrap().unwrap();
    let gen_meta = plan
        .generation_metadata
        .expect("generation metadata must be present");
    assert_eq!(gen_meta.model, PRIMARY_GEMINI_MODEL);
    assert_eq!(
        gen_meta.primary_model.as_deref(),
        Some(PRIMARY_GEMINI_MODEL)
    );
    assert_eq!(gen_meta.fallback_used, Some(false));
}

#[test]
fn test_worker_fallback_exhaustion_dead_letter() {
    let (_dir, _db, job_mgr, plan_service, _planner, worker) =
        setup_test_context(MockOutcome::FallbackUnavailable);
    let req = sample_request("proj-1");
    let job = submit_generate_job(&job_mgr, "proj-1", req, None, None).unwrap();

    // Attempts 1, 2, 3 fail on Primary
    for expected_count in 0..3 {
        let l = worker.acquire_job().unwrap().unwrap();
        assert_eq!(l.retry_count, expected_count);
        assert!(worker.process_one_job(&l).is_err());
    }

    // Attempt 4: retry_count = 3. Routes to fallback, fallback also fails.
    // Since retry_count >= max_retries, it transitions to DeadLetter.
    let l4 = worker.acquire_job().unwrap().unwrap();
    assert_eq!(l4.retry_count, 3);
    assert!(worker.process_one_job(&l4).is_err());

    let final_job = job_mgr.get_job(&job.id).unwrap().unwrap();
    assert_eq!(final_job.status, JobState::DeadLetter);
    assert_eq!(final_job.retry_count, 3);

    // Zero plans published
    let plans = plan_service.list_project_plans("proj-1", false).unwrap();
    assert_eq!(plans.len(), 0);
}

#[test]
fn test_worker_non_transient_does_not_trigger_fallback() {
    let (_dir, _db, job_mgr, plan_service, _planner, worker) =
        setup_test_context(MockOutcome::AuthFailed);
    let req = sample_request("proj-1");
    let job = submit_generate_job(&job_mgr, "proj-1", req, None, None).unwrap();

    let l1 = worker.acquire_job().unwrap().unwrap();
    assert_eq!(l1.retry_count, 0);
    assert!(worker.process_one_job(&l1).is_err());

    let final_job = job_mgr.get_job(&job.id).unwrap().unwrap();
    assert_eq!(final_job.status, JobState::Failed);
    assert_eq!(final_job.retry_count, 0);

    assert!(worker.acquire_job().unwrap().is_none());
    let plans = plan_service.list_project_plans("proj-1", false).unwrap();
    assert_eq!(plans.len(), 0);
}

#[test]
fn test_worker_fallback_respects_backoff_and_lease() {
    let (_dir, _db, job_mgr, plan_service, _planner, _worker) =
        setup_test_context(MockOutcome::PrimaryUnavailableThenFallbackSuccess);

    let sleeper = Arc::new(RecordingRetrySleeper::new());
    let mock_provider = Arc::new(MockProvider::new(
        MockOutcome::PrimaryUnavailableThenFallbackSuccess,
    ));
    let planner = Arc::new(ContentPlanner::new(mock_provider));

    let worker = LocalContentPlanWorker::new(
        "worker-test-backoff".to_string(),
        job_mgr.clone(),
        planner,
        plan_service,
    )
    .with_sleeper(sleeper.clone());

    let req = sample_request("proj-1");
    let _job = submit_generate_job(&job_mgr, "proj-1", req, None, None).unwrap();

    // 3 primary failures
    for _ in 0..3 {
        let l = worker.acquire_job().unwrap().unwrap();
        let _ = worker.process_one_job(&l);
    }

    // 4th attempt (fallback): verify backoff still waited before invocation
    let sleep_count_before = sleeper.recorded_sleeps().len();
    let l4 = worker.acquire_job().unwrap().unwrap();
    assert_eq!(l4.retry_count, 3);
    let res = worker.process_one_job(&l4);
    assert!(res.is_ok());

    let sleep_count_after = sleeper.recorded_sleeps().len();
    assert!(
        sleep_count_after > sleep_count_before,
        "Backoff must have occurred prior to fallback execution"
    );
}

#[test]
fn test_worker_fallback_preserves_idempotency_and_job_identity() {
    let (_dir, _db, job_mgr, plan_service, _planner, worker) =
        setup_test_context(MockOutcome::PrimaryUnavailableThenFallbackSuccess);

    let req = sample_request("proj-1");
    let job = submit_generate_job(&job_mgr, "proj-1", req, None, None).unwrap();
    let original_job_id = job.id.clone();
    let original_idempotency_key = job.idempotency_key.clone();

    // Run attempts 1-3 (failures)
    for _ in 0..3 {
        let l = worker.acquire_job().unwrap().unwrap();
        assert_eq!(l.id, original_job_id);
        let _ = worker.process_one_job(&l);
    }

    // Attempt 4 (fallback success)
    let l4 = worker.acquire_job().unwrap().unwrap();
    assert_eq!(l4.id, original_job_id);
    let res = worker.process_one_job(&l4);
    assert!(res.is_ok());

    let final_job = job_mgr.get_job(&original_job_id).unwrap().unwrap();
    assert_eq!(final_job.id, original_job_id);
    assert_eq!(final_job.idempotency_key, original_idempotency_key);
    assert_eq!(final_job.status, JobState::Completed);

    // Verify exactly one plan created, exactly one job exists
    let plans = plan_service.list_project_plans("proj-1", false).unwrap();
    assert_eq!(plans.len(), 1);
}

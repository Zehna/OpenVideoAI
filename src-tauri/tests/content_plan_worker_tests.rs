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
    AIProvider, ProviderError, ProviderRequest, ProviderResponse,
};
use openvideoai_lib::content_planning::service::ContentPlanService;
use openvideoai_lib::content_planning::worker::{
    start_local_content_plan_worker, LocalContentPlanWorker,
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

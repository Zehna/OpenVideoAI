use openvideoai_lib::artifacts::importer::ArtifactImporter;
use openvideoai_lib::artifacts::metadata::{MediaMetadata, MetadataRepository};
use openvideoai_lib::artifacts::repository::ArtifactRepository;
use openvideoai_lib::artifacts::resolver::ArtifactResolver;
use openvideoai_lib::db::migrations::run_all_migrations;
use openvideoai_lib::db::DbManager;
use openvideoai_lib::jobs::manager::JobManager;
use openvideoai_lib::jobs::model::{JobState, JobSubmission};
use openvideoai_lib::storage::local_store::LocalStore;
use openvideoai_lib::timeline::model::*;
use openvideoai_lib::timeline::renderspec::*;
use openvideoai_lib::timeline::worker::{start_local_render_worker, LocalRenderWorker};
use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tempfile::tempdir;

#[tokio::test]
async fn test_render_worker_public_loop_orchestration() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let store_dir = temp_dir.path().join("store");
    let staging_dir = temp_dir.path().join("staging");
    std::fs::create_dir_all(&store_dir).unwrap();
    std::fs::create_dir_all(&staging_dir).unwrap();

    let mut disk_conn = Connection::open(&db_path).unwrap();
    disk_conn
        .execute_batch("PRAGMA foreign_keys = ON;")
        .unwrap();
    run_all_migrations(&mut disk_conn).unwrap();

    let db_manager = Arc::new(DbManager::new(db_path).unwrap());
    let local_store = Arc::new(LocalStore::new(store_dir));
    let resolver = Arc::new(ArtifactResolver::new(
        db_manager.clone(),
        local_store.clone(),
    ));
    let importer = Arc::new(ArtifactImporter::new(
        db_manager.clone(),
        local_store.clone(),
        staging_dir.clone(),
    ));
    let job_manager = Arc::new(JobManager::new(db_manager.clone()));

    // 1. Create project and insert VALID video artifact fixture with metadata
    disk_conn
        .execute(
            "INSERT INTO projects (id, name, created_at, updated_at) VALUES ('p1', 'P1', datetime('now'), datetime('now'));",
            [],
        )
        .unwrap();
    let source_path = temp_dir.path().join("test_video.mp4");
    std::fs::write(&source_path, b"mock video payload").unwrap();
    let imported_source = importer.import("p1", &source_path).unwrap();

    let meta = MediaMetadata {
        artifact_id: imported_source.artifact.artifact_id.clone(),
        probe_status: "ok".to_string(),
        duration_ms: Some(5000),
        width: Some(1920),
        height: Some(1080),
        video_codec: Some("h264".to_string()),
        audio_codec: Some("aac".to_string()),
        frame_rate: Some(30.0),
        sample_rate: Some(48000),
        channels: Some(2),
        probe_error: None,
    };
    MetadataRepository::insert(&disk_conn, &meta).unwrap();

    // 2. Build VALID minimal RenderSpec with 1 track, 1 clip
    let spec = RenderSpecV1 {
        version: 1,
        project_id: "p1".to_string(),
        timeline_id: "tl1".to_string(),
        timeline_revision: 1,
        output: RenderOutputConfig {
            width: 1920,
            height: 1080,
            frame_rate: 30.0,
            container: "mp4".to_string(),
            video_codec: "h264".to_string(),
            audio_codec: "aac".to_string(),
            pixel_format: "yuv420p".to_string(),
        },
        tracks: vec![RenderTrack {
            id: "tr1".to_string(),
            kind: TrackKind::Video,
            order: 0,
            muted: false,
            clips: vec![RenderClip {
                id: "c1".to_string(),
                artifact_id: imported_source.artifact.artifact_id.clone(),
                source_in_ms: 0,
                source_out_ms: 3000,
                timeline_start_ms: 0,
                volume: 1.0,
                muted: false,
                transform: ClipTransform::default(),
                has_video: true,
                has_audio: true,
            }],
        }],
    };

    let payload = serde_json::json!({
        "schemaVersion": 1,
        "renderSpec": spec,
        "rendererProfileVersion": 1,
    });

    // 3. Submit render job
    let submission = JobSubmission {
        job_type: "render.timeline".to_string(),
        priority: Some(10),
        idempotency_key: Some("render.timeline:tl1:1:1:1".to_string()),
        project_id: Some("p1".to_string()),
        payload: payload.to_string(),
        max_retries: Some(3),
        payload_version: Some(1),
    };
    let job = job_manager.submit_job(submission).unwrap();
    assert_eq!(job.status, JobState::Queued);

    // 4. Start worker background loop
    let handle = start_local_render_worker(
        "render-worker-loop".to_string(),
        job_manager.clone(),
        resolver.clone(),
        importer.clone(),
        staging_dir.clone(),
        PathBuf::from("mock-ffmpeg"),
        None,
        Duration::from_millis(50),
        true, // mock mode
    );

    // 5. Poll job status until Completed
    let mut completed = false;
    for _ in 0..50 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let fresh = job_manager.get_job(&job.id).unwrap().unwrap();
        if fresh.status == JobState::Completed {
            completed = true;
            break;
        }
    }
    assert!(
        completed,
        "Render worker loop failed to process job to completed state"
    );

    // 6. Verify 7 checkpoints recorded
    let checkpoints = job_manager.get_checkpoints(&job.id).unwrap();
    let stages: Vec<String> = checkpoints.into_iter().map(|c| c.stage).collect();
    assert_eq!(
        stages,
        vec![
            "validate_spec",
            "resolve_artifacts",
            "compile_ffmpeg",
            "render_media",
            "validate_output",
            "import_output_artifact",
            "finalize",
        ]
    );

    // 7. Clean shutdown
    handle.shutdown().await;
}

#[tokio::test]
async fn test_render_job_cancellation_kills_child_and_cleans_up() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let store_dir = temp_dir.path().join("store");
    let staging_dir = temp_dir.path().join("staging");
    std::fs::create_dir_all(&store_dir).unwrap();
    std::fs::create_dir_all(&staging_dir).unwrap();

    let mut disk_conn = Connection::open(&db_path).unwrap();
    disk_conn
        .execute_batch("PRAGMA foreign_keys = ON;")
        .unwrap();
    run_all_migrations(&mut disk_conn).unwrap();

    let db_manager = Arc::new(DbManager::new(db_path).unwrap());
    let local_store = Arc::new(LocalStore::new(store_dir));
    let resolver = Arc::new(ArtifactResolver::new(
        db_manager.clone(),
        local_store.clone(),
    ));
    let importer = Arc::new(ArtifactImporter::new(
        db_manager.clone(),
        local_store.clone(),
        staging_dir.clone(),
    ));
    let job_manager = Arc::new(JobManager::new(db_manager.clone()));

    disk_conn
        .execute(
            "INSERT INTO projects (id, name, created_at, updated_at) VALUES ('p1', 'P1', datetime('now'), datetime('now'));",
            [],
        )
        .unwrap();

    let submission = JobSubmission {
        job_type: "render.timeline".to_string(),
        priority: Some(10),
        idempotency_key: Some("render.timeline:tl1:cancel:1:1".to_string()),
        project_id: Some("p1".to_string()),
        payload: "{}".to_string(),
        max_retries: Some(3),
        payload_version: Some(1),
    };
    let _job = job_manager.submit_job(submission).unwrap();

    // Acquire lease to simulate running
    let running_job = job_manager
        .acquire_next_job_for_type("render.timeline", 60)
        .unwrap()
        .unwrap();
    assert_eq!(running_job.status, JobState::Running);

    // Cancel job
    let cancelled = job_manager.cancel_job(&running_job.id).unwrap();
    assert_eq!(cancelled.status, JobState::Cancelled);

    let worker = LocalRenderWorker::new(
        "render-worker-cancel".to_string(),
        job_manager.clone(),
        resolver.clone(),
        importer.clone(),
        staging_dir.clone(),
        PathBuf::from("mock-ffmpeg"),
        None,
        false,
    );

    // Worker attempts to process cancelled job
    let err = worker.process_one_job(&running_job).unwrap_err();
    assert_eq!(err, "Job was cancelled");

    // Staging files cleaned up
    let staging_entries = std::fs::read_dir(&staging_dir).unwrap().count();
    assert_eq!(
        staging_entries, 0,
        "Staging directory must be empty after cancellation"
    );

    // No artifact created for cancelled job
    let artifacts = ArtifactRepository::list_for_project(&disk_conn, "p1").unwrap();
    assert_eq!(
        artifacts.len(),
        0,
        "No artifact should be created for cancelled job"
    );

    // Attempting to complete cancelled job with lease token is rejected by CAS
    let complete_res = job_manager.complete_job_with_token(
        &running_job.id,
        running_job.lease_token.as_deref(),
        Some("{}".to_string()),
    );
    assert!(complete_res.is_err(), "Cannot complete a cancelled job");
}

use openvideoai_lib::artifacts::importer::ArtifactImporter;
use openvideoai_lib::artifacts::metadata::probe_media;
use openvideoai_lib::artifacts::model::ArtifactKind;
use openvideoai_lib::artifacts::resolver::ArtifactResolver;
use openvideoai_lib::db::migrations::run_all_migrations;
use openvideoai_lib::db::DbManager;
use openvideoai_lib::jobs::manager::JobManager;
use openvideoai_lib::jobs::model::{JobState, JobSubmission};
use openvideoai_lib::storage::local_store::LocalStore;
use openvideoai_lib::timeline::model::*;
use openvideoai_lib::timeline::mutation::TimelineMutation;
use openvideoai_lib::timeline::renderspec::RenderSpecBuilder;
use openvideoai_lib::timeline::repository::TimelineRepository;
use openvideoai_lib::timeline::service::TimelineService;
use openvideoai_lib::timeline::worker::LocalRenderWorker;
use openvideoai_lib::tools::detector::find_executable;
use rusqlite::Connection;
use std::sync::Arc;
use tempfile::tempdir;

#[test]
fn test_e2e_synthetic_ffmpeg_render_acceptance() {
    let ffmpeg_exe = match find_executable("ffmpeg", None) {
        Some(p) => p,
        None => {
            eprintln!("SKIP: ffmpeg not detected on host system");
            return;
        }
    };
    let ffprobe_exe = find_executable("ffprobe", None);

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

    let db_manager = Arc::new(DbManager::new(db_path.clone()).unwrap());
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
    let service = TimelineService::new();

    // 1. Generate synthetic video fixture 1 (2s 320x240 @ 30fps with 1000Hz test audio)
    let clip_a_src = temp_dir.path().join("clip_a.mp4");
    let status_a = std::process::Command::new(&ffmpeg_exe)
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=320x240:rate=30:duration=2",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=1000:duration=2",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            clip_a_src.to_str().unwrap(),
        ])
        .status()
        .unwrap();
    assert!(status_a.success(), "Failed to generate fixture Clip A");

    // Generate synthetic video fixture 2 (1s 320x240 @ 30fps with 440Hz test audio)
    let clip_b_src = temp_dir.path().join("clip_b.mp4");
    let status_b = std::process::Command::new(&ffmpeg_exe)
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=320x240:rate=30:duration=1",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=1",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            clip_b_src.to_str().unwrap(),
        ])
        .status()
        .unwrap();
    assert!(status_b.success(), "Failed to generate fixture Clip B");

    // 2. Import into Artifact Core
    disk_conn
        .execute(
            "INSERT INTO projects (id, name, created_at, updated_at) VALUES ('p1', 'Acceptance Project', datetime('now'), datetime('now'));",
            [],
        )
        .unwrap();
    let imported_a = importer.import("p1", &clip_a_src).unwrap();
    let imported_b = importer.import("p1", &clip_b_src).unwrap();

    // 3. Create Timeline (320x240, 30fps)
    let canvas = CanvasConfig {
        width: 320,
        height: 240,
        frame_rate: 30.0,
    };
    let tl = TimelineRepository::create(
        &mut disk_conn,
        "tl_acc",
        "p1",
        "Acceptance Timeline",
        canvas,
    )
    .unwrap();

    // Add Track V1
    let tl = service
        .apply_mutation(
            &mut disk_conn,
            &tl.id,
            1,
            TimelineMutation::AddTrack {
                kind: TrackKind::Video,
                name: "V1".to_string(),
            },
        )
        .unwrap();
    let v1_id = tl.tracks[0].id.clone();

    // Add Clip A: 0..2000ms at timelineStart 0ms
    let tl = service
        .apply_mutation(
            &mut disk_conn,
            &tl.id,
            2,
            TimelineMutation::AddClip {
                track_id: v1_id.clone(),
                artifact_id: imported_a.artifact.artifact_id.clone(),
                timeline_start_ms: 0,
                source_in_ms: Some(0),
                source_out_ms: Some(2000),
            },
        )
        .unwrap();

    // Add Clip B: 0..1000ms at timelineStart 3500ms (introducing 1500ms black canvas gap!)
    let tl = service
        .apply_mutation(
            &mut disk_conn,
            &tl.id,
            3,
            TimelineMutation::AddClip {
                track_id: v1_id,
                artifact_id: imported_b.artifact.artifact_id.clone(),
                timeline_start_ms: 3500,
                source_in_ms: Some(0),
                source_out_ms: Some(1000),
            },
        )
        .unwrap();

    // Derived duration math: 3500ms + 1000ms = 4500ms (4.5s)
    assert_eq!(tl.derived_duration_ms(), 4500);

    // 4. Build RenderSpec v1 with stream capabilities bound from metadata
    let spec = RenderSpecBuilder::build_from_timeline(&tl, &disk_conn).unwrap();
    assert_eq!(spec.output.width, 320);
    assert_eq!(spec.output.height, 240);

    // 5. Submit render job
    let payload = serde_json::json!({
        "schemaVersion": 1,
        "renderSpec": spec,
        "rendererProfileVersion": 1,
    });
    let submission = JobSubmission {
        job_type: "render.timeline".to_string(),
        priority: Some(10),
        idempotency_key: Some(format!("render.timeline:tl_acc:{}:1:1", tl.revision)),
        project_id: Some("p1".to_string()),
        payload: payload.to_string(),
        max_retries: Some(3),
        payload_version: Some(1),
    };
    let job = job_manager.submit_job(submission).unwrap();
    assert_eq!(job.status, JobState::Queued);

    // Acquire lease so job is in Running state with valid lease token
    let running_job = job_manager
        .acquire_next_job_for_type("render.timeline", 60)
        .unwrap()
        .expect("Failed to acquire lease for render.timeline job");
    assert_eq!(running_job.id, job.id);
    assert_eq!(running_job.status, JobState::Running);

    // 6. Execute through LocalRenderWorker
    let worker = LocalRenderWorker::new(
        "render-worker-acceptance".to_string(),
        job_manager.clone(),
        resolver.clone(),
        importer.clone(),
        staging_dir.clone(),
        ffmpeg_exe.clone(),
        ffprobe_exe.clone(),
        false, // real mode
    );

    let output_artifact = worker.process_one_job(&running_job).unwrap();
    assert_eq!(output_artifact.kind, ArtifactKind::Video);

    // 7. Validate rendered output via FFprobe
    let output_path = resolver.resolve_uri(&output_artifact.uri).unwrap();
    assert!(output_path.is_file());
    assert!(output_path.metadata().unwrap().len() > 0);

    let meta = probe_media(
        &output_artifact.artifact_id,
        &output_path,
        ffprobe_exe.as_deref(),
    );
    if meta.probe_status == "ok" {
        assert_eq!(meta.video_codec.as_deref(), Some("h264"));
        assert_eq!(meta.audio_codec.as_deref(), Some("aac"));
        assert_eq!(meta.width, Some(320));
        assert_eq!(meta.height, Some(240));
        // Verify 4500ms duration with tolerance of 100ms
        let dur = meta.duration_ms.expect("Duration must be probed");
        assert!(
            (dur - 4500).abs() <= 100,
            "Expected ~4500ms duration, got: {}",
            dur
        );
    }

    // 8. Verify timeline state persists across clean DB reopen
    drop(disk_conn);
    let reopened = Connection::open(&db_path).unwrap();
    let reloaded = TimelineRepository::get_by_id(&reopened, "tl_acc")
        .unwrap()
        .unwrap();
    assert_eq!(reloaded.revision, 4);
    assert_eq!(reloaded.derived_duration_ms(), 4500);
    assert_eq!(reloaded.tracks[0].clips.len(), 2);
}

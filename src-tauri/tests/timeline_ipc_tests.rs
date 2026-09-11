use openvideoai_lib::artifacts::importer::ArtifactImporter;
use openvideoai_lib::artifacts::metadata::{MediaMetadata, MetadataRepository};
use openvideoai_lib::db::migrations::run_all_migrations;
use openvideoai_lib::db::DbManager;
use openvideoai_lib::jobs::manager::JobManager;
use openvideoai_lib::projects::model::Project;
use openvideoai_lib::projects::repository::ProjectRepository;
use openvideoai_lib::storage::local_store::LocalStore;
use openvideoai_lib::timeline::commands::*;
use openvideoai_lib::timeline::model::*;
use openvideoai_lib::timeline::mutation::TimelineMutation;
use rusqlite::Connection;
use std::sync::Arc;
use tempfile::tempdir;

#[test]
fn test_ipc_timeline_crud_and_mutations() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut conn = Connection::open(&db_path).unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    run_all_migrations(&mut conn).unwrap();

    let project = Project {
        id: "p1".to_string(),
        name: "P1".to_string(),
        description: None,
        created_at: "2026-09-10T12:00:00Z".to_string(),
        updated_at: "2026-09-10T12:00:00Z".to_string(),
    };
    ProjectRepository::create(&conn, &project).unwrap();
    drop(conn);

    let db_manager = Arc::new(DbManager::new(db_path).unwrap());

    // 1. Create timeline via command
    let doc = cmd_create_timeline(&db_manager, "p1", "Main", 1920, 1080, 30.0).unwrap();
    assert_eq!(doc.revision, 1);
    assert_eq!(doc.name, "Main");

    // 2. Fetch timeline
    let fetched = cmd_get_timeline(&db_manager, &doc.id).unwrap().unwrap();
    assert_eq!(fetched.id, doc.id);

    // 3. Apply mutation
    let updated = cmd_apply_timeline_mutation(
        &db_manager,
        &doc.id,
        1,
        TimelineMutation::AddTrack {
            kind: TrackKind::Video,
            name: "V1".to_string(),
        },
    )
    .unwrap();
    assert_eq!(updated.revision, 2);
    assert_eq!(updated.tracks.len(), 1);

    // 4. Undo mutation with matching expected revision
    let undone = cmd_undo_timeline(&db_manager, &doc.id, 2).unwrap();
    assert_eq!(undone.revision, 3);
    assert_eq!(undone.tracks.len(), 0);

    // 5. Redo mutation with matching expected revision
    let redone = cmd_redo_timeline(&db_manager, &doc.id, 3).unwrap();
    assert_eq!(redone.revision, 4);
    assert_eq!(redone.tracks.len(), 1);

    // 6. List project timelines
    let timelines = cmd_list_project_timelines(&db_manager, "p1").unwrap();
    assert_eq!(timelines.len(), 1);
    assert_eq!(timelines[0].id, doc.id);

    // 7. Delete timeline
    let deleted = cmd_delete_timeline(&db_manager, &doc.id).unwrap();
    assert!(deleted);
    let after_delete = cmd_get_timeline(&db_manager, &doc.id).unwrap();
    assert!(after_delete.is_none());
}

#[test]
fn test_ipc_render_spec_and_submit() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let store_dir = temp_dir.path().join("store");
    let staging_dir = temp_dir.path().join("staging");
    std::fs::create_dir_all(&store_dir).unwrap();
    std::fs::create_dir_all(&staging_dir).unwrap();

    let mut conn = Connection::open(&db_path).unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    run_all_migrations(&mut conn).unwrap();

    let project = Project {
        id: "p1".to_string(),
        name: "P1".to_string(),
        description: None,
        created_at: "2026-09-10T12:00:00Z".to_string(),
        updated_at: "2026-09-10T12:00:00Z".to_string(),
    };
    ProjectRepository::create(&conn, &project).unwrap();

    let db_manager = Arc::new(DbManager::new(db_path).unwrap());
    let local_store = Arc::new(LocalStore::new(store_dir));
    let importer = Arc::new(ArtifactImporter::new(
        db_manager.clone(),
        local_store,
        staging_dir,
    ));
    let job_manager = Arc::new(JobManager::new(db_manager.clone()));

    // Create a video artifact
    let source_path = temp_dir.path().join("clip.mp4");
    std::fs::write(&source_path, b"mock video").unwrap();
    let imported = importer.import("p1", &source_path).unwrap();

    let meta = MediaMetadata {
        artifact_id: imported.artifact.artifact_id.clone(),
        probe_status: "ok".to_string(),
        duration_ms: Some(6000),
        width: Some(1920),
        height: Some(1080),
        video_codec: Some("h264".to_string()),
        audio_codec: Some("aac".to_string()),
        frame_rate: Some(30.0),
        sample_rate: Some(48000),
        channels: Some(2),
        probe_error: None,
    };
    MetadataRepository::insert(&conn, &meta).unwrap();
    drop(conn);

    // Create timeline with 1 video track and 1 clip
    let doc = cmd_create_timeline(&db_manager, "p1", "Render TL", 1920, 1080, 30.0).unwrap();
    let doc2 = cmd_apply_timeline_mutation(
        &db_manager,
        &doc.id,
        1,
        TimelineMutation::AddTrack {
            kind: TrackKind::Video,
            name: "V1".to_string(),
        },
    )
    .unwrap();
    let track_id = &doc2.tracks[0].id;
    let doc3 = cmd_apply_timeline_mutation(
        &db_manager,
        &doc.id,
        2,
        TimelineMutation::AddClip {
            track_id: track_id.clone(),
            artifact_id: imported.artifact.artifact_id.clone(),
            timeline_start_ms: 0,
            source_in_ms: Some(0),
            source_out_ms: Some(4000),
        },
    )
    .unwrap();
    assert_eq!(doc3.revision, 3);

    // Build render spec
    let spec = cmd_build_render_spec(&db_manager, &doc.id).unwrap();
    assert_eq!(spec.tracks.len(), 1);
    assert_eq!(spec.tracks[0].clips.len(), 1);
    assert!(spec.tracks[0].clips[0].has_video);

    // Submit render with matching revision 3
    let job_id = cmd_submit_timeline_render(&db_manager, &job_manager, &doc.id, 3).unwrap();
    let job = job_manager.get_job(&job_id).unwrap().unwrap();
    assert_eq!(job.job_type, "render.timeline");
    assert_eq!(job.project_id, Some("p1".to_string()));

    // Submitting with wrong revision should fail
    let conflict = cmd_submit_timeline_render(&db_manager, &job_manager, &doc.id, 99);
    assert!(conflict.is_err());
}

use openvideoai_lib::artifacts::metadata::{MediaMetadata, MetadataRepository};
use openvideoai_lib::artifacts::model::{Artifact, ArtifactKind};
use openvideoai_lib::artifacts::repository::ArtifactRepository;
use openvideoai_lib::db::migrations::run_all_migrations;
use openvideoai_lib::projects::model::Project;
use openvideoai_lib::projects::repository::ProjectRepository;
use openvideoai_lib::timeline::model::*;
use openvideoai_lib::timeline::repository::TimelineRepository;
use rusqlite::Connection;

#[test]
fn test_create_initializes_revision_1_and_history_snapshot_0() {
    let mut conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    run_all_migrations(&mut conn).unwrap();

    let project = Project {
        id: "p1".to_string(),
        name: "Test Project".to_string(),
        description: None,
        created_at: "2026-09-10T12:00:00Z".to_string(),
        updated_at: "2026-09-10T12:00:00Z".to_string(),
    };
    ProjectRepository::create(&conn, &project).unwrap();

    let canvas = CanvasConfig {
        width: 1920,
        height: 1080,
        frame_rate: 30.0,
    };
    let created =
        TimelineRepository::create(&mut conn, "tl1", "p1", "Main Timeline", canvas).unwrap();

    // Invariant: initial revision = 1
    assert_eq!(created.revision, 1);
    assert_eq!(created.name, "Main Timeline");
    assert_eq!(created.tracks.len(), 0);

    // Invariant: history_cursor = 0 on timeline row
    let cursor: i64 = conn
        .query_row(
            "SELECT history_cursor FROM timelines WHERE id = 'tl1';",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(cursor, 0);

    // Invariant: history snapshot 0 exists in timeline_history table
    let (history_index, snapshot_json, op_type): (i64, String, String) = conn
        .query_row(
            "SELECT history_index, snapshot_json, operation_type FROM timeline_history WHERE timeline_id = 'tl1';",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();

    assert_eq!(history_index, 0);
    assert_eq!(op_type, "create");
    let parsed: serde_json::Value = serde_json::from_str(&snapshot_json).unwrap();
    assert_eq!(parsed["schemaVersion"], 1);
    assert_eq!(parsed["timeline"]["id"], "tl1");
    assert_eq!(parsed["timeline"]["revision"], 1);
}

#[test]
fn test_get_timeline_assembles_tracks_and_clips_in_order() {
    let mut conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    run_all_migrations(&mut conn).unwrap();

    let project = Project {
        id: "p1".to_string(),
        name: "Test Project".to_string(),
        description: None,
        created_at: "2026-09-10T12:00:00Z".to_string(),
        updated_at: "2026-09-10T12:00:00Z".to_string(),
    };
    ProjectRepository::create(&conn, &project).unwrap();

    // Insert artifact using actual Phase 1 8-column schema & repository
    let artifact = Artifact {
        id: "art-video-1".to_string(),
        sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string(),
        kind: ArtifactKind::Video,
        mime_type: Some("video/mp4".to_string()),
        size_bytes: 1048576,
        storage_key: "videos/art-video-1.mp4".to_string(),
        original_filename: "clip1.mp4".to_string(),
        created_at: "2026-09-10T12:00:00Z".to_string(),
    };
    ArtifactRepository::insert(&conn, &artifact).unwrap();
    ArtifactRepository::link_to_project(&conn, "p1", &artifact.id, "2026-09-10T12:00:00Z").unwrap();

    let meta = MediaMetadata {
        artifact_id: artifact.id.clone(),
        probe_status: "ok".to_string(),
        duration_ms: Some(10000),
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

    let canvas = CanvasConfig {
        width: 1920,
        height: 1080,
        frame_rate: 30.0,
    };
    TimelineRepository::create(&mut conn, "tl1", "p1", "Main Timeline", canvas).unwrap();

    // Insert tracks out of order into SQLite to verify order_index ASC ordering
    conn.execute(
        "INSERT INTO timeline_tracks (id, timeline_id, kind, order_index, name, muted) VALUES ('tr-audio', 'tl1', 'audio', 1, 'A1', 0);",
        [],
    ).unwrap();
    conn.execute(
        "INSERT INTO timeline_tracks (id, timeline_id, kind, order_index, name, muted) VALUES ('tr-video', 'tl1', 'video', 0, 'V1', 0);",
        [],
    ).unwrap();

    // Insert clips out of order to verify timeline_start_ms ASC ordering
    conn.execute(
        "INSERT INTO timeline_clips (id, track_id, artifact_id, source_in_ms, source_out_ms, timeline_start_ms, volume, muted, position_x, position_y, scale, rotation_deg, opacity)
         VALUES ('c2', 'tr-video', 'art-video-1', 0, 3000, 5000, 1.0, 0, 0.0, 0.0, 1.0, 0.0, 1.0);",
        [],
    ).unwrap();
    conn.execute(
        "INSERT INTO timeline_clips (id, track_id, artifact_id, source_in_ms, source_out_ms, timeline_start_ms, volume, muted, position_x, position_y, scale, rotation_deg, opacity)
         VALUES ('c1', 'tr-video', 'art-video-1', 0, 4000, 0, 1.0, 0, 0.0, 0.0, 1.0, 0.0, 1.0);",
        [],
    ).unwrap();

    let fetched = TimelineRepository::get_by_id(&conn, "tl1")
        .unwrap()
        .expect("Timeline must exist");
    assert_eq!(fetched.tracks.len(), 2);
    assert_eq!(fetched.tracks[0].name, "V1");
    assert_eq!(fetched.tracks[0].order, 0);
    assert_eq!(fetched.tracks[1].name, "A1");
    assert_eq!(fetched.tracks[1].order, 1);

    assert_eq!(fetched.tracks[0].clips.len(), 2);
    assert_eq!(fetched.tracks[0].clips[0].id, "c1");
    assert_eq!(fetched.tracks[0].clips[0].timeline_start_ms, 0);
    assert_eq!(fetched.tracks[0].clips[1].id, "c2");
    assert_eq!(fetched.tracks[0].clips[1].timeline_start_ms, 5000);
}

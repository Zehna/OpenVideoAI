use openvideoai_lib::artifacts::metadata::{MediaMetadata, MetadataRepository};
use openvideoai_lib::artifacts::model::{Artifact, ArtifactKind};
use openvideoai_lib::artifacts::repository::ArtifactRepository;
use openvideoai_lib::db::migrations::run_all_migrations;
use openvideoai_lib::projects::model::Project;
use openvideoai_lib::projects::repository::ProjectRepository;
use openvideoai_lib::timeline::error::TimelineError;
use openvideoai_lib::timeline::model::*;
use openvideoai_lib::timeline::mutation::TimelineMutation;
use openvideoai_lib::timeline::repository::TimelineRepository;
use openvideoai_lib::timeline::service::TimelineService;
use rusqlite::Connection;

fn setup_test_db() -> (Connection, String, String) {
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
        duration_ms: Some(20000), // 20s source
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

    (conn, "tl1".to_string(), artifact.id)
}

#[test]
fn test_same_track_overlap_rejected() {
    let (mut conn, tl_id, art_id) = setup_test_db();
    let service = TimelineService::new();

    // Add track V1 (rev 1 -> 2)
    let doc = service
        .apply_mutation(
            &mut conn,
            &tl_id,
            1,
            TimelineMutation::AddTrack {
                kind: TrackKind::Video,
                name: "V1".to_string(),
            },
        )
        .unwrap();
    let track_id = doc.tracks[0].id.clone();

    // Add clip 1 at timeline 0..5000ms (rev 2 -> 3)
    let doc = service
        .apply_mutation(
            &mut conn,
            &tl_id,
            2,
            TimelineMutation::AddClip {
                track_id: track_id.clone(),
                artifact_id: art_id.clone(),
                timeline_start_ms: 0,
                source_in_ms: Some(0),
                source_out_ms: Some(5000),
            },
        )
        .unwrap();
    assert_eq!(doc.revision, 3);

    // Add overlapping clip 2 on SAME track at timeline 3000..8000ms (overlaps 3000..5000)
    let err = service
        .apply_mutation(
            &mut conn,
            &tl_id,
            3,
            TimelineMutation::AddClip {
                track_id,
                artifact_id: art_id,
                timeline_start_ms: 3000,
                source_in_ms: Some(0),
                source_out_ms: Some(5000),
            },
        )
        .unwrap_err();

    assert_eq!(err, TimelineError::ClipOverlap);
}

#[test]
fn test_cross_track_overlap_accepted() {
    let (mut conn, tl_id, art_id) = setup_test_db();
    let service = TimelineService::new();

    // Add track V1 and track V2
    let doc = service
        .apply_mutation(
            &mut conn,
            &tl_id,
            1,
            TimelineMutation::AddTrack {
                kind: TrackKind::Video,
                name: "V1".to_string(),
            },
        )
        .unwrap();
    let v1_id = doc.tracks[0].id.clone();

    let doc = service
        .apply_mutation(
            &mut conn,
            &tl_id,
            2,
            TimelineMutation::AddTrack {
                kind: TrackKind::Video,
                name: "V2".to_string(),
            },
        )
        .unwrap();
    let v2_id = doc.tracks[1].id.clone();

    // Add clip on V1 at 0..5000ms
    service
        .apply_mutation(
            &mut conn,
            &tl_id,
            3,
            TimelineMutation::AddClip {
                track_id: v1_id,
                artifact_id: art_id.clone(),
                timeline_start_ms: 0,
                source_in_ms: Some(0),
                source_out_ms: Some(5000),
            },
        )
        .unwrap();

    // Add clip on V2 at 3000..8000ms (cross-track overlap is valid)
    let doc = service
        .apply_mutation(
            &mut conn,
            &tl_id,
            4,
            TimelineMutation::AddClip {
                track_id: v2_id,
                artifact_id: art_id,
                timeline_start_ms: 3000,
                source_in_ms: Some(0),
                source_out_ms: Some(5000),
            },
        )
        .unwrap();

    assert_eq!(doc.revision, 5);
    assert_eq!(doc.derived_duration_ms(), 8000);
}

#[test]
fn test_split_clip_math() {
    let (mut conn, tl_id, art_id) = setup_test_db();
    let service = TimelineService::new();

    // Add track V1
    let doc = service
        .apply_mutation(
            &mut conn,
            &tl_id,
            1,
            TimelineMutation::AddTrack {
                kind: TrackKind::Video,
                name: "V1".to_string(),
            },
        )
        .unwrap();
    let v1_id = doc.tracks[0].id.clone();

    // Add clip: source 2000..14000 (12s dur), timelineStart 5000..17000
    let doc = service
        .apply_mutation(
            &mut conn,
            &tl_id,
            2,
            TimelineMutation::AddClip {
                track_id: v1_id,
                artifact_id: art_id,
                timeline_start_ms: 5000,
                source_in_ms: Some(2000),
                source_out_ms: Some(14000),
            },
        )
        .unwrap();
    let original_clip_id = doc.tracks[0].clips[0].id.clone();

    // Split at timeline 10000ms (relative offset = 5000ms, source split = 7000ms)
    let doc = service
        .apply_mutation(
            &mut conn,
            &tl_id,
            3,
            TimelineMutation::SplitClip {
                clip_id: original_clip_id,
                split_timeline_ms: 10000,
            },
        )
        .unwrap();

    assert_eq!(doc.tracks[0].clips.len(), 2);
    let clip_a = &doc.tracks[0].clips[0];
    let clip_b = &doc.tracks[0].clips[1];

    assert_eq!(clip_a.timeline_start_ms, 5000);
    assert_eq!(clip_a.source_in_ms, 2000);
    assert_eq!(clip_a.source_out_ms, 7000);
    assert_eq!(clip_a.duration_ms(), 5000);

    assert_eq!(clip_b.timeline_start_ms, 10000);
    assert_eq!(clip_b.source_in_ms, 7000);
    assert_eq!(clip_b.source_out_ms, 14000);
    assert_eq!(clip_b.duration_ms(), 7000);
}

#[test]
fn test_trim_clip_validates_source_bounds() {
    let (mut conn, tl_id, art_id) = setup_test_db();
    let service = TimelineService::new();

    let doc = service
        .apply_mutation(
            &mut conn,
            &tl_id,
            1,
            TimelineMutation::AddTrack {
                kind: TrackKind::Video,
                name: "V1".to_string(),
            },
        )
        .unwrap();
    let v1_id = doc.tracks[0].id.clone();

    let doc = service
        .apply_mutation(
            &mut conn,
            &tl_id,
            2,
            TimelineMutation::AddClip {
                track_id: v1_id,
                artifact_id: art_id,
                timeline_start_ms: 0,
                source_in_ms: Some(0),
                source_out_ms: Some(5000),
            },
        )
        .unwrap();
    let clip_id = doc.tracks[0].clips[0].id.clone();

    // Trim beyond source media duration (source media duration is 20000ms)
    let err = service
        .apply_mutation(
            &mut conn,
            &tl_id,
            3,
            TimelineMutation::TrimClip {
                clip_id,
                source_in_ms: 0,
                source_out_ms: 25000,
            },
        )
        .unwrap_err();

    assert_eq!(err, TimelineError::InvalidClipRange);
}

#[test]
fn test_mutation_enforces_optimistic_revision_concurrency() {
    let (mut conn, tl_id, _) = setup_test_db();
    let service = TimelineService::new();

    // Current revision is 1. Passing expected_revision = 99 must fail.
    let err = service
        .apply_mutation(
            &mut conn,
            &tl_id,
            99,
            TimelineMutation::AddTrack {
                kind: TrackKind::Video,
                name: "V1".to_string(),
            },
        )
        .unwrap_err();

    assert_eq!(
        err,
        TimelineError::RevisionConflict {
            expected: 99,
            actual: 1
        }
    );
}

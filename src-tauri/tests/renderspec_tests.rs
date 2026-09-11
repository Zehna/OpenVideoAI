use openvideoai_lib::artifacts::metadata::MediaMetadata;
use openvideoai_lib::timeline::error::TimelineError;
use openvideoai_lib::timeline::model::*;
use openvideoai_lib::timeline::renderspec::*;
use std::collections::HashMap;

fn create_metadata_map() -> HashMap<String, MediaMetadata> {
    let mut map = HashMap::new();
    map.insert(
        "art-video-aac".to_string(),
        MediaMetadata {
            artifact_id: "art-video-aac".to_string(),
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
        },
    );
    map.insert(
        "art-video-noaudio".to_string(),
        MediaMetadata {
            artifact_id: "art-video-noaudio".to_string(),
            probe_status: "ok".to_string(),
            duration_ms: Some(8000),
            width: Some(1920),
            height: Some(1080),
            video_codec: Some("h264".to_string()),
            audio_codec: None, // No audio stream!
            frame_rate: Some(30.0),
            sample_rate: None,
            channels: None,
            probe_error: None,
        },
    );
    map.insert(
        "art-audio-only".to_string(),
        MediaMetadata {
            artifact_id: "art-audio-only".to_string(),
            probe_status: "ok".to_string(),
            duration_ms: Some(15000),
            width: None,
            height: None,
            video_codec: None, // No video stream!
            audio_codec: Some("aac".to_string()),
            frame_rate: None,
            sample_rate: Some(48000),
            channels: Some(2),
            probe_error: None,
        },
    );
    map
}

#[test]
fn test_video_artifact_with_aac_freezes_has_audio_true() {
    let meta_map = create_metadata_map();
    let doc = TimelineDocument {
        id: "tl1".to_string(),
        project_id: "p1".to_string(),
        name: "Test".to_string(),
        revision: 1,
        canvas: CanvasConfig {
            width: 1920,
            height: 1080,
            frame_rate: 30.0,
        },
        tracks: vec![TimelineTrack {
            id: "tr1".to_string(),
            timeline_id: "tl1".to_string(),
            kind: TrackKind::Video,
            order: 0,
            name: "V1".to_string(),
            muted: false,
            clips: vec![TimelineClip {
                id: "c1".to_string(),
                track_id: "tr1".to_string(),
                artifact_id: "art-video-aac".to_string(),
                source_in_ms: 0,
                source_out_ms: 5000,
                timeline_start_ms: 0,
                volume: 1.0,
                muted: false,
                transform: ClipTransform::default(),
            }],
        }],
        created_at: "2026-09-10T12:00:00Z".to_string(),
        updated_at: "2026-09-10T12:00:00Z".to_string(),
    };

    let spec = RenderSpecBuilder::build_from_timeline(&doc, &meta_map).unwrap();
    assert!(spec.tracks[0].clips[0].has_video);
    assert!(spec.tracks[0].clips[0].has_audio);
}

#[test]
fn test_video_artifact_without_audio_freezes_has_audio_false() {
    let meta_map = create_metadata_map();
    let doc = TimelineDocument {
        id: "tl1".to_string(),
        project_id: "p1".to_string(),
        name: "Test".to_string(),
        revision: 1,
        canvas: CanvasConfig {
            width: 1920,
            height: 1080,
            frame_rate: 30.0,
        },
        tracks: vec![TimelineTrack {
            id: "tr1".to_string(),
            timeline_id: "tl1".to_string(),
            kind: TrackKind::Video,
            order: 0,
            name: "V1".to_string(),
            muted: false,
            clips: vec![TimelineClip {
                id: "c1".to_string(),
                track_id: "tr1".to_string(),
                artifact_id: "art-video-noaudio".to_string(),
                source_in_ms: 0,
                source_out_ms: 5000,
                timeline_start_ms: 0,
                volume: 1.0,
                muted: false,
                transform: ClipTransform::default(),
            }],
        }],
        created_at: "2026-09-10T12:00:00Z".to_string(),
        updated_at: "2026-09-10T12:00:00Z".to_string(),
    };

    let spec = RenderSpecBuilder::build_from_timeline(&doc, &meta_map).unwrap();
    assert!(spec.tracks[0].clips[0].has_video);
    assert!(!spec.tracks[0].clips[0].has_audio);
}

#[test]
fn test_audio_only_artifact_freezes_stream_capabilities_correctly() {
    let meta_map = create_metadata_map();
    let doc = TimelineDocument {
        id: "tl1".to_string(),
        project_id: "p1".to_string(),
        name: "Test".to_string(),
        revision: 1,
        canvas: CanvasConfig {
            width: 1920,
            height: 1080,
            frame_rate: 30.0,
        },
        tracks: vec![TimelineTrack {
            id: "tr1".to_string(),
            timeline_id: "tl1".to_string(),
            kind: TrackKind::Audio,
            order: 0,
            name: "A1".to_string(),
            muted: false,
            clips: vec![TimelineClip {
                id: "c1".to_string(),
                track_id: "tr1".to_string(),
                artifact_id: "art-audio-only".to_string(),
                source_in_ms: 0,
                source_out_ms: 5000,
                timeline_start_ms: 0,
                volume: 1.0,
                muted: false,
                transform: ClipTransform::default(),
            }],
        }],
        created_at: "2026-09-10T12:00:00Z".to_string(),
        updated_at: "2026-09-10T12:00:00Z".to_string(),
    };

    let spec = RenderSpecBuilder::build_from_timeline(&doc, &meta_map).unwrap();
    assert!(!spec.tracks[0].clips[0].has_video);
    assert!(spec.tracks[0].clips[0].has_audio);
}

#[test]
fn test_missing_required_metadata_returns_typed_error() {
    let meta_map = HashMap::<String, MediaMetadata>::new(); // Empty!
    let doc = TimelineDocument {
        id: "tl1".to_string(),
        project_id: "p1".to_string(),
        name: "Test".to_string(),
        revision: 1,
        canvas: CanvasConfig {
            width: 1920,
            height: 1080,
            frame_rate: 30.0,
        },
        tracks: vec![TimelineTrack {
            id: "tr1".to_string(),
            timeline_id: "tl1".to_string(),
            kind: TrackKind::Video,
            order: 0,
            name: "V1".to_string(),
            muted: false,
            clips: vec![TimelineClip {
                id: "c1".to_string(),
                track_id: "tr1".to_string(),
                artifact_id: "missing-artifact".to_string(),
                source_in_ms: 0,
                source_out_ms: 5000,
                timeline_start_ms: 0,
                volume: 1.0,
                muted: false,
                transform: ClipTransform::default(),
            }],
        }],
        created_at: "2026-09-10T12:00:00Z".to_string(),
        updated_at: "2026-09-10T12:00:00Z".to_string(),
    };

    let err = RenderSpecBuilder::build_from_timeline(&doc, &meta_map).unwrap_err();
    assert_eq!(
        err,
        TimelineError::ArtifactNotFound("missing-artifact".to_string())
    );
}

#[test]
fn test_renderspec_v1_builds_immutable_snapshot_with_no_physical_paths() {
    let meta_map = create_metadata_map();
    let doc = TimelineDocument {
        id: "tl1".to_string(),
        project_id: "p1".to_string(),
        name: "Test Timeline".to_string(),
        revision: 4,
        canvas: CanvasConfig {
            width: 1920,
            height: 1080,
            frame_rate: 30.0,
        },
        tracks: vec![TimelineTrack {
            id: "tr1".to_string(),
            timeline_id: "tl1".to_string(),
            kind: TrackKind::Video,
            order: 0,
            name: "V1".to_string(),
            muted: false,
            clips: vec![TimelineClip {
                id: "c1".to_string(),
                track_id: "tr1".to_string(),
                artifact_id: "art-video-aac".to_string(),
                source_in_ms: 1000,
                source_out_ms: 5000,
                timeline_start_ms: 0,
                volume: 1.0,
                muted: false,
                transform: ClipTransform::default(),
            }],
        }],
        created_at: "2026-09-10T12:00:00Z".to_string(),
        updated_at: "2026-09-10T12:00:00Z".to_string(),
    };

    let spec = RenderSpecBuilder::build_from_timeline(&doc, &meta_map).unwrap();

    assert_eq!(spec.version, 1);
    assert_eq!(spec.project_id, "p1");
    assert_eq!(spec.timeline_id, "tl1");
    assert_eq!(spec.timeline_revision, 4);

    assert_eq!(spec.output.width, 1920);
    assert_eq!(spec.output.height, 1080);
    assert_eq!(spec.output.frame_rate, 30.0);
    assert_eq!(spec.output.container, "mp4");
    assert_eq!(spec.output.video_codec, "h264");
    assert_eq!(spec.output.audio_codec, "aac");
    assert_eq!(spec.output.pixel_format, "yuv420p");

    assert_eq!(spec.tracks.len(), 1);
    assert_eq!(spec.tracks[0].clips.len(), 1);
    assert_eq!(spec.tracks[0].clips[0].artifact_id, "art-video-aac");
    assert!(spec.tracks[0].clips[0].has_video);
    assert!(spec.tracks[0].clips[0].has_audio);

    // Serialization verification: contains no drive letters or slashes
    let serialized = serde_json::to_string(&spec).unwrap();
    assert!(
        !serialized.contains(r"C:\"),
        "RenderSpec must never contain physical filesystem paths"
    );
    assert!(
        !serialized.contains("/home/"),
        "RenderSpec must never contain physical filesystem paths"
    );
}

#[test]
fn test_empty_timeline_renderspec_rejected() {
    let meta_map = create_metadata_map();
    let doc = TimelineDocument {
        id: "tl1".to_string(),
        project_id: "p1".to_string(),
        name: "Empty".to_string(),
        revision: 1,
        canvas: CanvasConfig {
            width: 1920,
            height: 1080,
            frame_rate: 30.0,
        },
        tracks: vec![],
        created_at: "2026-09-10T12:00:00Z".to_string(),
        updated_at: "2026-09-10T12:00:00Z".to_string(),
    };

    let err = RenderSpecBuilder::build_from_timeline(&doc, &meta_map).unwrap_err();
    assert_eq!(
        err,
        TimelineError::RenderSpecInvalid("Timeline has no clips to render".to_string())
    );
}

#[test]
fn test_non_default_transform_rejected_in_phase_3() {
    let meta_map = create_metadata_map();
    let custom_transform = ClipTransform {
        rotation_deg: 45.0,
        ..Default::default()
    };

    let doc = TimelineDocument {
        id: "tl1".to_string(),
        project_id: "p1".to_string(),
        name: "Transform Test".to_string(),
        revision: 2,
        canvas: CanvasConfig {
            width: 1920,
            height: 1080,
            frame_rate: 30.0,
        },
        tracks: vec![TimelineTrack {
            id: "tr1".to_string(),
            timeline_id: "tl1".to_string(),
            kind: TrackKind::Video,
            order: 0,
            name: "V1".to_string(),
            muted: false,
            clips: vec![TimelineClip {
                id: "c1".to_string(),
                track_id: "tr1".to_string(),
                artifact_id: "art-video-aac".to_string(),
                source_in_ms: 0,
                source_out_ms: 3000,
                timeline_start_ms: 0,
                volume: 1.0,
                muted: false,
                transform: custom_transform,
            }],
        }],
        created_at: "2026-09-10T12:00:00Z".to_string(),
        updated_at: "2026-09-10T12:00:00Z".to_string(),
    };

    let err = RenderSpecBuilder::build_from_timeline(&doc, &meta_map).unwrap_err();
    assert_eq!(
        err,
        TimelineError::InvalidOperation(
            "Non-default clip transforms not supported in Phase 3".to_string()
        )
    );
}

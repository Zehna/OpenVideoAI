use openvideoai_lib::timeline::error::TimelineError;
use openvideoai_lib::timeline::model::*;

#[test]
fn test_clip_duration_and_invariants() {
    let mut clip = TimelineClip {
        id: "c1".to_string(),
        track_id: "t1".to_string(),
        artifact_id: "a1".to_string(),
        source_in_ms: 1000,
        source_out_ms: 5000,
        timeline_start_ms: 2000,
        volume: 1.0,
        muted: false,
        transform: ClipTransform::default(),
    };
    assert_eq!(clip.duration_ms(), 4000);
    assert_eq!(clip.timeline_end_ms(), 6000);
    assert!(clip.validate(Some(6000)).is_ok());

    // Invalid source out <= source in
    clip.source_out_ms = 1000;
    assert_eq!(
        clip.validate(Some(6000)),
        Err(TimelineError::InvalidClipRange)
    );

    // Exceeds source duration
    clip.source_out_ms = 7000;
    assert_eq!(
        clip.validate(Some(6000)),
        Err(TimelineError::InvalidClipRange)
    );

    // Negative timeline start
    clip.source_out_ms = 5000;
    clip.timeline_start_ms = -10;
    assert_eq!(
        clip.validate(Some(6000)),
        Err(TimelineError::InvalidClipRange)
    );

    // Invalid volume > 2.0
    clip.timeline_start_ms = 0;
    clip.volume = 2.5;
    assert_eq!(
        clip.validate(Some(6000)),
        Err(TimelineError::InvalidOperation(
            "Invalid volume or transform value".to_string()
        ))
    );

    // Invalid opacity > 1.0
    clip.volume = 1.0;
    clip.transform.opacity = 1.5;
    assert_eq!(
        clip.validate(Some(6000)),
        Err(TimelineError::InvalidOperation(
            "Invalid volume or transform value".to_string()
        ))
    );

    // Missing source duration
    clip.transform.opacity = 1.0;
    assert_eq!(
        clip.validate(None),
        Err(TimelineError::MediaDurationUnknown)
    );
}

#[test]
fn test_derived_timeline_duration() {
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
        tracks: vec![
            TimelineTrack {
                id: "tr1".to_string(),
                timeline_id: "tl1".to_string(),
                kind: TrackKind::Video,
                order: 0,
                name: "V1".to_string(),
                muted: false,
                clips: vec![TimelineClip {
                    id: "c1".to_string(),
                    track_id: "tr1".to_string(),
                    artifact_id: "a1".to_string(),
                    source_in_ms: 0,
                    source_out_ms: 3000,
                    timeline_start_ms: 1000,
                    volume: 1.0,
                    muted: false,
                    transform: ClipTransform::default(),
                }],
            },
            TimelineTrack {
                id: "tr2".to_string(),
                timeline_id: "tl1".to_string(),
                kind: TrackKind::Audio,
                order: 1,
                name: "A1".to_string(),
                muted: false,
                clips: vec![TimelineClip {
                    id: "c2".to_string(),
                    track_id: "tr2".to_string(),
                    artifact_id: "a2".to_string(),
                    source_in_ms: 0,
                    source_out_ms: 8000,
                    timeline_start_ms: 0,
                    volume: 1.0,
                    muted: false,
                    transform: ClipTransform::default(),
                }],
            },
        ],
        created_at: "2026-09-10T12:00:00Z".to_string(),
        updated_at: "2026-09-10T12:00:00Z".to_string(),
    };

    assert_eq!(doc.derived_duration_ms(), 8000);
}

#[test]
fn test_clip_transform_defaults() {
    let default_transform = ClipTransform::default();
    assert!(default_transform.is_default());
    assert_eq!(default_transform.x, 0.0);
    assert_eq!(default_transform.y, 0.0);
    assert_eq!(default_transform.scale, 1.0);
    assert_eq!(default_transform.rotation_deg, 0.0);
    assert_eq!(default_transform.opacity, 1.0);

    let mut custom = default_transform.clone();
    custom.scale = 1.5;
    assert!(!custom.is_default());
}

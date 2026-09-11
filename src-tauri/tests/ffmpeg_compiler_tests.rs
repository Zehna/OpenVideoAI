use openvideoai_lib::timeline::compiler::FfmpegCompiler;
use openvideoai_lib::timeline::model::*;
use openvideoai_lib::timeline::renderspec::*;
use std::collections::HashMap;
use std::path::PathBuf;

fn create_base_spec() -> (RenderSpecV1, HashMap<String, PathBuf>, PathBuf) {
    let spec = RenderSpecV1 {
        version: 1,
        project_id: "p1".to_string(),
        timeline_id: "tl1".to_string(),
        timeline_revision: 3,
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
            id: "tr-v0".to_string(),
            kind: TrackKind::Video,
            order: 0,
            muted: false,
            clips: vec![
                RenderClip {
                    id: "c1".to_string(),
                    artifact_id: "art-video-1".to_string(),
                    source_in_ms: 0,
                    source_out_ms: 2000,
                    timeline_start_ms: 0,
                    volume: 1.0,
                    muted: false,
                    transform: ClipTransform::default(),
                    has_video: true,
                    has_audio: true,
                },
                RenderClip {
                    id: "c2".to_string(),
                    artifact_id: "art-video-2".to_string(),
                    source_in_ms: 0,
                    source_out_ms: 1000,
                    timeline_start_ms: 3500, // 1.5s gap after c1!
                    volume: 1.0,
                    muted: false,
                    transform: ClipTransform::default(),
                    has_video: true,
                    has_audio: false,
                },
            ],
        }],
    };

    let mut map = HashMap::new();
    map.insert(
        "art-video-1".to_string(),
        PathBuf::from(r"C:\Media Storage\video 1.mp4"),
    );
    map.insert(
        "art-video-2".to_string(),
        PathBuf::from(r"C:\Media Storage\video 2.mp4"),
    );
    let out = PathBuf::from(r"C:\Render Output\final render.mp4");

    (spec, map, out)
}

#[test]
fn test_a_video_with_embedded_audio() {
    let (mut spec, map, out) = create_base_spec();
    spec.tracks[0].clips.truncate(1); // Clip 1 has has_video: true, has_audio: true
    let plan = FfmpegCompiler::compile(&spec, &map, out).unwrap();

    // Video has PTS placement and scaling
    assert!(plan
        .filter_complex
        .contains("trim=start=0.000:end=2.000,setpts=PTS-STARTPTS+0.000/TB"));
    // Audio has trim, volume, adelay, and timeline duration trim
    assert!(plan
        .filter_complex
        .contains("atrim=start=0.000:end=2.000,asetpts=PTS-STARTPTS,volume=1.00,adelay=0|0:all=1"));
    assert!(plan.filter_complex.contains("atrim=end=2.000"));
}

#[test]
fn test_b_sequential_clips_with_gap() {
    let (spec, map, out) = create_base_spec();
    let plan = FfmpegCompiler::compile(&spec, &map, out).unwrap();

    // Invariant: Clip 2 placed at timelineStart 3500ms (3.5s) has video presentation timestamp placed at 3.5s!
    assert!(plan
        .filter_complex
        .contains("trim=start=0.000:end=1.000,setpts=PTS-STARTPTS+3.500/TB"));
    assert!(plan
        .filter_complex
        .contains("overlay=x=0:y=0:enable='between(t,3.500,4.500)'"));

    // Base canvas generated for total derived duration: 3.5s + 1.0s = 4.5s
    assert_eq!(plan.derived_duration_ms, 4500);
    assert!(plan
        .filter_complex
        .contains("color=c=black:s=1920x1080:r=30:d=4.500[base_v]"));
}

#[test]
fn test_c_overlapping_video_tracks() {
    let (mut spec, mut map, out) = create_base_spec();
    spec.tracks[0].clips.truncate(1); // Track 0 (order 0): c1 at 0..2000ms

    // Add Track 1 (order 1) with overlapping clip
    spec.tracks.push(RenderTrack {
        id: "tr-v1".to_string(),
        kind: TrackKind::Video,
        order: 1,
        muted: false,
        clips: vec![RenderClip {
            id: "c3".to_string(),
            artifact_id: "art-video-3".to_string(),
            source_in_ms: 0,
            source_out_ms: 1000,
            timeline_start_ms: 1000,
            volume: 1.0,
            muted: false,
            transform: ClipTransform::default(),
            has_video: true,
            has_audio: false,
        }],
    });
    map.insert(
        "art-video-3".to_string(),
        PathBuf::from(r"C:\Media Storage\video 3.mp4"),
    );

    let plan = FfmpegCompiler::compile(&spec, &map, out).unwrap();

    // Track 0 clip overlays onto [base_v], Track 1 clip overlays above Track 0
    let base_idx = plan.filter_complex.find("[base_v]").unwrap();
    let layer0_idx = plan.filter_complex.find("[v_layer_0]").unwrap();
    let out_v_idx = plan.filter_complex.find("[out_v]").unwrap();
    assert!(base_idx < layer0_idx);
    assert!(layer0_idx < out_v_idx);
}

#[test]
fn test_d_audio_only_track() {
    let (mut spec, mut map, out) = create_base_spec();
    spec.tracks.push(RenderTrack {
        id: "tr-a0".to_string(),
        kind: TrackKind::Audio,
        order: 1,
        muted: false,
        clips: vec![RenderClip {
            id: "ac1".to_string(),
            artifact_id: "art-audio-1".to_string(),
            source_in_ms: 0,
            source_out_ms: 3000,
            timeline_start_ms: 500,
            volume: 0.8,
            muted: false,
            transform: ClipTransform::default(),
            has_video: false,
            has_audio: true,
        }],
    });
    map.insert(
        "art-audio-1".to_string(),
        PathBuf::from(r"C:\Media Storage\audio 1.wav"),
    );

    let plan = FfmpegCompiler::compile(&spec, &map, out).unwrap();
    assert!(plan
        .filter_complex
        .contains("volume=0.80,adelay=500|500:all=1"));
}

#[test]
fn test_e_muted_clip() {
    let (mut spec, map, out) = create_base_spec();
    // Clip 0 has has_video: true, has_audio: true
    // Enable audio on Clip 1 as well to explicitly verify partial audio exclusion
    spec.tracks[0].clips[1].has_audio = true;
    spec.tracks[0].clips[0].muted = true; // Clip 0 muted
    spec.tracks[0].clips[1].muted = false; // Clip 1 unmuted

    let plan = FfmpegCompiler::compile(&spec, &map, out).unwrap();

    // 1. Explicit proof: clip 0's video stream remains in the video composition
    assert!(
        plan.filter_complex.contains("[0:v]trim="),
        "Muted video clip must have its video stream trimmed"
    );
    assert!(
        plan.filter_complex.contains("setpts=PTS-STARTPTS+0.000/TB"),
        "Muted video clip must have its video PTS mapped"
    );
    assert!(
        plan.filter_complex.contains("[v_clip_0_scaled]"),
        "Muted video clip must be scaled and padded"
    );
    assert!(
        plan.filter_complex
            .contains("overlay=x=0:y=0:enable='between(t,0.000,2.000)'"),
        "Muted video clip must be overlaid onto the canvas"
    );
    // Clip 1's video also remains in the composition
    assert!(plan.filter_complex.contains("[1:v]trim="));

    // 2. Explicit proof: ONLY the muted clip's audio contribution is excluded
    assert!(
        !plan.filter_complex.contains("[0:a]"),
        "Muted clip audio stream [0:a] must be completely excluded"
    );
    assert!(
        plan.filter_complex.contains("[1:a]atrim="),
        "Unmuted clip audio stream [1:a] must remain included"
    );
    assert!(
        plan.filter_complex.contains("adelay=3500|3500:all=1"),
        "Unmuted clip audio must be delayed to its timeline position"
    );

    // 3. Verify case where ALL clips are muted -> silence generator inserted
    let (mut spec_all_muted, map_all, out_all) = create_base_spec();
    spec_all_muted.tracks[0].clips[0].muted = true;
    let plan_all = FfmpegCompiler::compile(&spec_all_muted, &map_all, out_all).unwrap();
    assert!(plan_all.filter_complex.contains("[0:v]"));
    assert!(!plan_all.filter_complex.contains("[0:a]"));
    assert!(plan_all.filter_complex.contains("anullsrc="));
}

#[test]
fn test_f_non_30fps() {
    let (mut spec, map, out) = create_base_spec();
    spec.output.frame_rate = 24.0;

    let plan = FfmpegCompiler::compile(&spec, &map, out).unwrap();
    assert!(plan.filter_complex.contains(":r=24:"));
    assert!(plan.args.contains(&"24".to_string()));
}

#[test]
fn test_g_portrait_canvas() {
    let (mut spec, map, out) = create_base_spec();
    spec.output.width = 1080;
    spec.output.height = 1920;

    let plan = FfmpegCompiler::compile(&spec, &map, out).unwrap();
    assert!(plan.filter_complex.contains("s=1080x1920:"));
    assert!(plan.filter_complex.contains(
        "scale=w=1080:h=1920:force_original_aspect_ratio=decrease,pad=1080:1920:(ow-iw)/2:(oh-ih)/2"
    ));
}

#[test]
fn test_h_path_with_spaces_and_unicode() {
    let (spec, mut map, _) = create_base_spec();
    let unicode_path = PathBuf::from(r"C:\Media Storage\日本語 ビデオ 1.mp4");
    map.insert("art-video-1".to_string(), unicode_path.clone());
    let out_unicode = PathBuf::from(r"C:\Render Output\レンダリング output.mp4");

    let plan = FfmpegCompiler::compile(&spec, &map, out_unicode.clone()).unwrap();

    // Verify input args contain the raw unicode path as a single discrete item
    assert!(plan
        .args
        .contains(&unicode_path.to_string_lossy().to_string()));
    assert_eq!(
        plan.args.last().unwrap(),
        &out_unicode.to_string_lossy().to_string()
    );
}

#[test]
fn test_i_renderspec_contains_no_physical_paths() {
    let (spec, _, _) = create_base_spec();
    let json = serde_json::to_string(&spec).unwrap();
    assert!(!json.contains(r"C:\"));
    assert!(!json.contains("/home/"));
    assert!(!json.contains(".mp4"));
}

#[test]
fn test_audio_mix_duration_policy_bounded_to_timeline() {
    let (mut spec, mut map, out) = create_base_spec();
    // Two audio sources where first is shorter than second
    spec.tracks[0].clips.clear();
    spec.tracks.push(RenderTrack {
        id: "tr-a0".to_string(),
        kind: TrackKind::Audio,
        order: 0,
        muted: false,
        clips: vec![
            RenderClip {
                id: "ac1".to_string(),
                artifact_id: "art-audio-short".to_string(),
                source_in_ms: 0,
                source_out_ms: 2000, // 2s
                timeline_start_ms: 0,
                volume: 1.0,
                muted: false,
                transform: ClipTransform::default(),
                has_video: false,
                has_audio: true,
            },
            RenderClip {
                id: "ac2".to_string(),
                artifact_id: "art-audio-long".to_string(),
                source_in_ms: 0,
                source_out_ms: 6000, // 6s
                timeline_start_ms: 1000,
                volume: 1.0,
                muted: false,
                transform: ClipTransform::default(),
                has_video: false,
                has_audio: true,
            },
        ],
    });
    map.insert(
        "art-audio-short".to_string(),
        PathBuf::from(r"C:\Media Storage\short.wav"),
    );
    map.insert(
        "art-audio-long".to_string(),
        PathBuf::from(r"C:\Media Storage\long.wav"),
    );

    let plan = FfmpegCompiler::compile(&spec, &map, out).unwrap();

    // Derived duration: 1000 + 6000 = 7000ms (7.000s)
    // Audio mix uses duration=longest and then atrim to exact duration
    assert!(plan
        .filter_complex
        .contains("amix=inputs=2:duration=longest"));
    assert!(plan.filter_complex.contains("atrim=end=7.000"));
}

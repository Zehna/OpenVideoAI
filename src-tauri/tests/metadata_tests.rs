use openvideoai_lib::artifacts::metadata::{
    parse_ffprobe_json, probe_media, MediaMetadata, MetadataRepository,
};
use openvideoai_lib::db::DbManager;
use std::fs;
use std::sync::Arc;
use tempfile::tempdir;

#[test]
fn test_parse_ffprobe_json_video_output() {
    let sample_json = r#"{
        "streams": [
            {
                "codec_type": "video",
                "codec_name": "h264",
                "width": 1920,
                "height": 1080,
                "r_frame_rate": "30/1"
            },
            {
                "codec_type": "audio",
                "codec_name": "aac",
                "sample_rate": "48000",
                "channels": 2
            }
        ],
        "format": {
            "duration": "15.500000"
        }
    }"#;

    let metadata = parse_ffprobe_json("art-1", sample_json).expect("failed to parse json");
    assert_eq!(metadata.artifact_id, "art-1");
    assert_eq!(metadata.probe_status, "ok");
    assert_eq!(metadata.duration_ms, Some(15500));
    assert_eq!(metadata.width, Some(1920));
    assert_eq!(metadata.height, Some(1080));
    assert_eq!(metadata.video_codec.as_deref(), Some("h264"));
    assert_eq!(metadata.audio_codec.as_deref(), Some("aac"));
    assert_eq!(metadata.frame_rate, Some(30.0));
    assert_eq!(metadata.sample_rate, Some(48000));
    assert_eq!(metadata.channels, Some(2));
    assert!(metadata.probe_error.is_none());
}

#[test]
fn test_parse_ffprobe_json_audio_only_output() {
    let sample_json = r#"{
        "streams": [
            {
                "codec_type": "audio",
                "codec_name": "mp3",
                "sample_rate": "44100",
                "channels": 1
            }
        ],
        "format": {
            "duration": "60.000000"
        }
    }"#;

    let metadata = parse_ffprobe_json("art-audio", sample_json).expect("failed to parse");
    assert_eq!(metadata.probe_status, "ok");
    assert_eq!(metadata.duration_ms, Some(60000));
    assert_eq!(metadata.video_codec, None);
    assert_eq!(metadata.audio_codec.as_deref(), Some("mp3"));
    assert_eq!(metadata.channels, Some(1));
    assert_eq!(metadata.sample_rate, Some(44100));
}

#[test]
fn test_probe_when_ffprobe_unavailable() {
    let dir = tempdir().expect("tempdir failed");
    let dummy_file = dir.path().join("dummy.mp4");
    fs::write(&dummy_file, b"test").expect("write failed");

    // Pass non-existent path as ffprobe_path
    let fake_ffprobe = dir.path().join("non_existent_ffprobe_binary_xyz.exe");
    let meta = probe_media("art-fail", &dummy_file, Some(&fake_ffprobe));

    assert_eq!(meta.probe_status, "unavailable");
    assert!(meta.probe_error.is_some());
}

#[test]
fn test_probe_when_invalid_media() {
    let dir = tempdir().expect("tempdir failed");
    let corrupt_file = dir.path().join("corrupt.mp4");
    fs::write(&corrupt_file, b"corrupted header bytes").expect("write failed");

    // Using real ffprobe if available
    let meta = probe_media("art-corrupt", &corrupt_file, None);
    // Even if ffprobe fails or is unavailable, probe_media must return safely without panicking
    assert!(meta.probe_status == "failed" || meta.probe_status == "unavailable");
}

#[test]
fn test_metadata_repository_persistence() {
    let dir = tempdir().expect("failed to create tempdir");
    let db_path = dir.path().join("metadata_test.db");
    let db = Arc::new(DbManager::new(db_path).expect("failed to init db"));
    db.run_migrations().expect("failed to run migrations");

    let conn = db.conn().expect("conn failed");

    // First insert parent artifact to satisfy FK
    conn.execute(
        "INSERT INTO artifacts (id, sha256, kind, mime_type, size_bytes, storage_key, original_filename, created_at)
         VALUES ('art-123', 'hash123', 'video', 'video/mp4', 1000, 'artifacts/ha/hash123', 'vid.mp4', '2026-09-08T00:00:00Z');",
        [],
    ).expect("failed to insert artifact");

    let meta = MediaMetadata {
        artifact_id: "art-123".to_string(),
        probe_status: "ok".to_string(),
        duration_ms: Some(12000),
        width: Some(1280),
        height: Some(720),
        video_codec: Some("h264".to_string()),
        audio_codec: Some("aac".to_string()),
        frame_rate: Some(24.0),
        sample_rate: Some(48000),
        channels: Some(2),
        probe_error: None,
    };

    MetadataRepository::insert(&conn, &meta).expect("failed to insert metadata");

    let retrieved = MetadataRepository::get_by_artifact_id(&conn, "art-123")
        .expect("get failed")
        .expect("metadata not found");

    assert_eq!(retrieved, meta);
}

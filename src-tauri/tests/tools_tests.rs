use openvideoai_lib::tools::detector::detect_tool;
use openvideoai_lib::tools::parser::parse_tool_version;

#[test]
fn test_parse_ffmpeg_version_gyan_build() {
    let output = "ffmpeg version 9.0.1-full_build-www.gyan.dev Copyright (c) 2000-2026 the FFmpeg developers\nbuilt with gcc 16.1.0";
    let version = parse_tool_version("ffmpeg", output);
    assert_eq!(version.as_deref(), Some("9.0.1-full_build-www.gyan.dev"));
}

#[test]
fn test_parse_ffmpeg_version_standard_linux() {
    let output = "ffmpeg version 6.1.1-3ubuntu1 Copyright (c) 2000-2023 the FFmpeg developers\nbuilt with gcc";
    let version = parse_tool_version("ffmpeg", output);
    assert_eq!(version.as_deref(), Some("6.1.1-3ubuntu1"));
}

#[test]
fn test_parse_ffprobe_version() {
    let output = "ffprobe version 9.0.1-full_build-www.gyan.dev Copyright (c) 2007-2026 the FFmpeg developers\nconfiguration: --enable-gpl";
    let version = parse_tool_version("ffprobe", output);
    assert_eq!(version.as_deref(), Some("9.0.1-full_build-www.gyan.dev"));
}

#[test]
fn test_parse_invalid_version_output() {
    assert_eq!(parse_tool_version("ffmpeg", ""), None);
    assert_eq!(parse_tool_version("ffmpeg", "Some random error or unrelated banner"), None);
    assert_eq!(parse_tool_version("ffprobe", "unrecognized program"), None);
}

#[test]
fn test_detect_missing_executable() {
    let status = detect_tool("nonexistent_binary_xyz_12345", None);
    assert!(!status.available);
    assert_eq!(status.path, None);
    assert_eq!(status.version, None);
}

#[test]
fn test_detect_ffmpeg_when_available() {
    let status = detect_tool("ffmpeg", None);
    // On this development environment, ffmpeg is installed and on PATH
    assert!(status.available, "Expected ffmpeg to be available on host");
    assert!(status.path.is_some(), "Expected path to be populated");
    assert!(status.version.is_some(), "Expected version to be populated");
    assert!(status.version.as_ref().unwrap().contains("9.0.1"));
}

#[test]
fn test_detect_ffprobe_when_available() {
    let status = detect_tool("ffprobe", None);
    // On this development environment, ffprobe is installed and on PATH
    assert!(status.available, "Expected ffprobe to be available on host");
    assert!(status.path.is_some(), "Expected path to be populated");
    assert!(status.version.is_some(), "Expected version to be populated");
    assert!(status.version.as_ref().unwrap().contains("9.0.1"));
}

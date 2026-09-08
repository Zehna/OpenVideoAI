use crate::db::DbError;
use crate::tools::detector::find_executable;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MediaMetadata {
    pub artifact_id: String,
    pub probe_status: String, // "ok", "unavailable", "failed"
    pub duration_ms: Option<i64>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub frame_rate: Option<f64>,
    pub sample_rate: Option<i32>,
    pub channels: Option<i32>,
    pub probe_error: Option<String>,
}

pub fn parse_ffprobe_json(artifact_id: &str, json_str: &str) -> Result<MediaMetadata, String> {
    let parsed: Value =
        serde_json::from_str(json_str).map_err(|e| format!("Invalid JSON: {}", e))?;

    let mut duration_ms = None;
    if let Some(format) = parsed.get("format") {
        if let Some(dur_str) = format.get("duration").and_then(|d| d.as_str()) {
            if let Ok(dur_f) = dur_str.parse::<f64>() {
                duration_ms = Some((dur_f * 1000.0).round() as i64);
            }
        }
    }

    let mut width = None;
    let mut height = None;
    let mut video_codec = None;
    let mut audio_codec = None;
    let mut frame_rate = None;
    let mut sample_rate = None;
    let mut channels = None;

    if let Some(streams) = parsed.get("streams").and_then(|s| s.as_array()) {
        for stream in streams {
            let codec_type = stream
                .get("codec_type")
                .and_then(|t| t.as_str())
                .unwrap_or("");

            if codec_type == "video" && video_codec.is_none() {
                video_codec = stream
                    .get("codec_name")
                    .and_then(|c| c.as_str())
                    .map(|s| s.to_string());
                width = stream
                    .get("width")
                    .and_then(|w| w.as_i64())
                    .map(|w| w as i32);
                height = stream
                    .get("height")
                    .and_then(|h| h.as_i64())
                    .map(|h| h as i32);

                if let Some(fps_str) = stream.get("r_frame_rate").and_then(|r| r.as_str()) {
                    if let Some((num, den)) = fps_str.split_once('/') {
                        if let (Ok(n), Ok(d)) = (num.parse::<f64>(), den.parse::<f64>()) {
                            if d > 0.0 {
                                frame_rate = Some((n / d * 100.0).round() / 100.0);
                            }
                        }
                    } else if let Ok(fps) = fps_str.parse::<f64>() {
                        frame_rate = Some(fps);
                    }
                }
            } else if codec_type == "audio" && audio_codec.is_none() {
                audio_codec = stream
                    .get("codec_name")
                    .and_then(|c| c.as_str())
                    .map(|s| s.to_string());
                if let Some(sr_val) = stream.get("sample_rate") {
                    if let Some(sr_str) = sr_val.as_str() {
                        sample_rate = sr_str.parse::<i32>().ok();
                    } else if let Some(sr_int) = sr_val.as_i64() {
                        sample_rate = Some(sr_int as i32);
                    }
                }
                channels = stream
                    .get("channels")
                    .and_then(|c| c.as_i64())
                    .map(|c| c as i32);
            }
        }
    }

    Ok(MediaMetadata {
        artifact_id: artifact_id.to_string(),
        probe_status: "ok".to_string(),
        duration_ms,
        width,
        height,
        video_codec,
        audio_codec,
        frame_rate,
        sample_rate,
        channels,
        probe_error: None,
    })
}

pub fn probe_media(
    artifact_id: &str,
    file_path: &Path,
    ffprobe_path: Option<&Path>,
) -> MediaMetadata {
    let exe = match ffprobe_path {
        Some(p) => {
            if !p.is_file() {
                return MediaMetadata {
                    artifact_id: artifact_id.to_string(),
                    probe_status: "unavailable".to_string(),
                    duration_ms: None,
                    width: None,
                    height: None,
                    video_codec: None,
                    audio_codec: None,
                    frame_rate: None,
                    sample_rate: None,
                    channels: None,
                    probe_error: Some(format!(
                        "Specified FFprobe executable not found: {}",
                        p.display()
                    )),
                };
            }
            p.to_path_buf()
        }
        None => match find_executable("ffprobe", None) {
            Some(p) => p,
            None => {
                return MediaMetadata {
                    artifact_id: artifact_id.to_string(),
                    probe_status: "unavailable".to_string(),
                    duration_ms: None,
                    width: None,
                    height: None,
                    video_codec: None,
                    audio_codec: None,
                    frame_rate: None,
                    sample_rate: None,
                    channels: None,
                    probe_error: Some("FFprobe executable not found on system PATH".to_string()),
                };
            }
        },
    };

    let output = match Command::new(&exe)
        .args([
            "-v",
            "quiet",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
        ])
        .arg(file_path)
        .output()
    {
        Ok(out) => out,
        Err(e) => {
            return MediaMetadata {
                artifact_id: artifact_id.to_string(),
                probe_status: "failed".to_string(),
                duration_ms: None,
                width: None,
                height: None,
                video_codec: None,
                audio_codec: None,
                frame_rate: None,
                sample_rate: None,
                channels: None,
                probe_error: Some(format!("Failed to spawn ffprobe: {}", e)),
            };
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return MediaMetadata {
            artifact_id: artifact_id.to_string(),
            probe_status: "failed".to_string(),
            duration_ms: None,
            width: None,
            height: None,
            video_codec: None,
            audio_codec: None,
            frame_rate: None,
            sample_rate: None,
            channels: None,
            probe_error: Some(if stderr.trim().is_empty() {
                "ffprobe exited with error".to_string()
            } else {
                stderr.trim().to_string()
            }),
        };
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    match parse_ffprobe_json(artifact_id, &stdout) {
        Ok(meta) => meta,
        Err(e) => MediaMetadata {
            artifact_id: artifact_id.to_string(),
            probe_status: "failed".to_string(),
            duration_ms: None,
            width: None,
            height: None,
            video_codec: None,
            audio_codec: None,
            frame_rate: None,
            sample_rate: None,
            channels: None,
            probe_error: Some(e),
        },
    }
}

pub struct MetadataRepository;

impl MetadataRepository {
    pub fn insert(conn: &Connection, meta: &MediaMetadata) -> Result<(), DbError> {
        conn.execute(
            "INSERT INTO artifact_media_metadata
             (artifact_id, probe_status, duration_ms, width, height, video_codec, audio_codec, frame_rate, sample_rate, channels, probe_error)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT(artifact_id) DO UPDATE SET
                probe_status = excluded.probe_status,
                duration_ms = excluded.duration_ms,
                width = excluded.width,
                height = excluded.height,
                video_codec = excluded.video_codec,
                audio_codec = excluded.audio_codec,
                frame_rate = excluded.frame_rate,
                sample_rate = excluded.sample_rate,
                channels = excluded.channels,
                probe_error = excluded.probe_error;",
            params![
                meta.artifact_id,
                meta.probe_status,
                meta.duration_ms,
                meta.width,
                meta.height,
                meta.video_codec,
                meta.audio_codec,
                meta.frame_rate,
                meta.sample_rate,
                meta.channels,
                meta.probe_error,
            ],
        )?;
        Ok(())
    }

    pub fn get_by_artifact_id(
        conn: &Connection,
        artifact_id: &str,
    ) -> Result<Option<MediaMetadata>, DbError> {
        let mut stmt = conn.prepare(
            "SELECT artifact_id, probe_status, duration_ms, width, height, video_codec, audio_codec, frame_rate, sample_rate, channels, probe_error
             FROM artifact_media_metadata WHERE artifact_id = ?1;",
        )?;

        let mut rows = stmt.query(params![artifact_id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(MediaMetadata {
                artifact_id: row.get(0)?,
                probe_status: row.get(1)?,
                duration_ms: row.get(2)?,
                width: row.get(3)?,
                height: row.get(4)?,
                video_codec: row.get(5)?,
                audio_codec: row.get(6)?,
                frame_rate: row.get(7)?,
                sample_rate: row.get(8)?,
                channels: row.get(9)?,
                probe_error: row.get(10)?,
            }))
        } else {
            Ok(None)
        }
    }
}

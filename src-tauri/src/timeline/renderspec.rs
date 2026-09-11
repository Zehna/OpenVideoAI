use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::error::TimelineError;
use super::model::*;
use crate::artifacts::metadata::{MediaMetadata, MetadataRepository};

pub trait MediaMetadataLookup {
    fn get_media_metadata(&self, artifact_id: &str)
        -> Result<Option<MediaMetadata>, TimelineError>;
}

impl MediaMetadataLookup for Connection {
    fn get_media_metadata(
        &self,
        artifact_id: &str,
    ) -> Result<Option<MediaMetadata>, TimelineError> {
        MetadataRepository::get_by_artifact_id(self, artifact_id)
            .map_err(|e| TimelineError::Database(e.to_string()))
    }
}

impl MediaMetadataLookup for HashMap<String, MediaMetadata> {
    fn get_media_metadata(
        &self,
        artifact_id: &str,
    ) -> Result<Option<MediaMetadata>, TimelineError> {
        Ok(self.get(artifact_id).cloned())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RenderOutputConfig {
    pub width: u32,
    pub height: u32,
    pub frame_rate: f64,
    pub container: String,    // "mp4"
    pub video_codec: String,  // "h264"
    pub audio_codec: String,  // "aac"
    pub pixel_format: String, // "yuv420p"
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RenderClip {
    pub id: String,
    pub artifact_id: String,
    pub source_in_ms: i64,
    pub source_out_ms: i64,
    pub timeline_start_ms: i64,
    pub volume: f64,
    pub muted: bool,
    pub transform: ClipTransform,
    pub has_video: bool,
    pub has_audio: bool,
}

impl RenderClip {
    pub fn duration_ms(&self) -> i64 {
        self.source_out_ms - self.source_in_ms
    }

    pub fn timeline_end_ms(&self) -> i64 {
        self.timeline_start_ms + self.duration_ms()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RenderTrack {
    pub id: String,
    pub kind: TrackKind,
    pub order: i32,
    pub muted: bool,
    pub clips: Vec<RenderClip>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RenderSpecV1 {
    pub version: u32, // 1
    pub project_id: String,
    pub timeline_id: String,
    pub timeline_revision: i64,
    pub output: RenderOutputConfig,
    pub tracks: Vec<RenderTrack>,
}

impl RenderSpecV1 {
    pub fn derived_duration_ms(&self) -> i64 {
        self.tracks
            .iter()
            .flat_map(|t| t.clips.iter().map(|c| c.timeline_end_ms()))
            .max()
            .unwrap_or(0)
    }
}

pub struct RenderSpecBuilder;

impl RenderSpecBuilder {
    pub fn build_from_timeline<M: MediaMetadataLookup>(
        doc: &TimelineDocument,
        metadata_lookup: &M,
    ) -> Result<RenderSpecV1, TimelineError> {
        let mut total_clips = 0;
        let mut render_tracks = Vec::new();

        for track in &doc.tracks {
            let mut render_clips = Vec::new();
            for clip in &track.clips {
                // Enforce Phase 3 transform contract: default transforms only
                if !clip.transform.is_default() {
                    return Err(TimelineError::InvalidOperation(
                        "Non-default clip transforms not supported in Phase 3".to_string(),
                    ));
                }

                // Freeze stream capability source of truth from committed artifact metadata
                let meta = metadata_lookup
                    .get_media_metadata(&clip.artifact_id)?
                    .ok_or_else(|| TimelineError::ArtifactNotFound(clip.artifact_id.clone()))?;

                let has_video = meta.video_codec.is_some();
                let has_audio = meta.audio_codec.is_some();

                if track.kind == TrackKind::Video && !has_video {
                    return Err(TimelineError::InvalidOperation(format!(
                        "Artifact {} on video track has no video stream",
                        clip.artifact_id
                    )));
                }
                if track.kind == TrackKind::Audio && !has_audio {
                    return Err(TimelineError::InvalidOperation(format!(
                        "Artifact {} on audio track has no audio stream",
                        clip.artifact_id
                    )));
                }

                total_clips += 1;
                render_clips.push(RenderClip {
                    id: clip.id.clone(),
                    artifact_id: clip.artifact_id.clone(),
                    source_in_ms: clip.source_in_ms,
                    source_out_ms: clip.source_out_ms,
                    timeline_start_ms: clip.timeline_start_ms,
                    volume: clip.volume,
                    muted: clip.muted,
                    transform: clip.transform.clone(),
                    has_video,
                    has_audio,
                });
            }

            render_tracks.push(RenderTrack {
                id: track.id.clone(),
                kind: track.kind,
                order: track.order,
                muted: track.muted,
                clips: render_clips,
            });
        }

        if total_clips == 0 {
            return Err(TimelineError::RenderSpecInvalid(
                "Timeline has no clips to render".to_string(),
            ));
        }

        Ok(RenderSpecV1 {
            version: 1,
            project_id: doc.project_id.clone(),
            timeline_id: doc.id.clone(),
            timeline_revision: doc.revision,
            output: RenderOutputConfig {
                width: doc.canvas.width,
                height: doc.canvas.height,
                frame_rate: doc.canvas.frame_rate,
                container: "mp4".to_string(),
                video_codec: "h264".to_string(),
                audio_codec: "aac".to_string(),
                pixel_format: "yuv420p".to_string(),
            },
            tracks: render_tracks,
        })
    }
}

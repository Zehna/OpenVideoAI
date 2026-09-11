use super::error::TimelineError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrackKind {
    Video,
    Audio,
}

impl TrackKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Video => "video",
            Self::Audio => "audio",
        }
    }
}

impl std::str::FromStr for TrackKind {
    type Err = TimelineError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "video" => Ok(Self::Video),
            "audio" => Ok(Self::Audio),
            _ => Err(TimelineError::InvalidOperation(format!(
                "Unknown track kind: {}",
                s
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipTransform {
    pub x: f64,
    pub y: f64,
    pub scale: f64,
    pub rotation_deg: f64,
    pub opacity: f64,
}

impl Default for ClipTransform {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            scale: 1.0,
            rotation_deg: 0.0,
            opacity: 1.0,
        }
    }
}

impl ClipTransform {
    pub fn is_default(&self) -> bool {
        (self.x.abs() < 1e-6)
            && (self.y.abs() < 1e-6)
            && ((self.scale - 1.0).abs() < 1e-6)
            && (self.rotation_deg.abs() < 1e-6)
            && ((self.opacity - 1.0).abs() < 1e-6)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineClip {
    pub id: String,
    pub track_id: String,
    pub artifact_id: String,
    pub source_in_ms: i64,
    pub source_out_ms: i64,
    pub timeline_start_ms: i64,
    pub volume: f64,
    pub muted: bool,
    pub transform: ClipTransform,
}

impl TimelineClip {
    pub fn duration_ms(&self) -> i64 {
        self.source_out_ms - self.source_in_ms
    }

    pub fn timeline_end_ms(&self) -> i64 {
        self.timeline_start_ms + self.duration_ms()
    }

    pub fn validate(&self, source_duration_ms: Option<i64>) -> Result<(), TimelineError> {
        if self.source_in_ms < 0
            || self.source_out_ms <= self.source_in_ms
            || self.timeline_start_ms < 0
        {
            return Err(TimelineError::InvalidClipRange);
        }
        if self.volume < 0.0
            || self.volume > 2.0
            || self.transform.opacity < 0.0
            || self.transform.opacity > 1.0
            || self.transform.scale <= 0.0
        {
            return Err(TimelineError::InvalidOperation(
                "Invalid volume or transform value".to_string(),
            ));
        }
        match source_duration_ms {
            Some(max_dur) => {
                if self.source_out_ms > max_dur {
                    return Err(TimelineError::InvalidClipRange);
                }
            }
            None => return Err(TimelineError::MediaDurationUnknown),
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineTrack {
    pub id: String,
    pub timeline_id: String,
    pub kind: TrackKind,
    pub order: i32,
    pub name: String,
    pub muted: bool,
    pub clips: Vec<TimelineClip>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasConfig {
    pub width: u32,
    pub height: u32,
    pub frame_rate: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineDocument {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub revision: i64,
    pub canvas: CanvasConfig,
    pub tracks: Vec<TimelineTrack>,
    pub created_at: String,
    pub updated_at: String,
}

impl TimelineDocument {
    pub fn derived_duration_ms(&self) -> i64 {
        self.tracks
            .iter()
            .flat_map(|t| t.clips.iter().map(|c| c.timeline_end_ms()))
            .max()
            .unwrap_or(0)
    }
}

use super::model::{ClipTransform, TrackKind};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TimelineMutation {
    AddTrack {
        kind: TrackKind,
        name: String,
    },
    ReorderTrack {
        track_id: String,
        new_order: i32,
    },
    DeleteTrack {
        track_id: String,
    },
    AddClip {
        track_id: String,
        artifact_id: String,
        timeline_start_ms: i64,
        source_in_ms: Option<i64>,
        source_out_ms: Option<i64>,
    },
    RemoveClip {
        clip_id: String,
    },
    MoveClip {
        clip_id: String,
        new_track_id: Option<String>,
        new_start_ms: i64,
    },
    TrimClip {
        clip_id: String,
        source_in_ms: i64,
        source_out_ms: i64,
    },
    SplitClip {
        clip_id: String,
        split_timeline_ms: i64,
    },
    SetClipAudio {
        clip_id: String,
        volume: f64,
        muted: bool,
    },
    SetClipTransform {
        clip_id: String,
        transform: ClipTransform,
    },
    UpdateCanvas {
        width: u32,
        height: u32,
        frame_rate: f64,
    },
}

pub fn check_same_track_overlap(
    clips: &[super::model::TimelineClip],
    new_start_ms: i64,
    new_end_ms: i64,
    ignore_clip_id: Option<&str>,
) -> bool {
    for c in clips {
        if Some(c.id.as_str()) == ignore_clip_id {
            continue;
        }
        let c_start = c.timeline_start_ms;
        let c_end = c.timeline_end_ms();
        if new_start_ms < c_end && new_end_ms > c_start {
            return true;
        }
    }
    false
}

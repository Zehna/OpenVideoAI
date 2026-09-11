use super::error::TimelineError;
use super::model::*;
use super::mutation::*;
use super::repository::TimelineRepository;
use crate::artifacts::metadata::MetadataRepository;
use chrono::Utc;
use rusqlite::{params, Connection, Transaction};
use serde_json::json;
use uuid::Uuid;

#[derive(Default)]
pub struct TimelineService;

impl TimelineService {
    pub fn new() -> Self {
        Self
    }

    pub fn apply_mutation(
        &self,
        conn: &mut Connection,
        timeline_id: &str,
        expected_revision: i64,
        mutation: TimelineMutation,
    ) -> Result<TimelineDocument, TimelineError> {
        let current = TimelineRepository::get_by_id(conn, timeline_id)?
            .ok_or_else(|| TimelineError::TimelineNotFound(timeline_id.to_string()))?;

        if current.revision != expected_revision {
            return Err(TimelineError::RevisionConflict {
                expected: expected_revision,
                actual: current.revision,
            });
        }

        let tx = conn.transaction()?;

        let op_name = match &mutation {
            TimelineMutation::AddTrack { kind, name } => {
                let max_order: Option<i32> = tx.query_row(
                    "SELECT MAX(order_index) FROM timeline_tracks WHERE timeline_id = ?1;",
                    params![timeline_id],
                    |r| r.get(0),
                )?;
                let next_order = max_order.map(|o| o + 1).unwrap_or(0);
                let track_id = format!("track-{}", Uuid::new_v4());
                tx.execute(
                    "INSERT INTO timeline_tracks (id, timeline_id, kind, order_index, name, muted)
                     VALUES (?1, ?2, ?3, ?4, ?5, 0);",
                    params![track_id, timeline_id, kind.as_str(), next_order, name],
                )?;
                "add_track"
            }
            TimelineMutation::ReorderTrack {
                track_id,
                new_order,
            } => {
                tx.execute(
                    "UPDATE timeline_tracks SET order_index = ?1 WHERE id = ?2 AND timeline_id = ?3;",
                    params![new_order, track_id, timeline_id],
                )?;
                "reorder_track"
            }
            TimelineMutation::DeleteTrack { track_id } => {
                tx.execute(
                    "DELETE FROM timeline_tracks WHERE id = ?1 AND timeline_id = ?2;",
                    params![track_id, timeline_id],
                )?;
                "delete_track"
            }
            TimelineMutation::AddClip {
                track_id,
                artifact_id,
                timeline_start_ms,
                source_in_ms,
                source_out_ms,
            } => {
                let meta = MetadataRepository::get_by_artifact_id(&tx, artifact_id)?
                    .ok_or_else(|| TimelineError::ArtifactNotFound(artifact_id.clone()))?;
                let duration_ms = meta
                    .duration_ms
                    .ok_or(TimelineError::MediaDurationUnknown)?;

                let in_ms = source_in_ms.unwrap_or(0);
                let out_ms = source_out_ms.unwrap_or(duration_ms);

                if in_ms < 0 || out_ms <= in_ms || out_ms > duration_ms || *timeline_start_ms < 0 {
                    return Err(TimelineError::InvalidClipRange);
                }

                let clip_end_ms = *timeline_start_ms + (out_ms - in_ms);

                // Check same-track collision
                let track = current
                    .tracks
                    .iter()
                    .find(|t| &t.id == track_id)
                    .ok_or_else(|| {
                        TimelineError::InvalidOperation("Track not found".to_string())
                    })?;
                if check_same_track_overlap(&track.clips, *timeline_start_ms, clip_end_ms, None) {
                    return Err(TimelineError::ClipOverlap);
                }

                let clip_id = format!("clip-{}", Uuid::new_v4());
                tx.execute(
                    "INSERT INTO timeline_clips (id, track_id, artifact_id, source_in_ms, source_out_ms, timeline_start_ms, volume, muted, position_x, position_y, scale, rotation_deg, opacity)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1.0, 0, 0.0, 0.0, 1.0, 0.0, 1.0);",
                    params![clip_id, track_id, artifact_id, in_ms, out_ms, timeline_start_ms],
                )?;
                "add_clip"
            }
            TimelineMutation::RemoveClip { clip_id } => {
                tx.execute(
                    "DELETE FROM timeline_clips WHERE id = ?1;",
                    params![clip_id],
                )?;
                "remove_clip"
            }
            TimelineMutation::MoveClip {
                clip_id,
                new_track_id,
                new_start_ms,
            } => {
                if *new_start_ms < 0 {
                    return Err(TimelineError::InvalidClipRange);
                }
                let mut target_track_id = String::new();
                let mut clip_opt = None;
                for t in &current.tracks {
                    for c in &t.clips {
                        if &c.id == clip_id {
                            clip_opt = Some(c.clone());
                            target_track_id = new_track_id.clone().unwrap_or_else(|| t.id.clone());
                            break;
                        }
                    }
                }
                let clip = clip_opt
                    .ok_or_else(|| TimelineError::InvalidOperation("Clip not found".to_string()))?;
                let clip_end = *new_start_ms + clip.duration_ms();

                let target_track = current
                    .tracks
                    .iter()
                    .find(|t| t.id == target_track_id)
                    .ok_or_else(|| {
                        TimelineError::InvalidOperation("Target track not found".to_string())
                    })?;

                if check_same_track_overlap(
                    &target_track.clips,
                    *new_start_ms,
                    clip_end,
                    Some(clip_id),
                ) {
                    return Err(TimelineError::ClipOverlap);
                }

                tx.execute(
                    "UPDATE timeline_clips SET track_id = ?1, timeline_start_ms = ?2 WHERE id = ?3;",
                    params![target_track_id, new_start_ms, clip_id],
                )?;
                "move_clip"
            }
            TimelineMutation::TrimClip {
                clip_id,
                source_in_ms,
                source_out_ms,
            } => {
                let mut found = None;
                for t in &current.tracks {
                    for c in &t.clips {
                        if &c.id == clip_id {
                            found = Some((t.clone(), c.clone()));
                            break;
                        }
                    }
                }
                let (track, clip) = found
                    .ok_or_else(|| TimelineError::InvalidOperation("Clip not found".to_string()))?;
                let meta = MetadataRepository::get_by_artifact_id(&tx, &clip.artifact_id)?
                    .ok_or_else(|| TimelineError::ArtifactNotFound(clip.artifact_id.clone()))?;
                let max_dur = meta
                    .duration_ms
                    .ok_or(TimelineError::MediaDurationUnknown)?;

                if *source_in_ms < 0 || *source_out_ms <= *source_in_ms || *source_out_ms > max_dur
                {
                    return Err(TimelineError::InvalidClipRange);
                }

                let new_end_ms = clip.timeline_start_ms + (*source_out_ms - *source_in_ms);
                if check_same_track_overlap(
                    &track.clips,
                    clip.timeline_start_ms,
                    new_end_ms,
                    Some(clip_id),
                ) {
                    return Err(TimelineError::ClipOverlap);
                }

                tx.execute(
                    "UPDATE timeline_clips SET source_in_ms = ?1, source_out_ms = ?2 WHERE id = ?3;",
                    params![source_in_ms, source_out_ms, clip_id],
                )?;
                "trim_clip"
            }
            TimelineMutation::SplitClip {
                clip_id,
                split_timeline_ms,
            } => {
                let mut found = None;
                for t in &current.tracks {
                    for c in &t.clips {
                        if &c.id == clip_id {
                            found = Some((t.clone(), c.clone()));
                            break;
                        }
                    }
                }
                let (track, clip) = found
                    .ok_or_else(|| TimelineError::InvalidOperation("Clip not found".to_string()))?;
                if *split_timeline_ms <= clip.timeline_start_ms
                    || *split_timeline_ms >= clip.timeline_end_ms()
                {
                    return Err(TimelineError::InvalidClipRange);
                }

                let rel_offset = *split_timeline_ms - clip.timeline_start_ms;
                let source_split = clip.source_in_ms + rel_offset;

                // Update Clip A
                tx.execute(
                    "UPDATE timeline_clips SET source_out_ms = ?1 WHERE id = ?2;",
                    params![source_split, clip_id],
                )?;

                // Insert Clip B
                let clip_b_id = format!("clip-{}", Uuid::new_v4());
                tx.execute(
                    "INSERT INTO timeline_clips (id, track_id, artifact_id, source_in_ms, source_out_ms, timeline_start_ms, volume, muted, position_x, position_y, scale, rotation_deg, opacity)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13);",
                    params![
                        clip_b_id,
                        track.id,
                        clip.artifact_id,
                        source_split,
                        clip.source_out_ms,
                        split_timeline_ms,
                        clip.volume,
                        clip.muted as i32,
                        clip.transform.x,
                        clip.transform.y,
                        clip.transform.scale,
                        clip.transform.rotation_deg,
                        clip.transform.opacity,
                    ],
                )?;
                "split_clip"
            }
            TimelineMutation::SetClipAudio {
                clip_id,
                volume,
                muted,
            } => {
                if *volume < 0.0 || *volume > 2.0 {
                    return Err(TimelineError::InvalidOperation(
                        "Volume must be in [0.0, 2.0]".to_string(),
                    ));
                }
                tx.execute(
                    "UPDATE timeline_clips SET volume = ?1, muted = ?2 WHERE id = ?3;",
                    params![volume, *muted as i32, clip_id],
                )?;
                "set_clip_audio"
            }
            TimelineMutation::SetClipTransform { clip_id, transform } => {
                if transform.scale <= 0.0 || transform.opacity < 0.0 || transform.opacity > 1.0 {
                    return Err(TimelineError::InvalidOperation(
                        "Invalid transform parameters".to_string(),
                    ));
                }
                tx.execute(
                    "UPDATE timeline_clips SET position_x = ?1, position_y = ?2, scale = ?3, rotation_deg = ?4, opacity = ?5 WHERE id = ?6;",
                    params![
                        transform.x,
                        transform.y,
                        transform.scale,
                        transform.rotation_deg,
                        transform.opacity,
                        clip_id
                    ],
                )?;
                "set_clip_transform"
            }
            TimelineMutation::UpdateCanvas {
                width,
                height,
                frame_rate,
            } => {
                tx.execute(
                    "UPDATE timelines SET width = ?1, height = ?2, frame_rate = ?3 WHERE id = ?4;",
                    params![width, height, frame_rate, timeline_id],
                )?;
                "update_canvas"
            }
        };

        // Monotonic revision + 1
        let new_revision = current.revision + 1;
        let now = Utc::now().to_rfc3339();

        // Load updated document before commit to create snapshot
        let updated_doc = Self::load_document_tx(&tx, timeline_id, new_revision, &now)?;

        // History handling:
        // 1. Invalidate redo branch: delete entries where history_index > current cursor
        let cursor: i64 = tx.query_row(
            "SELECT history_cursor FROM timelines WHERE id = ?1;",
            params![timeline_id],
            |r| r.get(0),
        )?;
        tx.execute(
            "DELETE FROM timeline_history WHERE timeline_id = ?1 AND history_index > ?2;",
            params![timeline_id, cursor],
        )?;

        // 2. Insert new snapshot at cursor + 1
        let new_cursor = cursor + 1;
        let snapshot = json!({
            "schemaVersion": 1,
            "timeline": updated_doc,
        });
        tx.execute(
            "INSERT INTO timeline_history (timeline_id, history_index, snapshot_json, operation_type, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5);",
            params![timeline_id, new_cursor, snapshot.to_string(), op_name, now],
        )?;

        // 3. Prune oldest snapshots if total > 50, but NEVER prune new_cursor
        let total_snapshots: i64 = tx.query_row(
            "SELECT count(*) FROM timeline_history WHERE timeline_id = ?1;",
            params![timeline_id],
            |r| r.get(0),
        )?;
        if total_snapshots > 50 {
            let to_remove = total_snapshots - 50;
            tx.execute(
                "DELETE FROM timeline_history WHERE timeline_id = ?1 AND history_index IN (
                    SELECT history_index FROM timeline_history WHERE timeline_id = ?1 AND history_index != ?2 ORDER BY history_index ASC LIMIT ?3
                );",
                params![timeline_id, new_cursor, to_remove],
            )?;
        }

        // 4. Update timeline revision and history cursor
        tx.execute(
            "UPDATE timelines SET revision = ?1, history_cursor = ?2, updated_at = ?3 WHERE id = ?4;",
            params![new_revision, new_cursor, now, timeline_id],
        )?;

        tx.commit()?;
        Ok(updated_doc)
    }

    pub fn load_document_tx(
        tx: &Transaction,
        timeline_id: &str,
        revision: i64,
        updated_at: &str,
    ) -> Result<TimelineDocument, TimelineError> {
        let (project_id, name, width, height, frame_rate, created_at): (
            String,
            String,
            u32,
            u32,
            f64,
            String,
        ) = tx.query_row(
            "SELECT project_id, name, width, height, frame_rate, created_at FROM timelines WHERE id = ?1;",
            params![timeline_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
        )?;

        let mut track_stmt = tx.prepare(
            "SELECT id, timeline_id, kind, order_index, name, muted FROM timeline_tracks WHERE timeline_id = ?1 ORDER BY order_index ASC;",
        )?;
        let track_rows = track_stmt.query_map(params![timeline_id], |r| {
            let kind_str: String = r.get(2)?;
            let kind = kind_str.parse::<TrackKind>().unwrap_or(TrackKind::Video);
            let muted_int: i32 = r.get(5)?;
            Ok(TimelineTrack {
                id: r.get(0)?,
                timeline_id: r.get(1)?,
                kind,
                order: r.get(3)?,
                name: r.get(4)?,
                muted: muted_int != 0,
                clips: Vec::new(),
            })
        })?;

        let mut tracks = Vec::new();
        for t in track_rows {
            tracks.push(t?);
        }

        let mut clip_stmt = tx.prepare(
            "SELECT id, track_id, artifact_id, source_in_ms, source_out_ms, timeline_start_ms, volume, muted, position_x, position_y, scale, rotation_deg, opacity
             FROM timeline_clips WHERE track_id = ?1 ORDER BY timeline_start_ms ASC;",
        )?;

        for track in &mut tracks {
            let clip_rows = clip_stmt.query_map(params![&track.id], |r| {
                let muted_int: i32 = r.get(7)?;
                Ok(TimelineClip {
                    id: r.get(0)?,
                    track_id: r.get(1)?,
                    artifact_id: r.get(2)?,
                    source_in_ms: r.get(3)?,
                    source_out_ms: r.get(4)?,
                    timeline_start_ms: r.get(5)?,
                    volume: r.get(6)?,
                    muted: muted_int != 0,
                    transform: ClipTransform {
                        x: r.get(8)?,
                        y: r.get(9)?,
                        scale: r.get(10)?,
                        rotation_deg: r.get(11)?,
                        opacity: r.get(12)?,
                    },
                })
            })?;
            for c in clip_rows {
                track.clips.push(c?);
            }
        }

        Ok(TimelineDocument {
            id: timeline_id.to_string(),
            project_id,
            name,
            revision,
            canvas: CanvasConfig {
                width,
                height,
                frame_rate,
            },
            tracks,
            created_at,
            updated_at: updated_at.to_string(),
        })
    }
}

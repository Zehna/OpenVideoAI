use super::error::TimelineError;
use super::model::*;
use chrono::Utc;
use rusqlite::{params, Connection};
use serde_json::json;

pub struct TimelineRepository;

impl TimelineRepository {
    pub fn create(
        conn: &mut Connection,
        id: &str,
        project_id: &str,
        name: &str,
        canvas: CanvasConfig,
    ) -> Result<TimelineDocument, TimelineError> {
        let now = Utc::now().to_rfc3339();
        let tx = conn.transaction()?;

        tx.execute(
            "INSERT INTO timelines (id, project_id, name, revision, width, height, frame_rate, history_cursor, created_at, updated_at)
             VALUES (?1, ?2, ?3, 1, ?4, ?5, ?6, 0, ?7, ?7);",
            params![
                id,
                project_id,
                name,
                canvas.width,
                canvas.height,
                canvas.frame_rate,
                now
            ],
        )?;

        let initial_doc = TimelineDocument {
            id: id.to_string(),
            project_id: project_id.to_string(),
            name: name.to_string(),
            revision: 1,
            canvas,
            tracks: Vec::new(),
            created_at: now.clone(),
            updated_at: now.clone(),
        };

        let snapshot = json!({
            "schemaVersion": 1,
            "timeline": initial_doc,
        });

        tx.execute(
            "INSERT INTO timeline_history (timeline_id, history_index, snapshot_json, operation_type, created_at)
             VALUES (?1, 0, ?2, 'create', ?3);",
            params![id, snapshot.to_string(), now],
        )?;

        tx.commit()?;
        Ok(initial_doc)
    }

    pub fn get_by_id(
        conn: &Connection,
        id: &str,
    ) -> Result<Option<TimelineDocument>, TimelineError> {
        let mut stmt = conn.prepare(
            "SELECT id, project_id, name, revision, width, height, frame_rate, created_at, updated_at
             FROM timelines WHERE id = ?1;",
        )?;

        let doc = stmt.query_row(params![id], |row| {
            let width: u32 = row.get(4)?;
            let height: u32 = row.get(5)?;
            let frame_rate: f64 = row.get(6)?;
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                CanvasConfig {
                    width,
                    height,
                    frame_rate,
                },
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
            ))
        });

        let (tl_id, project_id, name, revision, canvas, created_at, updated_at) = match doc {
            Ok(v) => v,
            Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
            Err(e) => return Err(TimelineError::Database(e.to_string())),
        };

        // Load tracks in order_index ASC
        let mut track_stmt = conn.prepare(
            "SELECT id, timeline_id, kind, order_index, name, muted
             FROM timeline_tracks WHERE timeline_id = ?1 ORDER BY order_index ASC;",
        )?;

        let track_rows = track_stmt.query_map(params![&tl_id], |row| {
            let kind_str: String = row.get(2)?;
            let kind = kind_str.parse::<TrackKind>().map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    2,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })?;
            let muted_int: i32 = row.get(5)?;
            Ok(TimelineTrack {
                id: row.get(0)?,
                timeline_id: row.get(1)?,
                kind,
                order: row.get(3)?,
                name: row.get(4)?,
                muted: muted_int != 0,
                clips: Vec::new(),
            })
        })?;

        let mut tracks = Vec::new();
        for t in track_rows {
            tracks.push(t?);
        }

        // Load clips for each track in timeline_start_ms ASC
        let mut clip_stmt = conn.prepare(
            "SELECT id, track_id, artifact_id, source_in_ms, source_out_ms, timeline_start_ms, volume, muted, position_x, position_y, scale, rotation_deg, opacity
             FROM timeline_clips WHERE track_id = ?1 ORDER BY timeline_start_ms ASC;",
        )?;

        for track in &mut tracks {
            let clip_rows = clip_stmt.query_map(params![&track.id], |row| {
                let muted_int: i32 = row.get(7)?;
                Ok(TimelineClip {
                    id: row.get(0)?,
                    track_id: row.get(1)?,
                    artifact_id: row.get(2)?,
                    source_in_ms: row.get(3)?,
                    source_out_ms: row.get(4)?,
                    timeline_start_ms: row.get(5)?,
                    volume: row.get(6)?,
                    muted: muted_int != 0,
                    transform: ClipTransform {
                        x: row.get(8)?,
                        y: row.get(9)?,
                        scale: row.get(10)?,
                        rotation_deg: row.get(11)?,
                        opacity: row.get(12)?,
                    },
                })
            })?;

            for c in clip_rows {
                track.clips.push(c?);
            }
        }

        Ok(Some(TimelineDocument {
            id: tl_id,
            project_id,
            name,
            revision,
            canvas,
            tracks,
            created_at,
            updated_at,
        }))
    }

    pub fn list_by_project(
        conn: &Connection,
        project_id: &str,
    ) -> Result<Vec<TimelineDocument>, TimelineError> {
        let mut stmt = conn
            .prepare("SELECT id FROM timelines WHERE project_id = ?1 ORDER BY created_at ASC;")?;
        let rows = stmt.query_map(params![project_id], |row| row.get::<_, String>(0))?;
        let mut docs = Vec::new();
        for id_res in rows {
            let id = id_res?;
            if let Some(doc) = Self::get_by_id(conn, &id)? {
                docs.push(doc);
            }
        }
        Ok(docs)
    }

    pub fn delete(conn: &Connection, id: &str) -> Result<bool, TimelineError> {
        let count = conn.execute("DELETE FROM timelines WHERE id = ?1;", params![id])?;
        Ok(count > 0)
    }
}

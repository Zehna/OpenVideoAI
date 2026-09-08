use super::model::{Artifact, ArtifactKind};
use crate::db::DbError;
use rusqlite::{params, Connection};
use std::str::FromStr;

pub struct ArtifactRepository;

impl ArtifactRepository {
    pub fn insert(conn: &Connection, artifact: &Artifact) -> Result<(), DbError> {
        conn.execute(
            "INSERT INTO artifacts (id, sha256, kind, mime_type, size_bytes, storage_key, original_filename, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8);",
            params![
                artifact.id,
                artifact.sha256,
                artifact.kind.as_str(),
                artifact.mime_type,
                artifact.size_bytes as i64,
                artifact.storage_key,
                artifact.original_filename,
                artifact.created_at,
            ],
        )?;
        Ok(())
    }

    pub fn get_by_id(conn: &Connection, id: &str) -> Result<Option<Artifact>, DbError> {
        let mut stmt = conn.prepare(
            "SELECT id, sha256, kind, mime_type, size_bytes, storage_key, original_filename, created_at
             FROM artifacts WHERE id = ?1;",
        )?;

        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            let kind_str: String = row.get(2)?;
            let size_bytes: i64 = row.get(4)?;
            Ok(Some(Artifact {
                id: row.get(0)?,
                sha256: row.get(1)?,
                kind: ArtifactKind::from_str(&kind_str).unwrap_or(ArtifactKind::Other),
                mime_type: row.get(3)?,
                size_bytes: size_bytes as u64,
                storage_key: row.get(5)?,
                original_filename: row.get(6)?,
                created_at: row.get(7)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn get_by_sha256(conn: &Connection, sha256: &str) -> Result<Option<Artifact>, DbError> {
        let mut stmt = conn.prepare(
            "SELECT id, sha256, kind, mime_type, size_bytes, storage_key, original_filename, created_at
             FROM artifacts WHERE sha256 = ?1;",
        )?;

        let mut rows = stmt.query(params![sha256])?;
        if let Some(row) = rows.next()? {
            let kind_str: String = row.get(2)?;
            let size_bytes: i64 = row.get(4)?;
            Ok(Some(Artifact {
                id: row.get(0)?,
                sha256: row.get(1)?,
                kind: ArtifactKind::from_str(&kind_str).unwrap_or(ArtifactKind::Other),
                mime_type: row.get(3)?,
                size_bytes: size_bytes as u64,
                storage_key: row.get(5)?,
                original_filename: row.get(6)?,
                created_at: row.get(7)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn link_to_project(
        conn: &Connection,
        project_id: &str,
        artifact_id: &str,
        added_at: &str,
    ) -> Result<bool, DbError> {
        let count = conn.execute(
            "INSERT OR IGNORE INTO project_artifacts (project_id, artifact_id, added_at)
             VALUES (?1, ?2, ?3);",
            params![project_id, artifact_id, added_at],
        )?;
        Ok(count > 0)
    }

    pub fn list_for_project(
        conn: &Connection,
        project_id: &str,
    ) -> Result<Vec<(Artifact, String)>, DbError> {
        let mut stmt = conn.prepare(
            "SELECT a.id, a.sha256, a.kind, a.mime_type, a.size_bytes, a.storage_key, a.original_filename, a.created_at, pa.added_at
             FROM artifacts a
             JOIN project_artifacts pa ON a.id = pa.artifact_id
             WHERE pa.project_id = ?1
             ORDER BY pa.added_at DESC;",
        )?;

        let rows = stmt.query_map(params![project_id], |row| {
            let kind_str: String = row.get(2)?;
            let size_bytes: i64 = row.get(4)?;
            let artifact = Artifact {
                id: row.get(0)?,
                sha256: row.get(1)?,
                kind: ArtifactKind::from_str(&kind_str).unwrap_or(ArtifactKind::Other),
                mime_type: row.get(3)?,
                size_bytes: size_bytes as u64,
                storage_key: row.get(5)?,
                original_filename: row.get(6)?,
                created_at: row.get(7)?,
            };
            let added_at: String = row.get(8)?;
            Ok((artifact, added_at))
        })?;

        let mut results = Vec::new();
        for item in rows {
            results.push(item?);
        }
        Ok(results)
    }
}

use super::model::Project;
use crate::db::DbError;
use rusqlite::{params, Connection};

pub struct ProjectRepository;

impl ProjectRepository {
    pub fn create(conn: &Connection, project: &Project) -> Result<(), DbError> {
        conn.execute(
            "INSERT INTO projects (id, name, description, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5);",
            params![
                project.id,
                project.name,
                project.description,
                project.created_at,
                project.updated_at,
            ],
        )?;
        Ok(())
    }

    pub fn get_by_id(conn: &Connection, id: &str) -> Result<Option<Project>, DbError> {
        let mut stmt = conn.prepare(
            "SELECT id, name, description, created_at, updated_at
             FROM projects WHERE id = ?1;",
        )?;

        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Project {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn list_all(conn: &Connection) -> Result<Vec<Project>, DbError> {
        let mut stmt = conn.prepare(
            "SELECT id, name, description, created_at, updated_at
             FROM projects ORDER BY updated_at DESC;",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(Project {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
            })
        })?;

        let mut projects = Vec::new();
        for p in rows {
            projects.push(p?);
        }
        Ok(projects)
    }

    pub fn update(conn: &Connection, project: &Project) -> Result<bool, DbError> {
        let count = conn.execute(
            "UPDATE projects SET name = ?1, description = ?2, updated_at = ?3 WHERE id = ?4;",
            params![
                project.name,
                project.description,
                project.updated_at,
                project.id,
            ],
        )?;
        Ok(count > 0)
    }
}

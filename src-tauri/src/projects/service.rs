use super::model::Project;
use super::repository::ProjectRepository;
use crate::db::{DbError, DbManager};
use chrono::Utc;
use std::sync::Arc;
use thiserror::Error;
use uuid::Uuid;

#[derive(Error, Debug)]
pub enum ProjectError {
    #[error("Project not found: {0}")]
    NotFound(String),

    #[error("Invalid project name: name cannot be empty")]
    InvalidName,

    #[error("Database error: {0}")]
    Database(#[from] DbError),
}

pub struct ProjectService {
    db: Arc<DbManager>,
}

impl ProjectService {
    pub fn new(db: Arc<DbManager>) -> Self {
        Self { db }
    }

    pub fn create_project(
        &self,
        name: String,
        description: Option<String>,
    ) -> Result<Project, ProjectError> {
        let trimmed_name = name.trim().to_string();
        if trimmed_name.is_empty() {
            return Err(ProjectError::InvalidName);
        }

        let now = Utc::now().to_rfc3339();
        let project = Project {
            id: Uuid::new_v4().to_string(),
            name: trimmed_name,
            description: description
                .map(|d| d.trim().to_string())
                .filter(|d| !d.is_empty()),
            created_at: now.clone(),
            updated_at: now,
        };

        let conn = self.db.conn()?;
        ProjectRepository::create(&conn, &project)?;
        tracing::info!(project_id = %project.id, name = %project.name, "Created project");

        Ok(project)
    }

    pub fn list_projects(&self) -> Result<Vec<Project>, ProjectError> {
        let conn = self.db.conn()?;
        let projects = ProjectRepository::list_all(&conn)?;
        Ok(projects)
    }

    pub fn get_project(&self, id: &str) -> Result<Project, ProjectError> {
        let conn = self.db.conn()?;
        let project = ProjectRepository::get_by_id(&conn, id)?
            .ok_or_else(|| ProjectError::NotFound(id.to_string()))?;
        Ok(project)
    }

    pub fn update_project(
        &self,
        id: &str,
        name: String,
        description: Option<String>,
    ) -> Result<Project, ProjectError> {
        let trimmed_name = name.trim().to_string();
        if trimmed_name.is_empty() {
            return Err(ProjectError::InvalidName);
        }

        let conn = self.db.conn()?;
        let existing = ProjectRepository::get_by_id(&conn, id)?
            .ok_or_else(|| ProjectError::NotFound(id.to_string()))?;

        let updated = Project {
            id: existing.id,
            name: trimmed_name,
            description: description
                .map(|d| d.trim().to_string())
                .filter(|d| !d.is_empty()),
            created_at: existing.created_at,
            updated_at: Utc::now().to_rfc3339(),
        };

        ProjectRepository::update(&conn, &updated)?;
        tracing::info!(project_id = %id, name = %updated.name, "Updated project");

        Ok(updated)
    }
}

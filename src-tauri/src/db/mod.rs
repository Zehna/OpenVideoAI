pub mod migrations;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DbError {
    #[error("Database IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("Database lock poisoned")]
    PoisonedLock,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseStatus {
    pub ok: bool,
    pub path: String,
}

pub struct DbManager {
    path: PathBuf,
    conn: Mutex<Connection>,
}

impl DbManager {
    /// Initialize SQLite connection and configure WAL mode.
    pub fn new(path: PathBuf) -> Result<Self, DbError> {
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)?;
            }
        }

        let conn = Connection::open(&path)?;

        // Configure WAL mode and normal synchronous for performant and durable local operation
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;",
        )?;

        tracing::info!(path = %path.display(), "Initialized SQLite database with WAL mode");

        Ok(Self {
            path,
            conn: Mutex::new(conn),
        })
    }

    /// Retrieve the current SQLite journal mode.
    pub fn get_journal_mode(&self) -> Result<String, DbError> {
        let conn = self.conn.lock().map_err(|_| DbError::PoisonedLock)?;
        let mode: String = conn.query_row("PRAGMA journal_mode;", [], |row| row.get(0))?;
        Ok(mode)
    }

    /// Run all pending schema migrations idempotently.
    pub fn run_migrations(&self) -> Result<(), DbError> {
        let mut conn = self.conn.lock().map_err(|_| DbError::PoisonedLock)?;
        migrations::run_all_migrations(&mut conn)?;
        Ok(())
    }

    /// Retrieve the count of successfully applied migrations.
    pub fn get_migration_count(&self) -> Result<usize, DbError> {
        let conn = self.conn.lock().map_err(|_| DbError::PoisonedLock)?;
        let count: usize = conn.query_row("SELECT COUNT(*) FROM _migrations;", [], |row| row.get(0))?;
        Ok(count)
    }

    /// Execute an active database health query to verify read/write readiness.
    pub fn check_health(&self) -> Result<DatabaseStatus, DbError> {
        let conn = self.conn.lock().map_err(|_| DbError::PoisonedLock)?;
        
        // Execute active check query
        let query_result: Result<i64, rusqlite::Error> =
            conn.query_row("SELECT COUNT(*) FROM _migrations;", [], |row| row.get(0));

        let ok = query_result.is_ok();
        Ok(DatabaseStatus {
            ok,
            path: self.path.to_string_lossy().to_string(),
        })
    }

    /// Expose path
    pub fn path(&self) -> &PathBuf {
        &self.path
    }
}

use super::model::DomainEvent;
use crate::db::{DbError, DbManager};
use rusqlite::{params, Connection, Row};
use std::sync::{Arc, Mutex};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum EventError {
    #[error("Database error: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("Database manager error: {0}")]
    DbManager(#[from] DbError),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type EventHandler = Arc<dyn Fn(&DomainEvent<serde_json::Value>) + Send + Sync>;

/// Domain Event Bus supporting transactional SQLite audit persistence and process-local broadcast.
///
/// ## Delivery & Durability Semantics:
/// 1. **Durable Persistence:** Domain events semantically tied to job state transitions MUST be persisted
///    in the same SQLite transaction as the state mutation (`EventBus::record_event_tx`). If the transaction
///    aborts or crashes, the event is rolled back. Once committed to `domain_events`, the audit log is durable.
/// 2. **Best-Effort Broadcast:** Process-local in-memory subscribers (such as the Tauri webview event bridge)
///    are invoked via `EventBus::broadcast` ONLY AFTER the enclosing SQLite transaction has committed.
/// 3. **External Delivery:** Reliable outbox polling / external webhooks / remote workers are explicitly out
///    of scope for Phase 2 and deferred to future network integrations.
pub struct EventBus {
    db: Arc<DbManager>,
    subscribers: Mutex<Vec<EventHandler>>,
}

fn row_to_event(row: &Row<'_>) -> rusqlite::Result<DomainEvent<serde_json::Value>> {
    let payload_str: String = row.get(5)?;
    let payload = serde_json::from_str(&payload_str).unwrap_or(serde_json::Value::Null);

    Ok(DomainEvent {
        event_id: row.get(0)?,
        event_type: row.get(1)?,
        version: row.get(2)?,
        project_id: row.get(3)?,
        job_id: row.get(4)?,
        payload,
        timestamp: row.get(6)?,
    })
}

impl EventBus {
    pub fn new(db: Arc<DbManager>) -> Self {
        Self {
            db,
            subscribers: Mutex::new(Vec::new()),
        }
    }

    /// Register a subscriber callback to receive domain events synchronously.
    pub fn subscribe(&self, handler: EventHandler) {
        if let Ok(mut subs) = self.subscribers.lock() {
            subs.push(handler);
        }
    }

    /// Persist a domain event inside an existing SQLite transaction/connection.
    /// Guarantees that state mutations and event logging are atomic.
    pub fn record_event_tx(
        conn: &Connection,
        event: &DomainEvent<serde_json::Value>,
    ) -> Result<(), EventError> {
        let payload_str = serde_json::to_string(&event.payload)?;

        conn.execute(
            "INSERT INTO domain_events (id, event_type, version, project_id, job_id, payload, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                event.event_id,
                event.event_type,
                event.version,
                event.project_id,
                event.job_id,
                payload_str,
                event.timestamp,
            ],
        )?;

        tracing::info!(
            event_id = %event.event_id,
            event_type = %event.event_type,
            "Persisted domain event to SQLite"
        );

        Ok(())
    }

    /// Broadcast a domain event to all registered in-memory subscribers.
    /// MUST be called AFTER the transaction persisting the event has committed.
    pub fn broadcast(&self, event: &DomainEvent<serde_json::Value>) {
        if let Ok(subs) = self.subscribers.lock() {
            for sub in subs.iter() {
                sub(event);
            }
        }
    }

    /// Convenience standalone publish: persists to SQLite in its own connection, then broadcasts.
    pub fn publish(&self, event: DomainEvent<serde_json::Value>) -> Result<(), EventError> {
        {
            let conn = self.db.conn()?;
            Self::record_event_tx(&conn, &event)?;
        }
        self.broadcast(&event);
        Ok(())
    }

    /// Retrieve persisted domain events with optional filtering.
    pub fn list_events(
        &self,
        event_type: Option<&str>,
        job_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<DomainEvent<serde_json::Value>>, EventError> {
        let conn = self.db.conn()?;
        let mut sql = "SELECT id, event_type, version, project_id, job_id, payload, created_at FROM domain_events WHERE 1=1".to_string();
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(et) = event_type {
            sql.push_str(" AND event_type = ?");
            params_vec.push(Box::new(et.to_string()));
        }

        if let Some(jid) = job_id {
            sql.push_str(" AND job_id = ?");
            params_vec.push(Box::new(jid.to_string()));
        }

        sql.push_str(" ORDER BY created_at DESC LIMIT ?");
        params_vec.push(Box::new(limit as i64));

        let mut stmt = conn.prepare(&sql)?;
        let param_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|b| b.as_ref()).collect();
        let rows = stmt.query_map(param_refs.as_slice(), row_to_event)?;

        let mut events = Vec::new();
        for row in rows {
            events.push(row?);
        }
        Ok(events)
    }
}

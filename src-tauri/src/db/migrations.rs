use rusqlite::{Connection, Transaction};
use std::collections::HashSet;

pub struct Migration {
    pub version: i32,
    pub name: &'static str,
    pub sql: &'static str,
}

pub static MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "initial_foundation_schema",
        sql: "
            CREATE TABLE IF NOT EXISTS app_metadata (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            INSERT OR IGNORE INTO app_metadata (key, value) VALUES ('initialized_at', datetime('now'));
        ",
    },
    Migration {
        version: 2,
        name: "project_and_artifact_core",
        sql: "
            CREATE TABLE IF NOT EXISTS projects (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS artifacts (
                id TEXT PRIMARY KEY,
                sha256 TEXT NOT NULL UNIQUE,
                kind TEXT NOT NULL,
                mime_type TEXT,
                size_bytes INTEGER NOT NULL,
                storage_key TEXT NOT NULL UNIQUE,
                original_filename TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS artifact_media_metadata (
                artifact_id TEXT PRIMARY KEY REFERENCES artifacts(id) ON DELETE CASCADE,
                probe_status TEXT NOT NULL,
                duration_ms INTEGER,
                width INTEGER,
                height INTEGER,
                video_codec TEXT,
                audio_codec TEXT,
                frame_rate REAL,
                sample_rate INTEGER,
                channels INTEGER,
                probe_error TEXT
            );

            CREATE TABLE IF NOT EXISTS project_artifacts (
                project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                artifact_id TEXT NOT NULL REFERENCES artifacts(id) ON DELETE CASCADE,
                added_at TEXT NOT NULL,
                PRIMARY KEY(project_id, artifact_id)
            );

            CREATE INDEX IF NOT EXISTS idx_artifacts_sha256 ON artifacts(sha256);
            CREATE INDEX IF NOT EXISTS idx_project_artifacts_project ON project_artifacts(project_id);
        ",
    },
    Migration {
        version: 3,
        name: "durable_jobs_and_events",
        sql: "
            CREATE TABLE IF NOT EXISTS jobs (
                id TEXT PRIMARY KEY,
                job_type TEXT NOT NULL,
                status TEXT NOT NULL,
                priority INTEGER NOT NULL DEFAULT 0,
                idempotency_key TEXT UNIQUE,
                project_id TEXT REFERENCES projects(id) ON DELETE SET NULL,
                payload TEXT NOT NULL,
                result TEXT,
                error TEXT,
                retry_count INTEGER NOT NULL DEFAULT 0,
                max_retries INTEGER NOT NULL DEFAULT 3,
                leased_by TEXT,
                lease_expires_at TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                started_at TEXT,
                completed_at TEXT
            );

            CREATE INDEX IF NOT EXISTS idx_jobs_status_priority ON jobs(status, priority DESC, created_at ASC);
            CREATE INDEX IF NOT EXISTS idx_jobs_project_id ON jobs(project_id);
            CREATE INDEX IF NOT EXISTS idx_jobs_idempotency_key ON jobs(idempotency_key);

            CREATE TABLE IF NOT EXISTS job_checkpoints (
                id TEXT PRIMARY KEY,
                job_id TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
                stage TEXT NOT NULL,
                checkpoint_data TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_job_checkpoints_job_id ON job_checkpoints(job_id, created_at ASC);

            CREATE TABLE IF NOT EXISTS domain_events (
                id TEXT PRIMARY KEY,
                event_type TEXT NOT NULL,
                version INTEGER NOT NULL DEFAULT 1,
                project_id TEXT,
                job_id TEXT,
                payload TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_domain_events_type_created ON domain_events(event_type, created_at DESC);
            CREATE INDEX IF NOT EXISTS idx_domain_events_job_id ON domain_events(job_id);
        ",
    },
    Migration {
        version: 4,
        name: "jobs_durability_and_concurrency_audit",
        sql: "
            ALTER TABLE jobs ADD COLUMN payload_version INTEGER NOT NULL DEFAULT 1;
            ALTER TABLE jobs ADD COLUMN lease_token TEXT;

            -- Deterministic reconciliation for legacy duplicate idempotency keys:
            -- The earliest job (MIN rowid) retains the original idempotency_key.
            -- Subsequent duplicate rows are disambiguated as '<key>#dup_<id>',
            -- preserving 100% of historical job rows with zero data loss.
            UPDATE jobs 
            SET idempotency_key = idempotency_key || '#dup_' || id 
            WHERE rowid NOT IN (
                SELECT MIN(rowid) 
                FROM jobs 
                WHERE idempotency_key IS NOT NULL 
                GROUP BY idempotency_key
            )
            AND idempotency_key IS NOT NULL;

            DROP INDEX IF EXISTS idx_jobs_idempotency_key;
            CREATE UNIQUE INDEX IF NOT EXISTS idx_jobs_idempotency_key ON jobs(idempotency_key) WHERE idempotency_key IS NOT NULL;
        ",
    },
    Migration {
        version: 5,
        name: "media_and_timeline_core",
        sql: "
            CREATE TABLE IF NOT EXISTS timelines (
                id TEXT PRIMARY KEY,
                project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                name TEXT NOT NULL,
                revision INTEGER NOT NULL DEFAULT 1,
                width INTEGER NOT NULL,
                height INTEGER NOT NULL,
                frame_rate REAL NOT NULL,
                history_cursor INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS timeline_tracks (
                id TEXT PRIMARY KEY,
                timeline_id TEXT NOT NULL REFERENCES timelines(id) ON DELETE CASCADE,
                kind TEXT NOT NULL,
                order_index INTEGER NOT NULL,
                name TEXT NOT NULL,
                muted INTEGER NOT NULL DEFAULT 0,
                UNIQUE(timeline_id, order_index)
            );

            CREATE TABLE IF NOT EXISTS timeline_clips (
                id TEXT PRIMARY KEY,
                track_id TEXT NOT NULL REFERENCES timeline_tracks(id) ON DELETE CASCADE,
                artifact_id TEXT NOT NULL REFERENCES artifacts(id),
                source_in_ms INTEGER NOT NULL,
                source_out_ms INTEGER NOT NULL,
                timeline_start_ms INTEGER NOT NULL,
                volume REAL NOT NULL DEFAULT 1.0,
                muted INTEGER NOT NULL DEFAULT 0,
                position_x REAL NOT NULL DEFAULT 0.0,
                position_y REAL NOT NULL DEFAULT 0.0,
                scale REAL NOT NULL DEFAULT 1.0,
                rotation_deg REAL NOT NULL DEFAULT 0.0,
                opacity REAL NOT NULL DEFAULT 1.0
            );

            CREATE TABLE IF NOT EXISTS timeline_history (
                timeline_id TEXT NOT NULL REFERENCES timelines(id) ON DELETE CASCADE,
                history_index INTEGER NOT NULL,
                snapshot_json TEXT NOT NULL,
                operation_type TEXT NOT NULL,
                created_at TEXT NOT NULL,
                PRIMARY KEY(timeline_id, history_index)
            );

            CREATE INDEX IF NOT EXISTS idx_timelines_project_id ON timelines(project_id);
            CREATE INDEX IF NOT EXISTS idx_timeline_tracks_timeline_order ON timeline_tracks(timeline_id, order_index);
            CREATE INDEX IF NOT EXISTS idx_timeline_clips_track_start ON timeline_clips(track_id, timeline_start_ms);
            CREATE INDEX IF NOT EXISTS idx_timeline_clips_artifact_id ON timeline_clips(artifact_id);
            CREATE INDEX IF NOT EXISTS idx_timeline_history_timeline_index ON timeline_history(timeline_id, history_index);
        ",
    },
];

pub fn run_all_migrations(conn: &mut Connection) -> Result<(), rusqlite::Error> {
    // Ensure migrations tracking table exists
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS _migrations (
            id INTEGER PRIMARY KEY,
            version INTEGER UNIQUE NOT NULL,
            name TEXT NOT NULL,
            applied_at TEXT NOT NULL
        );",
    )?;

    // Fetch applied migrations
    let applied_versions: HashSet<i32> = {
        let mut stmt = conn.prepare("SELECT version FROM _migrations")?;
        let rows = stmt.query_map([], |row| row.get(0))?;
        let mut versions = HashSet::new();
        for row in rows {
            versions.insert(row?);
        }
        versions
    };

    // Apply pending migrations inside a transaction
    for migration in MIGRATIONS {
        if !applied_versions.contains(&migration.version) {
            let tx = conn.transaction()?;
            apply_migration(&tx, migration)?;
            tx.commit()?;
            tracing::info!(
                version = migration.version,
                name = migration.name,
                "Applied database migration"
            );
        }
    }

    Ok(())
}

fn apply_migration(tx: &Transaction, migration: &Migration) -> Result<(), rusqlite::Error> {
    tx.execute_batch(migration.sql)?;
    tx.execute(
        "INSERT INTO _migrations (version, name, applied_at) VALUES (?1, ?2, datetime('now'))",
        rusqlite::params![migration.version, migration.name],
    )?;
    Ok(())
}

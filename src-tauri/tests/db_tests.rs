use openvideoai_lib::db::DbManager;
use tempfile::tempdir;

#[test]
fn test_db_initialization_and_wal_mode() {
    let dir = tempdir().expect("failed to create tempdir");
    let db_path = dir.path().join("test_openvideoai.db");

    let db_manager = DbManager::new(db_path.clone()).expect("failed to initialize db");

    // Verify journal mode is WAL
    let journal_mode = db_manager
        .get_journal_mode()
        .expect("failed to get journal_mode");
    assert_eq!(journal_mode.to_uppercase(), "WAL");
}

#[test]
fn test_db_migrations_are_idempotent() {
    let dir = tempdir().expect("failed to create tempdir");
    let db_path = dir.path().join("test_openvideoai.db");

    let db_manager = DbManager::new(db_path).expect("failed to initialize db");

    // Initial run of migrations
    let res1 = db_manager.run_migrations();
    assert!(res1.is_ok(), "First migration run failed: {:?}", res1.err());

    let count1 = db_manager
        .get_migration_count()
        .expect("failed to count migrations");
    assert!(count1 > 0, "Expected migrations to have been applied");

    // Second run must be completely idempotent
    let res2 = db_manager.run_migrations();
    assert!(
        res2.is_ok(),
        "Second migration run failed: {:?}",
        res2.err()
    );

    let count2 = db_manager
        .get_migration_count()
        .expect("failed to count migrations");
    assert_eq!(count1, count2, "Migration count changed on second run");
}

#[test]
fn test_db_health_query() {
    let dir = tempdir().expect("failed to create tempdir");
    let db_path = dir.path().join("test_openvideoai.db");

    let db_manager = DbManager::new(db_path.clone()).expect("failed to initialize db");
    db_manager.run_migrations().expect("migrations failed");

    let health = db_manager.check_health().expect("health check failed");
    assert!(health.ok, "Expected db health ok to be true");
    assert_eq!(health.path, db_path.to_string_lossy().to_string());
}

#[test]
fn test_migration_3_schema_and_foreign_keys() {
    let dir = tempdir().expect("failed to create tempdir");
    let db_path = dir.path().join("test_migration_3.db");

    let db_manager = DbManager::new(db_path).expect("failed to initialize db");
    db_manager.run_migrations().expect("migrations failed");

    assert_eq!(
        db_manager.get_migration_count().expect("migration count"),
        5
    );

    let conn = db_manager.conn().expect("conn failed");

    // Test insert job with idempotency key
    conn.execute(
        "INSERT INTO jobs (id, job_type, status, priority, idempotency_key, payload, created_at, updated_at)
         VALUES ('job-1', 'test_task', 'queued', 10, 'key-123', '{\"test\":true}', datetime('now'), datetime('now'));",
        [],
    ).expect("insert job failed");

    // Duplicate idempotency key must fail
    let duplicate_res = conn.execute(
        "INSERT INTO jobs (id, job_type, status, priority, idempotency_key, payload, created_at, updated_at)
         VALUES ('job-2', 'test_task', 'queued', 5, 'key-123', '{\"test\":false}', datetime('now'), datetime('now'));",
        [],
    );
    assert!(
        duplicate_res.is_err(),
        "Duplicate idempotency key should violate UNIQUE constraint"
    );

    // Insert checkpoint
    conn.execute(
        "INSERT INTO job_checkpoints (id, job_id, stage, checkpoint_data, created_at)
         VALUES ('cp-1', 'job-1', 'extraction', '{\"progress\":50}', datetime('now'));",
        [],
    )
    .expect("insert checkpoint failed");

    // Checkpoint with non-existent job must fail foreign key
    let invalid_cp = conn.execute(
        "INSERT INTO job_checkpoints (id, job_id, stage, checkpoint_data, created_at)
         VALUES ('cp-2', 'non-existent-job', 'stage1', '{}', datetime('now'));",
        [],
    );
    assert!(
        invalid_cp.is_err(),
        "Foreign key constraint on job_id must be enforced"
    );

    // Insert domain event
    conn.execute(
        "INSERT INTO domain_events (id, event_type, version, job_id, payload, created_at)
         VALUES ('evt-1', 'JobCreated', 1, 'job-1', '{\"name\":\"test\"}', datetime('now'));",
        [],
    )
    .expect("insert domain event failed");
}

#[test]
fn test_migration_3_to_4_upgrade_regression() {
    let dir = tempdir().expect("failed to create tempdir");
    let db_path = dir.path().join("pre_audit_legacy.db");

    // 1. Manually fixture a pre-audit database applied up to migration 3
    {
        let raw_conn = rusqlite::Connection::open(&db_path).expect("open raw db");
        raw_conn
            .execute_batch(
                "CREATE TABLE _migrations (
                    id INTEGER PRIMARY KEY,
                    version INTEGER UNIQUE NOT NULL,
                    name TEXT NOT NULL,
                    applied_at TEXT NOT NULL
                );
                INSERT INTO _migrations (version, name, applied_at) VALUES (1, 'initial_app_metadata', datetime('now'));
                INSERT INTO _migrations (version, name, applied_at) VALUES (2, 'project_and_artifact_core', datetime('now'));
                INSERT INTO _migrations (version, name, applied_at) VALUES (3, 'durable_jobs_and_events', datetime('now'));

                -- Pre-audit migration 3 jobs table (NO payload_version, NO lease_token)
                CREATE TABLE jobs (
                    id TEXT PRIMARY KEY,
                    job_type TEXT NOT NULL,
                    status TEXT NOT NULL,
                    priority INTEGER NOT NULL DEFAULT 0,
                    idempotency_key TEXT UNIQUE,
                    project_id TEXT,
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

                CREATE INDEX idx_jobs_status_priority ON jobs(status, priority DESC, created_at ASC);
                CREATE INDEX idx_jobs_project_id ON jobs(project_id);
                CREATE INDEX idx_jobs_idempotency_key ON jobs(idempotency_key);

                CREATE TABLE job_checkpoints (
                    id TEXT PRIMARY KEY,
                    job_id TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
                    stage TEXT NOT NULL,
                    checkpoint_data TEXT NOT NULL,
                    created_at TEXT NOT NULL
                );

                CREATE TABLE domain_events (
                    id TEXT PRIMARY KEY,
                    event_type TEXT NOT NULL,
                    version INTEGER NOT NULL DEFAULT 1,
                    project_id TEXT,
                    job_id TEXT,
                    payload TEXT NOT NULL,
                    created_at TEXT NOT NULL
                );

                -- 2. Insert existing row into legacy jobs table to verify data preservation
                INSERT INTO jobs (id, job_type, status, priority, idempotency_key, payload, created_at, updated_at)
                VALUES ('job-legacy-1', 'transcode', 'queued', 10, 'legacy-key-999', '{\"source\":\"old.mp4\"}', '2026-09-09T10:00:00Z', '2026-09-09T10:00:00Z');
                ",
            )
            .expect("fixture pre-audit db");
    }

    // 3. Run current migration engine on the pre-audit database
    let db_manager = DbManager::new(db_path).expect("init db manager");
    db_manager
        .run_migrations()
        .expect("upgrade migrations to 4 must succeed");

    assert_eq!(
        db_manager.get_migration_count().expect("migration count"),
        5,
        "Expected all 5 migrations applied after upgrade"
    );

    let conn = db_manager.conn().expect("get conn");

    // 4. Verify existing legacy row was preserved and received default payload_version = 1
    let mut stmt = conn
        .prepare("SELECT id, job_type, payload, payload_version, lease_token, idempotency_key FROM jobs WHERE id = 'job-legacy-1'")
        .expect("prepare select");
    let (id, job_type, payload, payload_version, lease_token, idempotency_key): (
        String,
        String,
        String,
        i32,
        Option<String>,
        Option<String>,
    ) = stmt
        .query_row([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
            ))
        })
        .expect("query existing row");

    assert_eq!(id, "job-legacy-1");
    assert_eq!(job_type, "transcode");
    assert_eq!(payload, "{\"source\":\"old.mp4\"}");
    assert_eq!(
        payload_version, 1,
        "Existing row must receive default payload_version = 1"
    );
    assert!(
        lease_token.is_none(),
        "lease_token should default to NULL on existing row"
    );
    assert_eq!(idempotency_key.as_deref(), Some("legacy-key-999"));

    // 5. Verify lease_token column support exists and can be updated
    conn.execute(
        "UPDATE jobs SET lease_token = 'token-uuid-456' WHERE id = 'job-legacy-1'",
        [],
    )
    .expect("update lease_token");
    let updated_token: String = conn
        .query_row(
            "SELECT lease_token FROM jobs WHERE id = 'job-legacy-1'",
            [],
            |row| row.get(0),
        )
        .expect("select updated token");
    assert_eq!(updated_token, "token-uuid-456");

    // 6. Verify the partial unique idempotency index exists and enforces uniqueness
    let index_sql: String = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'index' AND name = 'idx_jobs_idempotency_key'",
            [],
            |row| row.get(0),
        )
        .expect("select index sql");
    assert!(
        index_sql.contains("WHERE idempotency_key IS NOT NULL"),
        "Index must be a partial unique index on non-null idempotency_key: {}",
        index_sql
    );

    // Duplicate non-null key must fail
    let dup_res = conn.execute(
        "INSERT INTO jobs (id, job_type, status, priority, idempotency_key, payload, created_at, updated_at)
         VALUES ('job-new-dup', 'transcode', 'queued', 5, 'legacy-key-999', '{}', datetime('now'), datetime('now'));",
        [],
    );
    assert!(
        dup_res.is_err(),
        "Duplicate non-null idempotency key must fail unique index"
    );

    // Multiple NULL idempotency keys must succeed
    conn.execute(
        "INSERT INTO jobs (id, job_type, status, priority, idempotency_key, payload, created_at, updated_at)
         VALUES ('job-null-1', 'transcode', 'queued', 0, NULL, '{}', datetime('now'), datetime('now'));",
        [],
    )
    .expect("insert first null key job");

    conn.execute(
        "INSERT INTO jobs (id, job_type, status, priority, idempotency_key, payload, created_at, updated_at)
         VALUES ('job-null-2', 'transcode', 'queued', 0, NULL, '{}', datetime('now'), datetime('now'));",
        [],
    )
    .expect("insert second null key job");
}

#[test]
fn test_migration_4_reconciles_legacy_duplicate_idempotency_keys() {
    let dir = tempdir().expect("failed to create tempdir");
    let db_path = dir.path().join("pre_audit_duplicates.db");

    // 1. Construct a pre-audit Migration-3 fixture containing duplicate non-null idempotency keys
    {
        let raw_conn = rusqlite::Connection::open(&db_path).expect("open raw db");
        raw_conn
            .execute_batch(
                "CREATE TABLE _migrations (
                    id INTEGER PRIMARY KEY,
                    version INTEGER UNIQUE NOT NULL,
                    name TEXT NOT NULL,
                    applied_at TEXT NOT NULL
                );
                INSERT INTO _migrations (version, name, applied_at) VALUES (1, 'initial_app_metadata', datetime('now'));
                INSERT INTO _migrations (version, name, applied_at) VALUES (2, 'project_and_artifact_core', datetime('now'));
                INSERT INTO _migrations (version, name, applied_at) VALUES (3, 'durable_jobs_and_events', datetime('now'));

                -- Migration 3 jobs table without unique constraint on idempotency_key to simulate legacy duplicate data
                CREATE TABLE jobs (
                    id TEXT PRIMARY KEY,
                    job_type TEXT NOT NULL,
                    status TEXT NOT NULL,
                    priority INTEGER NOT NULL DEFAULT 0,
                    idempotency_key TEXT,
                    project_id TEXT,
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

                CREATE INDEX idx_jobs_idempotency_key ON jobs(idempotency_key);

                CREATE TABLE job_checkpoints (
                    id TEXT PRIMARY KEY,
                    job_id TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
                    stage TEXT NOT NULL,
                    checkpoint_data TEXT NOT NULL,
                    created_at TEXT NOT NULL
                );

                CREATE TABLE domain_events (
                    id TEXT PRIMARY KEY,
                    event_type TEXT NOT NULL,
                    version INTEGER NOT NULL DEFAULT 1,
                    project_id TEXT,
                    job_id TEXT,
                    payload TEXT NOT NULL,
                    created_at TEXT NOT NULL
                );

                -- Insert TWO jobs with the exact same non-null idempotency_key
                INSERT INTO jobs (id, job_type, status, priority, idempotency_key, payload, created_at, updated_at)
                VALUES ('job-dup-first', 'encode', 'completed', 10, 'shared-idemp-key', '{\"v\":1}', '2026-09-08T10:00:00Z', '2026-09-08T10:00:00Z');

                INSERT INTO jobs (id, job_type, status, priority, idempotency_key, payload, created_at, updated_at)
                VALUES ('job-dup-second', 'encode', 'completed', 10, 'shared-idemp-key', '{\"v\":2}', '2026-09-08T10:05:00Z', '2026-09-08T10:05:00Z');

                -- Insert a third job with a unique idempotency key
                INSERT INTO jobs (id, job_type, status, priority, idempotency_key, payload, created_at, updated_at)
                VALUES ('job-unique', 'transcribe', 'queued', 5, 'unique-idemp-key', '{\"v\":3}', '2026-09-08T10:10:00Z', '2026-09-08T10:10:00Z');
                ",
            )
            .expect("fixture legacy db with duplicates");
    }

    // 2. Run current migration engine (applying Migration 4)
    let db_manager = DbManager::new(db_path).expect("init db manager");
    db_manager
        .run_migrations()
        .expect("Migration 4 must succeed even with legacy duplicate idempotency keys");

    let conn = db_manager.conn().expect("get conn");

    // 3. Verify ALL 3 jobs are preserved (zero data loss)
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM jobs", [], |row| row.get(0))
        .expect("count jobs");
    assert_eq!(count, 3, "All 3 historical jobs must be preserved");

    // 4. Verify the earliest job preserved the original idempotency key
    let key_first: String = conn
        .query_row(
            "SELECT idempotency_key FROM jobs WHERE id = 'job-dup-first'",
            [],
            |row| row.get(0),
        )
        .expect("get key first");
    assert_eq!(key_first, "shared-idemp-key");

    // 5. Verify the duplicate job's key was deterministically disambiguated
    let key_second: String = conn
        .query_row(
            "SELECT idempotency_key FROM jobs WHERE id = 'job-dup-second'",
            [],
            |row| row.get(0),
        )
        .expect("get key second");
    assert_eq!(key_second, "shared-idemp-key#dup_job-dup-second");

    // 6. Verify the unique job's key is unchanged
    let key_unique: String = conn
        .query_row(
            "SELECT idempotency_key FROM jobs WHERE id = 'job-unique'",
            [],
            |row| row.get(0),
        )
        .expect("get key unique");
    assert_eq!(key_unique, "unique-idemp-key");

    // 7. Verify the partial unique index exists and enforces uniqueness on new inserts
    let dup_res = conn.execute(
        "INSERT INTO jobs (id, job_type, status, priority, idempotency_key, payload, created_at, updated_at)
         VALUES ('job-new-conflict', 'encode', 'queued', 0, 'shared-idemp-key', '{}', datetime('now'), datetime('now'));",
        [],
    );
    assert!(
        dup_res.is_err(),
        "New insert with existing non-null idempotency_key must fail"
    );
}

#[test]
fn test_migration_5_applies_cleanly_over_migration_4() {
    let dir = tempdir().expect("failed to create tempdir");
    let db_path = dir.path().join("test_migration_5.db");

    let db_manager = DbManager::new(db_path).expect("failed to initialize db");
    db_manager.run_migrations().expect("migrations failed");

    assert_eq!(
        db_manager.get_migration_count().expect("migration count"),
        5,
        "Expected all 5 migrations to be applied"
    );

    let conn = db_manager.conn().expect("conn failed");

    // Verify all Phase 3 tables exist
    let tables: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name;")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();

    assert!(tables.contains(&"timelines".to_string()));
    assert!(tables.contains(&"timeline_tracks".to_string()));
    assert!(tables.contains(&"timeline_clips".to_string()));
    assert!(tables.contains(&"timeline_history".to_string()));

    // Verify foreign keys and cascade rules
    conn.execute(
        "INSERT INTO projects (id, name, created_at, updated_at) VALUES ('p1', 'Test Project', datetime('now'), datetime('now'));",
        [],
    ).unwrap();

    conn.execute(
        "INSERT INTO artifacts (id, sha256, kind, mime_type, size_bytes, storage_key, original_filename, created_at)
         VALUES ('art-1', 'sha256-test', 'video', 'video/mp4', 1024, 'art-1.mp4', 'test.mp4', datetime('now'));",
        [],
    ).unwrap();

    conn.execute(
        "INSERT INTO timelines (id, project_id, name, revision, width, height, frame_rate, history_cursor, created_at, updated_at)
         VALUES ('t1', 'p1', 'Main Timeline', 1, 1920, 1080, 30.0, 0, datetime('now'), datetime('now'));",
        [],
    ).unwrap();

    conn.execute(
        "INSERT INTO timeline_tracks (id, timeline_id, kind, order_index, name, muted)
         VALUES ('tr1', 't1', 'video', 0, 'V1', 0);",
        [],
    )
    .unwrap();

    conn.execute(
        "INSERT INTO timeline_clips (id, track_id, artifact_id, source_in_ms, source_out_ms, timeline_start_ms, volume, muted, position_x, position_y, scale, rotation_deg, opacity)
         VALUES ('cl1', 'tr1', 'art-1', 0, 1000, 0, 1.0, 0, 0.0, 0.0, 1.0, 0.0, 1.0);",
        [],
    ).unwrap();

    conn.execute(
        "INSERT INTO timeline_history (timeline_id, history_index, snapshot_json, operation_type, created_at)
         VALUES ('t1', 0, '{\"revision\":1}', 'init', datetime('now'));",
        [],
    ).unwrap();

    // Track order uniqueness per timeline
    let duplicate_order = conn.execute(
        "INSERT INTO timeline_tracks (id, timeline_id, kind, order_index, name, muted)
         VALUES ('tr2', 't1', 'video', 0, 'V2', 0);",
        [],
    );
    assert!(
        duplicate_order.is_err(),
        "Duplicate track order on same timeline must be rejected"
    );

    // History primary key uniqueness (timeline_id, history_index)
    let duplicate_history = conn.execute(
        "INSERT INTO timeline_history (timeline_id, history_index, snapshot_json, operation_type, created_at)
         VALUES ('t1', 0, '{\"revision\":2}', 'mutation', datetime('now'));",
        [],
    );
    assert!(
        duplicate_history.is_err(),
        "Duplicate history_index on same timeline must be rejected"
    );

    // Cascading delete: project deletion cascades to timeline, tracks, clips, and history
    conn.execute("DELETE FROM projects WHERE id = 'p1';", [])
        .unwrap();
    let count: i64 = conn
        .query_row("SELECT count(*) FROM timelines WHERE id = 't1';", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(count, 0);
    let track_count: i64 = conn
        .query_row(
            "SELECT count(*) FROM timeline_tracks WHERE id = 'tr1';",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(track_count, 0);
    let clip_count: i64 = conn
        .query_row(
            "SELECT count(*) FROM timeline_clips WHERE id = 'cl1';",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(clip_count, 0);
    let hist_count: i64 = conn
        .query_row(
            "SELECT count(*) FROM timeline_history WHERE timeline_id = 't1';",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(hist_count, 0);
}

#[test]
fn test_migration_4_to_5_upgrade_regression() {
    let dir = tempdir().expect("failed to create tempdir");
    let db_path = dir.path().join("pre_migration_5_upgrade.db");

    // 1. Fixture a real v0.3.0 / Migration-4 database
    {
        let raw_conn = rusqlite::Connection::open(&db_path).expect("open raw db");
        raw_conn.execute_batch(
            "PRAGMA foreign_keys = ON;

             CREATE TABLE _migrations (
                 id INTEGER PRIMARY KEY,
                 version INTEGER UNIQUE NOT NULL,
                 name TEXT NOT NULL,
                 applied_at TEXT NOT NULL
             );
             INSERT INTO _migrations (version, name, applied_at) VALUES (1, 'initial_foundation_schema', datetime('now'));
             INSERT INTO _migrations (version, name, applied_at) VALUES (2, 'project_and_artifact_core', datetime('now'));
             INSERT INTO _migrations (version, name, applied_at) VALUES (3, 'durable_jobs_and_events', datetime('now'));
             INSERT INTO _migrations (version, name, applied_at) VALUES (4, 'jobs_durability_and_concurrency_audit', datetime('now'));

             CREATE TABLE app_metadata (
                 key TEXT PRIMARY KEY,
                 value TEXT NOT NULL
             );
             INSERT INTO app_metadata (key, value) VALUES ('initialized_at', '2026-09-08T10:00:00Z');

             CREATE TABLE projects (
                 id TEXT PRIMARY KEY,
                 name TEXT NOT NULL,
                 description TEXT,
                 created_at TEXT NOT NULL,
                 updated_at TEXT NOT NULL
             );
             INSERT INTO projects (id, name, description, created_at, updated_at)
             VALUES ('proj-upgrade-1', 'Existing Project', 'v0.3.0 data', '2026-09-08T10:00:00Z', '2026-09-08T10:00:00Z');

             CREATE TABLE artifacts (
                 id TEXT PRIMARY KEY,
                 sha256 TEXT NOT NULL UNIQUE,
                 kind TEXT NOT NULL,
                 mime_type TEXT,
                 size_bytes INTEGER NOT NULL,
                 storage_key TEXT NOT NULL UNIQUE,
                 original_filename TEXT NOT NULL,
                 created_at TEXT NOT NULL
             );
             INSERT INTO artifacts (id, sha256, kind, mime_type, size_bytes, storage_key, original_filename, created_at)
             VALUES ('art-upgrade-1', 'sha256-existing-hash', 'video', 'video/mp4', 2048, 'existing.mp4', 'existing.mp4', '2026-09-08T10:00:00Z');

             CREATE TABLE artifact_media_metadata (
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
             INSERT INTO artifact_media_metadata (artifact_id, probe_status, duration_ms, width, height, video_codec, audio_codec, frame_rate, sample_rate, channels)
             VALUES ('art-upgrade-1', 'ready', 5000, 1920, 1080, 'h264', 'aac', 30.0, 48000, 2);

             CREATE TABLE project_artifacts (
                 project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                 artifact_id TEXT NOT NULL REFERENCES artifacts(id) ON DELETE CASCADE,
                 added_at TEXT NOT NULL,
                 PRIMARY KEY(project_id, artifact_id)
             );
             INSERT INTO project_artifacts (project_id, artifact_id, added_at)
             VALUES ('proj-upgrade-1', 'art-upgrade-1', '2026-09-08T10:00:00Z');

             CREATE TABLE jobs (
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
                 completed_at TEXT,
                 payload_version INTEGER NOT NULL DEFAULT 1,
                 lease_token TEXT
             );
             CREATE UNIQUE INDEX idx_jobs_idempotency_key ON jobs(idempotency_key) WHERE idempotency_key IS NOT NULL;
             INSERT INTO jobs (id, job_type, status, priority, idempotency_key, project_id, payload, payload_version, lease_token, created_at, updated_at)
             VALUES ('job-upgrade-1', 'video_render', 'queued', 10, 'key-upgrade-1', 'proj-upgrade-1', '{\"v\":1}', 1, 'token-init-1', '2026-09-08T10:00:00Z', '2026-09-08T10:00:00Z');

             CREATE TABLE job_checkpoints (
                 id TEXT PRIMARY KEY,
                 job_id TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
                 stage TEXT NOT NULL,
                 checkpoint_data TEXT NOT NULL,
                 created_at TEXT NOT NULL
             );
             INSERT INTO job_checkpoints (id, job_id, stage, checkpoint_data, created_at)
             VALUES ('cp-upgrade-1', 'job-upgrade-1', 'init', '{\"pct\":0}', '2026-09-08T10:00:00Z');

             CREATE TABLE domain_events (
                 id TEXT PRIMARY KEY,
                 event_type TEXT NOT NULL,
                 version INTEGER NOT NULL DEFAULT 1,
                 project_id TEXT,
                 job_id TEXT,
                 payload TEXT NOT NULL,
                 created_at TEXT NOT NULL
             );
             INSERT INTO domain_events (id, event_type, version, project_id, job_id, payload, created_at)
             VALUES ('ev-upgrade-1', 'job_enqueued', 1, 'proj-upgrade-1', 'job-upgrade-1', '{\"status\":\"queued\"}', '2026-09-08T10:00:00Z');
            ",
        ).expect("fixture v0.3.0 Migration-4 database");
    }

    // 2. Run current migration engine on pre-existing database
    let db_manager = DbManager::new(db_path).expect("initialize DbManager over existing db");
    db_manager
        .run_migrations()
        .expect("Migration 5 must apply cleanly over Migration 4");

    // 3. Verify migration count is now 5
    assert_eq!(
        db_manager.get_migration_count().expect("migration count"),
        5,
        "Migration count must be exactly 5 after upgrade"
    );

    let conn = db_manager.conn().expect("get conn");

    // 4. Verify ZERO data loss on existing v0.3.0 entities
    let proj_name: String = conn
        .query_row(
            "SELECT name FROM projects WHERE id = 'proj-upgrade-1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(proj_name, "Existing Project");

    let art_key: String = conn
        .query_row(
            "SELECT storage_key FROM artifacts WHERE id = 'art-upgrade-1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(art_key, "existing.mp4");

    let meta_status: String = conn
        .query_row(
            "SELECT probe_status FROM artifact_media_metadata WHERE artifact_id = 'art-upgrade-1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(meta_status, "ready");

    let job_token: String = conn
        .query_row(
            "SELECT lease_token FROM jobs WHERE id = 'job-upgrade-1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(job_token, "token-init-1");

    let cp_stage: String = conn
        .query_row(
            "SELECT stage FROM job_checkpoints WHERE id = 'cp-upgrade-1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(cp_stage, "init");

    let ev_type: String = conn
        .query_row(
            "SELECT event_type FROM domain_events WHERE id = 'ev-upgrade-1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(ev_type, "job_enqueued");

    // 5. Verify Phase 3 tables are now present and can link to existing v0.3.0 data
    conn.execute(
        "INSERT INTO timelines (id, project_id, name, revision, width, height, frame_rate, history_cursor, created_at, updated_at)
         VALUES ('tl-new-1', 'proj-upgrade-1', 'Timeline for Existing Project', 1, 1920, 1080, 24.0, 0, datetime('now'), datetime('now'));",
        [],
    ).expect("insert timeline referencing existing project");

    conn.execute(
        "INSERT INTO timeline_tracks (id, timeline_id, kind, order_index, name, muted)
         VALUES ('tr-new-1', 'tl-new-1', 'video', 0, 'V1', 0);",
        [],
    )
    .expect("insert track");

    conn.execute(
        "INSERT INTO timeline_clips (id, track_id, artifact_id, source_in_ms, source_out_ms, timeline_start_ms, volume, muted, position_x, position_y, scale, rotation_deg, opacity)
         VALUES ('cl-new-1', 'tr-new-1', 'art-upgrade-1', 0, 3000, 0, 1.0, 0, 0.0, 0.0, 1.0, 0.0, 1.0);",
        [],
    ).expect("insert clip referencing existing artifact");
}

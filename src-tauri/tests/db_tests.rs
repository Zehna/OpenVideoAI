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
        4
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
        4,
        "Expected all 4 migrations applied after upgrade"
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

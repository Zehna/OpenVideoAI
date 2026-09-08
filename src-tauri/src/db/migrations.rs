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

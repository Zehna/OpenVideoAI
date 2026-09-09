use openvideoai_lib::db::DbManager;
use openvideoai_lib::projects::service::ProjectService;
use std::sync::Arc;
use tempfile::tempdir;

#[test]
fn test_create_and_get_project() {
    let dir = tempdir().expect("failed to create tempdir");
    let db_path = dir.path().join("projects_test.db");
    let db = Arc::new(DbManager::new(db_path).expect("failed to init db"));
    db.run_migrations().expect("failed to run migrations");

    let service = ProjectService::new(db);
    let project = service
        .create_project(
            "Project Alpha".to_string(),
            Some("A test video project".to_string()),
        )
        .expect("failed to create project");

    assert!(!project.id.is_empty());
    assert_eq!(project.name, "Project Alpha");
    assert_eq!(project.description.as_deref(), Some("A test video project"));
    assert!(!project.created_at.is_empty());
    assert_eq!(project.created_at, project.updated_at);

    let retrieved = service
        .get_project(&project.id)
        .expect("failed to get project");
    assert_eq!(retrieved.id, project.id);
    assert_eq!(retrieved.name, "Project Alpha");
}

#[test]
fn test_list_projects() {
    let dir = tempdir().expect("failed to create tempdir");
    let db_path = dir.path().join("list_test.db");
    let db = Arc::new(DbManager::new(db_path).expect("failed to init db"));
    db.run_migrations().expect("failed to run migrations");

    let service = ProjectService::new(db);
    assert!(service.list_projects().expect("failed to list").is_empty());

    service
        .create_project("P1".to_string(), None)
        .expect("create P1 failed");
    service
        .create_project("P2".to_string(), Some("Desc".to_string()))
        .expect("create P2 failed");

    let list = service.list_projects().expect("failed to list");
    assert_eq!(list.len(), 2);
}

#[test]
fn test_update_project() {
    let dir = tempdir().expect("failed to create tempdir");
    let db_path = dir.path().join("update_test.db");
    let db = Arc::new(DbManager::new(db_path).expect("failed to init db"));
    db.run_migrations().expect("failed to run migrations");

    let service = ProjectService::new(db);
    let created = service
        .create_project("Original".to_string(), None)
        .expect("create failed");

    let updated = service
        .update_project(
            &created.id,
            "Renamed".to_string(),
            Some("New description".to_string()),
        )
        .expect("update failed");

    assert_eq!(updated.id, created.id);
    assert_eq!(updated.name, "Renamed");
    assert_eq!(updated.description.as_deref(), Some("New description"));

    let retrieved = service.get_project(&created.id).expect("get failed");
    assert_eq!(retrieved.name, "Renamed");
    assert_eq!(retrieved.description.as_deref(), Some("New description"));
}

#[test]
fn test_project_persists_after_db_reopen() {
    let dir = tempdir().expect("failed to create tempdir");
    let db_path = dir.path().join("reopen_test.db");

    let created_id = {
        let db = Arc::new(DbManager::new(db_path.clone()).expect("failed to init db"));
        db.run_migrations().expect("failed to run migrations");
        let service = ProjectService::new(db);
        let project = service
            .create_project("Persistent Project".to_string(), None)
            .expect("create failed");
        project.id
    };

    // Reopen database connection
    let db2 = Arc::new(DbManager::new(db_path).expect("failed to reopen db"));
    let service2 = ProjectService::new(db2);
    let project = service2
        .get_project(&created_id)
        .expect("get failed after reopen");
    assert_eq!(project.name, "Persistent Project");
}

#[test]
fn test_migration_2_is_idempotent() {
    let dir = tempdir().expect("failed to create tempdir");
    let db_path = dir.path().join("migration2_test.db");
    let db = Arc::new(DbManager::new(db_path).expect("failed to init db"));

    db.run_migrations().expect("first migration run failed");
    let count1 = db.get_migration_count().expect("count failed");
    assert!(count1 >= 2);

    // Second run must succeed with zero side effects
    db.run_migrations().expect("second migration run failed");
    let count2 = db.get_migration_count().expect("count failed");
    assert_eq!(count1, count2);
}

#[test]
fn test_project_foreign_key_constraints() {
    let dir = tempdir().expect("failed to create tempdir");
    let db_path = dir.path().join("fk_test.db");
    let db = Arc::new(DbManager::new(db_path).expect("failed to init db"));
    db.run_migrations().expect("failed to run migrations");

    let conn = db.conn().expect("conn failed");

    // Inserting into project_artifacts with non-existent project_id must fail due to FK
    let res = conn.execute(
        "INSERT INTO project_artifacts (project_id, artifact_id, added_at) VALUES ('non-existent', 'artifact-1', '2026-09-08T00:00:00Z');",
        [],
    );
    assert!(res.is_err(), "Foreign key constraint was not enforced!");
}

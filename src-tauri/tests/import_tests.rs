use openvideoai_lib::artifacts::importer::ArtifactImporter;
use openvideoai_lib::artifacts::resolver::ArtifactResolver;
use openvideoai_lib::db::DbManager;
use openvideoai_lib::projects::service::ProjectService;
use openvideoai_lib::storage::local_store::LocalStore;
use std::fs;
use std::sync::Arc;
use tempfile::tempdir;

#[test]
fn test_first_import_creates_canonical_artifact() {
    let dir = tempdir().expect("failed to create tempdir");
    let db_path = dir.path().join("import_test.db");
    let db = Arc::new(DbManager::new(db_path).expect("failed to init db"));
    db.run_migrations().expect("failed to run migrations");

    let staging_dir = dir.path().join("staging");
    let artifacts_dir = dir.path().join("artifacts");
    let store = Arc::new(LocalStore::new(artifacts_dir.clone()));

    let project_service = ProjectService::new(db.clone());
    let project = project_service
        .create_project("Test Project".to_string(), None)
        .expect("create project failed");

    let sample_video = dir.path().join("sample_clip.mp4");
    fs::write(&sample_video, b"sample mp4 binary data for testing").expect("write failed");

    let importer = ArtifactImporter::new(db.clone(), store.clone(), staging_dir);
    let imported = importer
        .import(&project.id, &sample_video)
        .expect("import failed");

    assert!(!imported.artifact.artifact_id.is_empty());
    assert_eq!(imported.artifact.kind.as_str(), "video");
    assert_eq!(imported.original_filename, "sample_clip.mp4");
    assert!(!imported.is_deduplicated);
    assert_eq!(
        imported.artifact.uri,
        format!("artifact://{}", imported.artifact.artifact_id)
    );

    // Verify canonical file exists in store
    let canonical_path = store
        .resolve_path(&format!(
            "artifacts/{}/{}",
            &imported.artifact.sha256[..2],
            imported.artifact.sha256
        ))
        .expect("resolve failed");
    assert!(canonical_path.exists());
}

#[test]
fn test_duplicate_import_in_same_project_yields_one_link() {
    let dir = tempdir().expect("failed to create tempdir");
    let db_path = dir.path().join("dedup_test.db");
    let db = Arc::new(DbManager::new(db_path).expect("failed to init db"));
    db.run_migrations().expect("failed to run migrations");

    let staging_dir = dir.path().join("staging");
    let artifacts_dir = dir.path().join("artifacts");
    let store = Arc::new(LocalStore::new(artifacts_dir));

    let project_service = ProjectService::new(db.clone());
    let project = project_service
        .create_project("Project 1".to_string(), None)
        .expect("create project failed");

    let sample_video = dir.path().join("sample_clip.mp4");
    fs::write(&sample_video, b"identical video bytes").expect("write failed");

    let importer = ArtifactImporter::new(db.clone(), store, staging_dir);

    let first = importer
        .import(&project.id, &sample_video)
        .expect("first import failed");
    assert!(!first.is_deduplicated);

    let second = importer
        .import(&project.id, &sample_video)
        .expect("second import failed");
    assert!(second.is_deduplicated);
    assert_eq!(first.artifact.artifact_id, second.artifact.artifact_id);

    // List project artifacts: must contain exactly 1 entry, not 2
    let list = importer.list_for_project(&project.id).expect("list failed");
    assert_eq!(list.len(), 1);
}

#[test]
fn test_cross_project_import_reuses_same_artifact() {
    let dir = tempdir().expect("failed to create tempdir");
    let db_path = dir.path().join("cross_project_test.db");
    let db = Arc::new(DbManager::new(db_path).expect("failed to init db"));
    db.run_migrations().expect("failed to run migrations");

    let staging_dir = dir.path().join("staging");
    let artifacts_dir = dir.path().join("artifacts");
    let store = Arc::new(LocalStore::new(artifacts_dir.clone()));

    let project_service = ProjectService::new(db.clone());
    let p1 = project_service
        .create_project("P1".to_string(), None)
        .unwrap();
    let p2 = project_service
        .create_project("P2".to_string(), None)
        .unwrap();

    let sample_media = dir.path().join("shared_media.mp4");
    fs::write(&sample_media, b"shared media data content").expect("write failed");

    let importer = ArtifactImporter::new(db.clone(), store, staging_dir);

    let imp1 = importer
        .import(&p1.id, &sample_media)
        .expect("import 1 failed");
    assert!(!imp1.is_deduplicated);

    let imp2 = importer
        .import(&p2.id, &sample_media)
        .expect("import 2 failed");
    assert!(imp2.is_deduplicated);
    assert_eq!(imp1.artifact.artifact_id, imp2.artifact.artifact_id);
    assert_eq!(imp1.artifact.sha256, imp2.artifact.sha256);

    // Verify there is only one canonical file on disk
    let prefix_dir = artifacts_dir.join(&imp1.artifact.sha256[..2]);
    let files_in_dir = fs::read_dir(prefix_dir)
        .expect("read prefix dir failed")
        .count();
    assert_eq!(
        files_in_dir, 1,
        "Only one canonical file should exist on disk"
    );
}

#[test]
fn test_resolve_artifact_uri_to_canonical_path() {
    let dir = tempdir().expect("failed to create tempdir");
    let db_path = dir.path().join("resolver_test.db");
    let db = Arc::new(DbManager::new(db_path).expect("failed to init db"));
    db.run_migrations().expect("failed to run migrations");

    let staging_dir = dir.path().join("staging");
    let artifacts_dir = dir.path().join("artifacts");
    let store = Arc::new(LocalStore::new(artifacts_dir));

    let project_service = ProjectService::new(db.clone());
    let p = project_service
        .create_project("Resolver Project".to_string(), None)
        .unwrap();

    let sample_file = dir.path().join("image.png");
    fs::write(&sample_file, b"sample png bytes").unwrap();

    let importer = ArtifactImporter::new(db.clone(), store.clone(), staging_dir);
    let imported = importer.import(&p.id, &sample_file).unwrap();

    let resolver = ArtifactResolver::new(db, store);
    let resolved_path = resolver
        .resolve_uri(&imported.artifact.uri)
        .expect("resolve failed");

    assert!(resolved_path.exists());
    assert_eq!(fs::read(resolved_path).unwrap(), b"sample png bytes");
}

#[test]
fn test_reject_invalid_artifact_uri() {
    let dir = tempdir().expect("failed to create tempdir");
    let db_path = dir.path().join("invalid_uri_test.db");
    let db = Arc::new(DbManager::new(db_path).expect("failed to init db"));
    let store = Arc::new(LocalStore::new(dir.path().join("artifacts")));

    let resolver = ArtifactResolver::new(db, store);

    assert!(resolver.resolve_uri("http://malicious.com").is_err());
    assert!(resolver
        .resolve_uri("artifact://non-existent-uuid")
        .is_err());
    assert!(resolver.resolve_uri("file:///etc/passwd").is_err());
    assert!(resolver.resolve_uri("artifact://../../secret").is_err());
}

#[test]
fn test_failed_import_does_not_leave_db_artifact() {
    let dir = tempdir().expect("failed to create tempdir");
    let db_path = dir.path().join("fail_test.db");
    let db = Arc::new(DbManager::new(db_path).expect("failed to init db"));
    db.run_migrations().expect("failed to run migrations");

    let staging_dir = dir.path().join("staging");
    let artifacts_dir = dir.path().join("artifacts");
    let store = Arc::new(LocalStore::new(artifacts_dir));

    let importer = ArtifactImporter::new(db.clone(), store, staging_dir);

    // Non-existent source file
    let non_existent = dir.path().join("missing.mp4");
    let res = importer.import("non-existent-proj", &non_existent);
    assert!(res.is_err());

    // Verify 0 artifacts in DB
    let conn = db.conn().unwrap();
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM artifacts;", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0);
}

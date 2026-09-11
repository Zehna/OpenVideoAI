use openvideoai_lib::artifacts::model::{Artifact, ArtifactKind};
use openvideoai_lib::artifacts::repository::ArtifactRepository;
use openvideoai_lib::artifacts::resolver::ArtifactResolver;
use openvideoai_lib::db::migrations::run_all_migrations;
use openvideoai_lib::db::DbManager;
use openvideoai_lib::storage::local_store::LocalStore;
use openvideoai_lib::timeline::preview::handle_preview_range_request;
use rusqlite::Connection;
use std::fs::File;
use std::io::Write;
use std::sync::Arc;
use tempfile::tempdir;

fn setup_test_env() -> (tempfile::TempDir, Connection, Arc<ArtifactResolver>, String) {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let store_dir = temp_dir.path().join("store");
    std::fs::create_dir_all(&store_dir).unwrap();

    let mut disk_conn = Connection::open(&db_path).unwrap();
    disk_conn
        .execute_batch("PRAGMA foreign_keys = ON;")
        .unwrap();
    run_all_migrations(&mut disk_conn).unwrap();

    // Create test media file of exactly 100 bytes (0x00 to 0x63)
    let file_bytes: Vec<u8> = (0..100).collect();
    let sha256 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string();
    let storage_key = format!("artifacts/{}/{}", &sha256[0..2], sha256);
    let file_path = store_dir.join(&sha256[0..2]).join(&sha256);
    std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();
    let mut f = File::create(&file_path).unwrap();
    f.write_all(&file_bytes).unwrap();
    drop(f);

    let artifact = Artifact {
        id: "art-preview-1".to_string(),
        sha256,
        kind: ArtifactKind::Video,
        mime_type: Some("video/mp4".to_string()),
        size_bytes: 100,
        storage_key,
        original_filename: "test.mp4".to_string(),
        created_at: "2026-09-10T12:00:00Z".to_string(),
    };
    ArtifactRepository::insert(&disk_conn, &artifact).unwrap();

    let db_manager = Arc::new(DbManager::new(db_path).unwrap());
    let local_store = Arc::new(LocalStore::new(store_dir));
    let resolver = Arc::new(ArtifactResolver::new(db_manager, local_store));

    (temp_dir, disk_conn, resolver, artifact.id)
}

#[test]
fn test_range_bytes_0_to_49_returns_206() {
    let (_dir, conn, resolver, art_id) = setup_test_env();
    let res = handle_preview_range_request(&art_id, Some("bytes=0-49"), &resolver, &conn).unwrap();

    assert_eq!(res.status_code, 206);
    assert_eq!(res.body.len(), 50);
    assert_eq!(res.body[0], 0);
    assert_eq!(res.body[49], 49);

    let content_range = res
        .headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-range"))
        .map(|(_, v)| v.as_str());
    assert_eq!(content_range, Some("bytes 0-49/100"));
}

#[test]
fn test_open_ended_range_returns_206() {
    let (_dir, conn, resolver, art_id) = setup_test_env();
    let res = handle_preview_range_request(&art_id, Some("bytes=50-"), &resolver, &conn).unwrap();

    assert_eq!(res.status_code, 206);
    assert_eq!(res.body.len(), 50);
    assert_eq!(res.body[0], 50);
    assert_eq!(res.body[49], 99);

    let content_range = res
        .headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-range"))
        .map(|(_, v)| v.as_str());
    assert_eq!(content_range, Some("bytes 50-99/100"));
}

#[test]
fn test_no_range_header_returns_200() {
    let (_dir, conn, resolver, art_id) = setup_test_env();
    let res = handle_preview_range_request(&art_id, None, &resolver, &conn).unwrap();

    assert_eq!(res.status_code, 200);
    assert_eq!(res.body.len(), 100);
    assert_eq!(res.body[0], 0);
    assert_eq!(res.body[99], 99);
}

#[test]
fn test_out_of_bounds_range_returns_416() {
    let (_dir, conn, resolver, art_id) = setup_test_env();
    let res =
        handle_preview_range_request(&art_id, Some("bytes=200-300"), &resolver, &conn).unwrap();

    assert_eq!(res.status_code, 416);
    let content_range = res
        .headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-range"))
        .map(|(_, v)| v.as_str());
    assert_eq!(content_range, Some("bytes */100"));
}

#[test]
fn test_non_existent_artifact_returns_404() {
    let (_dir, conn, resolver, _) = setup_test_env();
    let res = handle_preview_range_request("non-existent-id", None, &resolver, &conn).unwrap();
    assert_eq!(res.status_code, 404);
}

#[test]
fn test_path_traversal_artifact_id_rejected_with_400_or_404() {
    let (_dir, conn, resolver, _) = setup_test_env();
    let res = handle_preview_range_request("../etc/passwd", None, &resolver, &conn).unwrap();
    assert!(res.status_code == 400 || res.status_code == 404);
}

#[test]
fn test_correct_mime_type_returned() {
    let (_dir, conn, resolver, art_id) = setup_test_env();
    let res = handle_preview_range_request(&art_id, None, &resolver, &conn).unwrap();

    let content_type = res
        .headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-type"))
        .map(|(_, v)| v.as_str());
    assert_eq!(content_type, Some("video/mp4"));
}

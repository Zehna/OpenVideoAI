use openvideoai_lib::storage::local_store::LocalStore;
use openvideoai_lib::storage::staging::{stream_to_staging, StagedFile};
use std::fs::{self, File};
use std::io::Write;
use tempfile::tempdir;

#[test]
fn test_streaming_sha256_correctness() {
    let dir = tempdir().expect("failed to create tempdir");
    let test_file_path = dir.path().join("test_file.bin");

    // Known test data: "hello openvideoai\n"
    let content = b"hello openvideoai\n";
    {
        let mut file = File::create(&test_file_path).expect("failed to create test file");
        file.write_all(content).expect("failed to write test data");
    }

    let staging_dir = dir.path().join("staging");
    fs::create_dir_all(&staging_dir).expect("failed to create staging dir");

    let staged: StagedFile =
        stream_to_staging(&test_file_path, &staging_dir).expect("failed to stream to staging");

    // Precalculated SHA-256 for b"hello openvideoai\n"
    // echo "hello openvideoai" | sha256sum
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(content);
    let expected_sha256 = format!("{:x}", hasher.finalize());

    assert_eq!(staged.sha256, expected_sha256);
    assert_eq!(staged.size_bytes, content.len() as u64);
    assert!(staged.temp_path.exists());
}

#[test]
fn test_streaming_staging_and_promote() {
    let dir = tempdir().expect("failed to create tempdir");
    let staging_dir = dir.path().join("staging");
    let artifacts_dir = dir.path().join("artifacts");

    let source_path = dir.path().join("video.mp4");
    fs::write(&source_path, b"fake mp4 video stream bytes").expect("failed to write source");

    let staged = stream_to_staging(&source_path, &staging_dir).expect("staging failed");
    let store = LocalStore::new(artifacts_dir.clone());

    let storage_key = store.store_artifact(&staged).expect("store failed");
    assert!(storage_key.starts_with("artifacts/"));
    assert!(
        !staged.temp_path.exists(),
        "Staging file should have been moved"
    );

    let resolved = store.resolve_path(&storage_key).expect("resolve failed");
    assert!(resolved.exists());
    assert_eq!(
        fs::read(&resolved).expect("read failed"),
        b"fake mp4 video stream bytes"
    );
}

#[test]
fn test_canonical_path_partitioning() {
    let dir = tempdir().expect("failed to create tempdir");
    let artifacts_dir = dir.path().join("artifacts");
    let store = LocalStore::new(artifacts_dir);

    let sha256 = "3a7bd2e098492040182810928410294102948120491820491820491820491820";
    let key = store.compute_storage_key(sha256);
    assert_eq!(key, format!("artifacts/3a/{}", sha256));
}

#[test]
fn test_storage_resolution_prevents_path_traversal() {
    let dir = tempdir().expect("failed to create tempdir");
    let artifacts_dir = dir.path().join("artifacts");
    let store = LocalStore::new(artifacts_dir);

    // Attempt traversal
    assert!(store
        .resolve_path("artifacts/../../windows/system32")
        .is_err());
    assert!(store.resolve_path("../secret.txt").is_err());
    assert!(store.resolve_path("invalid_prefix/3a/something").is_err());
}

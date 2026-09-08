use openvideoai_lib::storage::recovery::sweep_staging_directory;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_sweep_staging_directory_cleans_stale_files() {
    let dir = tempdir().expect("failed to create tempdir");
    let staging_dir = dir.path().join("staging");
    fs::create_dir_all(&staging_dir).expect("create staging failed");

    let f1 = staging_dir.join("stage_file1.tmp");
    let f2 = staging_dir.join("stage_file2.tmp");
    fs::write(&f1, b"partial upload bytes 1").expect("write f1 failed");
    fs::write(&f2, b"partial upload bytes 2").expect("write f2 failed");

    assert!(f1.exists());
    assert!(f2.exists());

    let swept_count = sweep_staging_directory(&staging_dir).expect("sweep failed");
    assert_eq!(swept_count, 2);
    assert!(!f1.exists());
    assert!(!f2.exists());
}

#[test]
fn test_unrelated_files_outside_staging_are_untouched() {
    let dir = tempdir().expect("failed to create tempdir");
    let staging_dir = dir.path().join("staging");
    let artifacts_dir = dir.path().join("artifacts");
    fs::create_dir_all(&staging_dir).expect("create staging failed");
    fs::create_dir_all(&artifacts_dir).expect("create artifacts failed");

    let keep_file = artifacts_dir.join("canonical.bin");
    fs::write(&keep_file, b"permanent canonical file").expect("write keep failed");

    let stale = staging_dir.join("stage_stale.tmp");
    fs::write(&stale, b"stale").expect("write stale failed");

    let swept = sweep_staging_directory(&staging_dir).expect("sweep failed");
    assert_eq!(swept, 1);
    assert!(!stale.exists());
    assert!(keep_file.exists());
}

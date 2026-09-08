use openvideoai_lib::artifacts::importer::ArtifactImporter;
use openvideoai_lib::artifacts::resolver::ArtifactResolver;
use openvideoai_lib::db::DbManager;
use openvideoai_lib::projects::service::ProjectService;
use openvideoai_lib::storage::local_store::LocalStore;
use openvideoai_lib::storage::recovery::sweep_staging_directory;
use std::fs;
use std::process::Command;
use std::sync::Arc;
use tempfile::tempdir;

fn generate_synthetic_mp4(dest_path: &std::path::Path) -> bool {
    // Generate a valid 1-second 320x240 H.264 video with silent AAC audio using host ffmpeg
    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=c=blue:s=320x240:d=1",
            "-f",
            "lavfi",
            "-i",
            "anullsrc=r=44100:cl=stereo",
            "-c:v",
            "libx264",
            "-t",
            "1",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            "-shortest",
            dest_path.to_str().unwrap(),
        ])
        .output();

    match status {
        Ok(output) => output.status.success(),
        Err(_) => false,
    }
}

#[test]
fn test_phase1_runtime_media_acceptance_and_deduplication() {
    let dir = tempdir().expect("failed to create tempdir");
    let base_dir = dir.path();

    // 1. Generate real synthetic MP4 file
    let media_src = base_dir.join("sample_video.mp4");
    let generated = generate_synthetic_mp4(&media_src);
    if !generated {
        eprintln!("Host ffmpeg failed to generate test media, skipping runtime probe test");
        return;
    }
    assert!(media_src.exists());
    let src_bytes = fs::read(&media_src).expect("read source mp4");
    assert!(!src_bytes.is_empty());

    // 2. Initialize Subsystems
    let db_path = base_dir.join("test.db");
    let db_manager = Arc::new(DbManager::new(db_path.clone()).expect("init db"));
    db_manager.run_migrations().expect("migrations 1 & 2");

    let artifacts_dir = base_dir.join("artifacts");
    let staging_dir = base_dir.join("staging");
    let local_store = Arc::new(LocalStore::new(artifacts_dir.clone()));

    let project_service = ProjectService::new(db_manager.clone());
    let importer =
        ArtifactImporter::new(db_manager.clone(), local_store.clone(), staging_dir.clone());
    let resolver = ArtifactResolver::new(db_manager.clone(), local_store);

    // 3. Create Project A ("Feature Alpha")
    let p_alpha = project_service
        .create_project("Feature Alpha".to_string(), None)
        .expect("create project A");

    // 4. Import media into Project A
    let imported_a1 = importer
        .import(&p_alpha.id, &media_src)
        .expect("import media into project A");

    assert_eq!(imported_a1.original_filename, "sample_video.mp4");
    assert_eq!(imported_a1.artifact.mime_type.as_deref(), Some("video/mp4"));
    assert_eq!(imported_a1.artifact.size_bytes, src_bytes.len() as u64);
    assert_eq!(
        imported_a1.artifact.uri,
        format!("artifact://{}", imported_a1.artifact.artifact_id)
    );

    // Verify metadata was probed by ffprobe
    assert!(
        imported_a1.metadata.is_some(),
        "FFprobe metadata should be extracted"
    );
    let meta = imported_a1.metadata.unwrap();
    assert_eq!(meta.probe_status, "ok");
    assert_eq!(meta.width, Some(320));
    assert_eq!(meta.height, Some(240));
    assert!(meta.video_codec.is_some());
    assert!(meta.duration_ms.is_some());

    // Verify canonical path partitioned storage: artifacts/xx/xxxx...
    let canonical_path = resolver
        .resolve_uri(&imported_a1.artifact.uri)
        .expect("resolve canonical path");
    assert!(canonical_path.exists());
    assert!(canonical_path
        .to_string_lossy()
        .contains(&imported_a1.artifact.sha256[..2]));

    // 5. Re-import SAME media into Project A (Deduplication within project)
    let imported_a2 = importer
        .import(&p_alpha.id, &media_src)
        .expect("re-import same media into project A");

    assert!(imported_a2.is_deduplicated);
    assert_eq!(
        imported_a2.artifact.artifact_id,
        imported_a1.artifact.artifact_id
    );
    assert_eq!(imported_a2.artifact.sha256, imported_a1.artifact.sha256);

    let links_a = importer
        .list_for_project(&p_alpha.id)
        .expect("list project A artifacts");
    assert_eq!(
        links_a.len(),
        1,
        "Project A should only link to artifact once"
    );

    // 6. Create Project B ("Feature Beta") and import same media
    let p_beta = project_service
        .create_project("Feature Beta".to_string(), None)
        .expect("create project B");

    let imported_b1 = importer
        .import(&p_beta.id, &media_src)
        .expect("import media into project B");

    assert!(imported_b1.is_deduplicated);
    assert_eq!(
        imported_b1.artifact.artifact_id,
        imported_a1.artifact.artifact_id
    );
    assert_eq!(imported_b1.artifact.sha256, imported_a1.artifact.sha256);

    // List artifacts in Project B
    let links_b = importer
        .list_for_project(&p_beta.id)
        .expect("list project B artifacts");
    assert_eq!(links_b.len(), 1);

    // Check total artifact count in artifacts directory: exactly 1 canonical file
    let mut canonical_file_count = 0;
    for entry in fs::read_dir(&artifacts_dir).expect("read artifacts dir") {
        let entry = entry.expect("entry");
        if entry.path().is_dir() {
            for sub in fs::read_dir(entry.path()).expect("read shard dir") {
                let sub = sub.expect("sub entry");
                if sub.path().is_file() {
                    canonical_file_count += 1;
                }
            }
        }
    }
    assert_eq!(
        canonical_file_count, 1,
        "Storage must have exactly 1 deduplicated canonical file"
    );

    // 7. Test Staging Recovery
    let orphan_tmp = staging_dir.join("stage_interrupted_upload.tmp");
    fs::write(&orphan_tmp, b"incomplete chunks").expect("write orphan tmp");
    assert!(orphan_tmp.exists());

    let swept = sweep_staging_directory(&staging_dir).expect("sweep staging");
    assert_eq!(swept, 1);
    assert!(!orphan_tmp.exists());
    assert!(
        canonical_path.exists(),
        "Canonical artifacts must remain intact"
    );

    // 8. Test Project Update & Persistence across DB reopen
    project_service
        .update_project(
            &p_alpha.id,
            "Alpha Final".to_string(),
            Some("Updated description".to_string()),
        )
        .expect("update project");

    drop(project_service);
    drop(importer);
    drop(resolver);
    drop(db_manager);

    // Reopen DB
    let db_manager2 = Arc::new(DbManager::new(db_path).expect("reopen db"));
    let project_service2 = ProjectService::new(db_manager2);
    let reloaded = project_service2
        .get_project(&p_alpha.id)
        .expect("project query successful");

    assert_eq!(reloaded.name, "Alpha Final");
    assert_eq!(reloaded.description.as_deref(), Some("Updated description"));
}

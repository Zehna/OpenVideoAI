# Phase 1: Project & Artifact Core Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a reliable local Project and Artifact foundation that supports project management, streaming SHA-256 deduplicated artifact storage, FFprobe metadata extraction, atomic file staging/promotion, debounced autosave, and startup staging reconciliation.

**Architecture:** Global content-addressed local artifact store partitioned by SHA-256 hash prefix (`artifacts/<sha256[0:2]>/<sha256>`). Projects reference assets through immutable `ArtifactRef` (`artifact://<artifactId>`). SQLite persists project records, artifact registries, media metadata, and join links. Staging is atomic and swept on startup.

**Tech Stack:** Tauri 2, Rust (rusqlite, sha2, uuid, tracing), React 19, TypeScript (strict), Vitest, Tailwind CSS.

**Spec:** `docs/superpowers/specs/2026-09-08-phase-1-project-artifact-core-design.md`

## Global Constraints
- Target workspace: `D:\OpenVideoAI`
- Git branch: `phase/1-project-artifact-core` (based on `v0.1.0` / `cb22c51`)
- Strict scope boundary: NO AI providers, Whisper, TTS, Kaggle, Activepieces, timeline editor, video renderer, or Docker/Postgres/Redis.
- Physical storage paths must NEVER be exposed in public `ArtifactRef` responses.
- All database operations touching multiple tables must be transactional with foreign keys enabled.
- All test suites must use temporary directories and temporary SQLite databases.
- Zero warnings on `cargo clippy -- -D warnings` and TypeScript compiler (`tsc --noEmit`).

---

### Task 1: Dependencies, Migration 2 & Project Persistence Subsystem

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/db/migrations.rs`
- Modify: `src-tauri/src/db/mod.rs`
- Create: `src-tauri/src/projects/mod.rs`
- Create: `src-tauri/src/projects/model.rs`
- Create: `src-tauri/src/projects/repository.rs`
- Create: `src-tauri/src/projects/service.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/tests/project_tests.rs`

**Interfaces:**
- Consumes: `DbManager` (`rusqlite::Connection`)
- Produces:
  ```rust
  pub struct Project {
      pub id: String,
      pub name: String,
      pub description: Option<String>,
      pub created_at: String,
      pub updated_at: String,
  }
  pub struct ProjectService {
      pub fn create_project(&self, name: String, description: Option<String>) -> Result<Project, ProjectError>;
      pub fn list_projects(&self) -> Result<Vec<Project>, ProjectError>;
      pub fn get_project(&self, id: &str) -> Result<Project, ProjectError>;
      pub fn update_project(&self, id: &str, name: String, description: Option<String>) -> Result<Project, ProjectError>;
  }
  ```

- [ ] **Step 1: Add dependencies to `src-tauri/Cargo.toml`**
Add `sha2 = "0.10"`, `uuid = { version = "1", features = ["v4", "fast-rng"] }`, `chrono = { version = "0.4", features = ["serde"] }`.

- [ ] **Step 2: Add Migration 2 to `src-tauri/src/db/migrations.rs`**
Add Migration 2 creating tables: `projects`, `artifacts`, `artifact_media_metadata`, `project_artifacts`.

- [ ] **Step 3: Write failing project persistence tests**
In `src-tauri/tests/project_tests.rs`:
- `test_create_and_get_project`
- `test_list_projects`
- `test_update_project`
- `test_project_persists_after_db_reopen`
- `test_migration_2_is_idempotent`
- `test_project_foreign_key_constraints`

- [ ] **Step 4: Run tests to verify failure**
Run: `cargo test --manifest-path src-tauri/Cargo.toml --test project_tests`
Expected: FAIL with compilation error (modules not found).

- [ ] **Step 5: Implement `src-tauri/src/projects/` models, repository, and service**
Implement `Project`, `ProjectError`, `ProjectRepository`, and `ProjectService` with input validation (non-empty name, trimmed).

- [ ] **Step 6: Run tests to verify they pass**
Run: `cargo test --manifest-path src-tauri/Cargo.toml --test project_tests`
Expected: PASS (6/6 tests).

- [ ] **Step 7: Commit Task 1**
```bash
git add src-tauri/Cargo.toml src-tauri/src/db/ src-tauri/src/projects/ src-tauri/src/lib.rs src-tauri/tests/project_tests.rs
git commit -m "feat: project persistence"
```

---

### Task 2: Artifact Storage, Streaming Hashing & Staging Subsystem

**Files:**
- Create: `src-tauri/src/storage/mod.rs`
- Create: `src-tauri/src/storage/staging.rs`
- Create: `src-tauri/src/storage/local_store.rs`
- Create: `src-tauri/src/artifacts/mod.rs`
- Create: `src-tauri/src/artifacts/model.rs`
- Create: `src-tauri/src/artifacts/hashing.rs`
- Create: `src-tauri/src/artifacts/repository.rs`
- Test: `src-tauri/tests/artifact_storage_tests.rs`

**Interfaces:**
- Consumes: Storage directories (`app_data_dir/artifacts`, `app_data_dir/staging`), `DbManager`
- Produces:
  ```rust
  pub struct StagedFile {
      pub temp_path: PathBuf,
      pub sha256: String,
      pub size_bytes: u64,
  }
  pub fn stream_to_staging(source_path: &Path, staging_dir: &Path) -> Result<StagedFile, StorageError>;
  pub struct LocalStore {
      pub fn store_artifact(&self, staged: StagedFile) -> Result<String, StorageError>; // returns storage_key
      pub fn resolve_path(&self, storage_key: &str) -> Result<PathBuf, StorageError>;
  }
  ```

- [ ] **Step 1: Write failing artifact storage and hashing tests**
In `src-tauri/tests/artifact_storage_tests.rs`:
- `test_streaming_sha256_correctness` (compare against known byte vector SHA-256)
- `test_streaming_staging_and_promote`
- `test_canonical_path_partitioning` (check `artifacts/<sha256[0:2]>/<sha256>`)
- `test_storage_resolution_prevents_path_traversal`

- [ ] **Step 2: Run tests to verify failure**
Run: `cargo test --manifest-path src-tauri/Cargo.toml --test artifact_storage_tests`
Expected: FAIL with compilation error.

- [ ] **Step 3: Implement streaming staging, SHA-256 hasher, and local store**
Implement `stream_to_staging` reading in 64KB buffers while updating `sha2::Sha256` and writing to `staging/stage_<uuid>.tmp`. Implement `LocalStore::store_artifact` atomic rename and directory creation.

- [ ] **Step 4: Run tests to verify they pass**
Run: `cargo test --manifest-path src-tauri/Cargo.toml --test artifact_storage_tests`
Expected: PASS.

- [ ] **Step 5: Commit Task 2**
```bash
git add src-tauri/src/storage/ src-tauri/src/artifacts/ src-tauri/tests/artifact_storage_tests.rs
git commit -m "feat: artifact storage and hashing"
```

---

### Task 3: Media Metadata Probing with FFprobe

**Files:**
- Create: `src-tauri/src/artifacts/metadata.rs`
- Modify: `src-tauri/src/artifacts/mod.rs`
- Test: `src-tauri/tests/metadata_tests.rs`

**Interfaces:**
- Consumes: FFprobe executable discovery (`tools::find_executable`), canonical file path
- Produces:
  ```rust
  pub struct MediaMetadata {
      pub artifact_id: String,
      pub probe_status: String, // "ok", "unavailable", "failed"
      pub duration_ms: Option<i64>,
      pub width: Option<i32>,
      pub height: Option<i32>,
      pub video_codec: Option<String>,
      pub audio_codec: Option<String>,
      pub frame_rate: Option<f64>,
      pub sample_rate: Option<i32>,
      pub channels: Option<i32>,
      pub probe_error: Option<String>,
  }
  pub fn probe_media(artifact_id: &str, file_path: &Path, ffprobe_path: Option<&Path>) -> MediaMetadata;
  ```

- [ ] **Step 1: Write failing metadata probing tests**
In `src-tauri/tests/metadata_tests.rs`:
- `test_parse_ffprobe_json_video_output` (mock JSON payload from standard video probe)
- `test_parse_ffprobe_json_audio_only_output`
- `test_probe_when_ffprobe_unavailable` (returns `probe_status = "unavailable"` with no panic)
- `test_probe_when_invalid_media` (returns `probe_status = "failed"` with captured error)

- [ ] **Step 2: Run tests to verify failure**
Run: `cargo test --manifest-path src-tauri/Cargo.toml --test metadata_tests`
Expected: FAIL.

- [ ] **Step 3: Implement FFprobe probe execution and JSON parser**
Execute `ffprobe -v quiet -print_format json -show_format -show_streams <path>` with a 15-second timeout, parse container streams and format headers into `MediaMetadata`.

- [ ] **Step 4: Run tests to verify they pass**
Run: `cargo test --manifest-path src-tauri/Cargo.toml --test metadata_tests`
Expected: PASS.

- [ ] **Step 5: Commit Task 3**
```bash
git add src-tauri/src/artifacts/metadata.rs src-tauri/src/artifacts/mod.rs src-tauri/tests/metadata_tests.rs
git commit -m "feat: artifact metadata probing"
```

---

### Task 4: Project Artifact Import Flow & Tauri IPC Commands

**Files:**
- Create: `src-tauri/src/artifacts/importer.rs`
- Create: `src-tauri/src/artifacts/resolver.rs`
- Create: `src-tauri/src/artifacts/commands.rs`
- Create: `src-tauri/src/projects/commands.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/tests/import_tests.rs`

**Interfaces:**
- Consumes: `ProjectService`, `LocalStore`, `ArtifactRepository`, `probe_media`
- Produces:
  - IPC commands: `create_project`, `list_projects`, `get_project`, `update_project`, `import_artifact`, `list_project_artifacts`
  - Safe URI resolver: `resolve_artifact_uri(uri: &str) -> Result<PathBuf, ResolverError>`

- [ ] **Step 1: Write failing import integration tests**
In `src-tauri/tests/import_tests.rs`:
- `test_first_import_creates_canonical_artifact`
- `test_duplicate_import_in_same_project_yields_one_link`
- `test_cross_project_import_reuses_same_artifact`
- `test_resolve_artifact_uri_to_canonical_path`
- `test_reject_invalid_artifact_uri`
- `test_failed_import_does_not_leave_db_artifact`

- [ ] **Step 2: Run tests to verify failure**
Run: `cargo test --manifest-path src-tauri/Cargo.toml --test import_tests`
Expected: FAIL.

- [ ] **Step 3: Implement Importer, Resolver, and IPC Commands**
Implement `ArtifactImporter::import`, `ArtifactResolver::resolve_uri`, wire handlers into `src-tauri/src/lib.rs` and update `AppState`.

- [ ] **Step 4: Run tests to verify they pass**
Run: `cargo test --manifest-path src-tauri/Cargo.toml --test import_tests`
Expected: PASS.

- [ ] **Step 5: Commit Task 4**
```bash
git add src-tauri/src/artifacts/ src-tauri/src/projects/commands.rs src-tauri/src/lib.rs src-tauri/tests/import_tests.rs
git commit -m "feat: project artifact import flow"
```

---

### Task 5: Frontend Projects & Assets UI

**Files:**
- Create: `src/features/projects/types.ts`
- Create: `src/features/projects/api.ts`
- Create: `src/features/projects/hooks/useAutosave.ts`
- Create: `src/features/projects/components/ProjectList.tsx`
- Create: `src/features/projects/components/ProjectDetail.tsx`
- Create: `src/features/projects/components/CreateProjectModal.tsx`
- Create: `src/features/artifacts/types.ts`
- Create: `src/features/artifacts/api.ts`
- Create: `src/features/artifacts/components/ArtifactList.tsx`
- Create: `src/features/artifacts/components/ImportModal.tsx`
- Modify: `src/app/App.tsx`
- Test: `src/features/projects/__tests__/ProjectDetail.test.tsx`
- Test: `src/features/projects/__tests__/ProjectList.test.tsx`
- Test: `src/features/artifacts/__tests__/ArtifactList.test.tsx`

**Interfaces:**
- Consumes: Tauri IPC commands
- Produces: Project CRUD UI, debounced autosave, file import modal, asset list with metadata and deduplication badges.

- [ ] **Step 1: Write failing frontend unit tests**
In `src/features/projects/__tests__/`:
- Test project list rendering and selection
- Test create project form submission
- Test debounced project title/description autosave
- Test artifact list rendering with metadata details
- Test metadata unavailable/failed state rendering
- Test deduplication badge indicator

- [ ] **Step 2: Run tests to verify failure**
Run: `npm run test`
Expected: FAIL.

- [ ] **Step 3: Implement types, APIs, components, and App integration**
Implement clean, modular React components using Tailwind CSS and integrate into `App.tsx` navigation bar (switching between "Projects" and "System Health").

- [ ] **Step 4: Run tests to verify they pass**
Run: `npm run test`
Expected: PASS.

- [ ] **Step 5: Verify frontend build & typecheck**
Run: `npm run typecheck; npm run build`
Expected: PASS with 0 errors.

- [ ] **Step 6: Commit Task 5**
```bash
git add src/
git commit -m "feat(frontend): project and asset UI"
```

---

### Task 6: Autosave & Staging Recovery Subsystem

**Files:**
- Create: `src-tauri/src/storage/recovery.rs`
- Modify: `src-tauri/src/storage/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/tests/recovery_tests.rs`

**Interfaces:**
- Consumes: `staging_dir`
- Produces:
  ```rust
  pub fn sweep_staging_directory(staging_dir: &Path) -> Result<usize, StorageError>;
  ```

- [ ] **Step 1: Write failing recovery tests**
In `src-tauri/tests/recovery_tests.rs`:
- `test_sweep_staging_directory_cleans_stale_files`
- `test_unrelated_files_outside_staging_are_untouched`

- [ ] **Step 2: Run tests to verify failure**
Run: `cargo test --manifest-path src-tauri/Cargo.toml --test recovery_tests`
Expected: FAIL.

- [ ] **Step 3: Implement staging cleanup and hook into Tauri startup**
In `src-tauri/src/storage/recovery.rs` sweep all `stage_*.tmp` files inside `staging/` on setup in `lib.rs`.

- [ ] **Step 4: Run tests to verify they pass**
Run: `cargo test --manifest-path src-tauri/Cargo.toml --test recovery_tests`
Expected: PASS.

- [ ] **Step 5: Commit Task 6**
```bash
git add src-tauri/src/storage/ src-tauri/src/lib.rs src-tauri/tests/recovery_tests.rs
git commit -m "feat: autosave and staging recovery"
```

---

### Task 7: Quality Gates & Verification Script Hardening

**Files:**
- Modify: `scripts/verify.ps1`

- [ ] **Step 1: Update `scripts/verify.ps1` if needed and execute full gate suite**
Run: `powershell -ExecutionPolicy Bypass -File scripts/verify.ps1`
Expected: ALL 6 GATES PASS.

- [ ] **Step 2: Perform runtime media acceptance test**
Create real media test file (small MP4/PNG), execute runtime import, duplicate same-project import, cross-project import, and application restart verification.

- [ ] **Step 3: Commit Task 7**
```bash
git add scripts/verify.ps1
git commit -m "test/chore: verification and runtime hardening"
```

- [ ] **Step 4: Generate Phase 1 Completion Report**
Verify all acceptance criteria, check git status is clean, and output final report.

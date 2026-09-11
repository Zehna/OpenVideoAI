# Phase 3 — Media & Timeline Core Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver the first usable non-destructive video editing vertical slice for OpenVideoAI with multi-track timeline persistence, optimistic revision control, bounded persistent undo/redo, secure media preview gateway, immutable RenderSpec v1 with frozen stream capabilities, local FFmpeg rendering with timeline PTS alignment, type-safe job leasing, and Phase 2 durable job integration.

**Architecture:** A layered, non-destructive architecture where frontend timeline mutations flow through `TimelineService` with optimistic revision locking into SQLite (Migration 5). The editor renders preview media securely via `openvideoai-media://artifact/<id>` without exposing file paths. On render, an immutable `RenderSpecV1` snapshot is compiled from timeline and metadata into an `FfmpegExecutionPlan` (with `setpts=PTS-STARTPTS+start/TB` and `amix duration=longest` trimmed to canvas duration) and submitted as a durable `render.timeline` job. A background local render worker acquires the job via atomic type-filtered leasing (`acquire_next_job_for_type`), manages lease heartbeats, monitors child process execution, records 7 stage checkpoints, and imports the final MP4 into Artifact Core as an `ArtifactRef`.

**Tech Stack:** Rust (rusqlite, serde, tokio, tauri v2, tempfile), FFmpeg / FFprobe (system binary via argument arrays), TypeScript, React, Tailwind CSS, Vitest, React Testing Library.

**Spec:** [OpenVideoAI Phase 3 — Media & Timeline Core Design](file:///d:/OpenVideoAI/docs/superpowers/specs/2026-09-10-phase-3-media-timeline-core-design.md)

---

## Global Constraints

- **Base commit floor:** Phase 2 release-close state at `f8438a4`. Tag `v0.3.0` before branching.
- **Target branch:** `phase/3-media-timeline-core`.
- **Target version:** `v0.4.0`.
- **Pre-existing migrations are immutable:** Migrations 1–4 MUST NOT be modified. Phase 3 schema resides exclusively in Migration 5.
- **Canonical time representation:** Canonical timeline and clip timestamps MUST be integer milliseconds (`i64`).
- **Same-track overlap rule:** Overlapping clips on the same track are forbidden and rejected with `clip_overlap`. Cross-track overlap is permitted.
- **Duration is derived:** Timeline `durationMs` is computed from the maximum clip end time (`timelineStartMs + (sourceOutMs - sourceInMs)`) across all tracks; it is never independently stored as a mutable source of truth.
- **Optimistic revision control:** Every committed mutation and undo/redo operation requires matching `expectedRevision`. Monotonic increment (`revision += 1`). Conflicts return `revision_conflict`.
- **Initial timeline history:** Timeline creation transactionally creates `revision = 1`, `history_cursor = 0`, and the initial snapshot at `history_index = 0`.
- **Bounded undo/redo:** At most 50 snapshots in `timeline_history`. New edits after undo prune the redo branch. Undo/redo strictly increment `revision`.
- **Security & Artifact boundary:** Frontend NEVER receives physical filesystem paths. Media preview is served strictly via read-only `openvideoai-media://artifact/<id>` gateway with byte-range support.
- **RenderSpec immutability & metadata binding:** RenderSpecBuilder freezes stream capabilities (`hasVideo`, `hasAudio`) from committed artifact metadata into immutable `RenderClip` entries. Workers never probe live media or read live timeline state.
- **FFmpeg safety:** FFmpeg execution MUST use executable path + argument arrays (`Command::arg`), NEVER shell-string concatenation.
- **Timeline PTS placement:** Clips placed at `timelineStartMs > 0` MUST have presentation timestamps placed onto the timeline using `setpts=PTS-STARTPTS+<startSeconds>/TB` before compositing.
- **Audio duration policy:** Multi-source mixing uses `amix=inputs=N:duration=longest` and is strictly constrained to the timeline duration using `atrim=end=<duration>`. Zero-audio timelines generate silent audio matching timeline duration via `anullsrc`.
- **Type-safe leasing:** Render worker leases jobs exclusively via atomic type-filtered query `acquire_next_job_for_type("render.timeline", lease_secs)`. It never leases unrelated jobs.
- **Public worker lifecycle:** Background render worker is operated via `start_local_render_worker` returning a handle with clean shutdown and heartbeat monitoring.
- **Output scope:** Fixed to MP4 container, H.264 video codec, AAC audio codec, and yuv420p pixel format.
- **No Phase 4+ creep:** No AI models, Whisper, TTS, ComfyUI, transitions, keyframes, or server modes.

---

## Task Decomposition

### Task 1: Git Release Tag `v0.3.0` & Target Branch `phase/3-media-timeline-core`

**Files:**
- Modify: `.git` (repository state, tags, and branch)

**Interfaces:**
- Consumes: Head commit `f8438a4` on `phase/2-durable-jobs-events`
- Produces: Git tag `v0.3.0` and active branch `phase/3-media-timeline-core`

- [ ] **Step 1: Verify Phase 2 clean state and quality gates**
Run: `powershell -ExecutionPolicy Bypass -File scripts/verify.ps1`
Expected: ALL PHASE 2 QUALITY GATES PASSED (exit code 0).

- [ ] **Step 2: Verify or create Git tag `v0.3.0`**
Run: `git tag -l "v0.3.0"`
If not present:
`git tag -a v0.3.0 -m "release: v0.3.0 Durable Jobs & Events"`
Run: `git tag --list`
Expected: `v0.1.0`, `v0.2.0`, `v0.3.0`.

- [ ] **Step 3: Create and checkout Phase 3 branch**
Run: `git checkout -b phase/3-media-timeline-core` (or `git checkout phase/3-media-timeline-core` if already created)
Run: `git branch --show-current`
Expected: `phase/3-media-timeline-core`.

---

### Task 2: Migration 5 — SQLite Timeline & History Schema

**Files:**
- Modify: `src-tauri/src/db/migrations.rs`
- Test: `src-tauri/tests/db_tests.rs`

**Interfaces:**
- Consumes: `MIGRATIONS` slice in `src-tauri/src/db/migrations.rs`
- Produces: Migration 5 adding tables `timelines`, `timeline_tracks`, `timeline_clips`, `timeline_history` and required indexes.

- [ ] **Step 1: Write the failing test for Migration 5 upgrade and schema verification**
Add `test_migration_5_applies_cleanly_over_migration_4` in `src-tauri/tests/db_tests.rs`:
```rust
#[test]
fn test_migration_5_applies_cleanly_over_migration_4() {
    let mut conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    run_all_migrations(&mut conn).unwrap();

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
        "INSERT INTO timelines (id, project_id, name, revision, width, height, frame_rate, history_cursor, created_at, updated_at)
         VALUES ('t1', 'p1', 'Main Timeline', 1, 1920, 1080, 30.0, 0, datetime('now'), datetime('now'));",
        [],
    ).unwrap();

    conn.execute(
        "INSERT INTO timeline_tracks (id, timeline_id, kind, order_index, name, muted)
         VALUES ('tr1', 't1', 'video', 0, 'V1', 0);",
        [],
    ).unwrap();

    // Track order uniqueness per timeline
    let duplicate_order = conn.execute(
        "INSERT INTO timeline_tracks (id, timeline_id, kind, order_index, name, muted)
         VALUES ('tr2', 't1', 'video', 0, 'V2', 0);",
        [],
    );
    assert!(duplicate_order.is_err(), "Duplicate track order on same timeline must be rejected");

    // Cascading delete: project deletion cascades to timeline and tracks
    conn.execute("DELETE FROM projects WHERE id = 'p1';", []).unwrap();
    let count: i64 = conn.query_row("SELECT count(*) FROM timelines WHERE id = 't1';", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 0);
    let track_count: i64 = conn.query_row("SELECT count(*) FROM timeline_tracks WHERE id = 'tr1';", [], |r| r.get(0)).unwrap();
    assert_eq!(track_count, 0);
}
```

- [ ] **Step 2: Run test to verify it fails**
Run: `cargo test --manifest-path src-tauri/Cargo.toml test_migration_5_applies_cleanly_over_migration_4`
Expected: FAIL (Migration 5 not yet defined).

- [ ] **Step 3: Implement Migration 5 in `src-tauri/src/db/migrations.rs`**
Add Migration 5 to `MIGRATIONS`:
```rust
    Migration {
        version: 5,
        name: "media_and_timeline_core",
        sql: "
            CREATE TABLE IF NOT EXISTS timelines (
                id TEXT PRIMARY KEY,
                project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                name TEXT NOT NULL,
                revision INTEGER NOT NULL DEFAULT 1,
                width INTEGER NOT NULL,
                height INTEGER NOT NULL,
                frame_rate REAL NOT NULL,
                history_cursor INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS timeline_tracks (
                id TEXT PRIMARY KEY,
                timeline_id TEXT NOT NULL REFERENCES timelines(id) ON DELETE CASCADE,
                kind TEXT NOT NULL,
                order_index INTEGER NOT NULL,
                name TEXT NOT NULL,
                muted INTEGER NOT NULL DEFAULT 0,
                UNIQUE(timeline_id, order_index)
            );

            CREATE TABLE IF NOT EXISTS timeline_clips (
                id TEXT PRIMARY KEY,
                track_id TEXT NOT NULL REFERENCES timeline_tracks(id) ON DELETE CASCADE,
                artifact_id TEXT NOT NULL REFERENCES artifacts(id),
                source_in_ms INTEGER NOT NULL,
                source_out_ms INTEGER NOT NULL,
                timeline_start_ms INTEGER NOT NULL,
                volume REAL NOT NULL DEFAULT 1.0,
                muted INTEGER NOT NULL DEFAULT 0,
                position_x REAL NOT NULL DEFAULT 0.0,
                position_y REAL NOT NULL DEFAULT 0.0,
                scale REAL NOT NULL DEFAULT 1.0,
                rotation_deg REAL NOT NULL DEFAULT 0.0,
                opacity REAL NOT NULL DEFAULT 1.0
            );

            CREATE TABLE IF NOT EXISTS timeline_history (
                timeline_id TEXT NOT NULL REFERENCES timelines(id) ON DELETE CASCADE,
                history_index INTEGER NOT NULL,
                snapshot_json TEXT NOT NULL,
                operation_type TEXT NOT NULL,
                created_at TEXT NOT NULL,
                PRIMARY KEY(timeline_id, history_index)
            );

            CREATE INDEX IF NOT EXISTS idx_timelines_project_id ON timelines(project_id);
            CREATE INDEX IF NOT EXISTS idx_timeline_tracks_timeline_order ON timeline_tracks(timeline_id, order_index);
            CREATE INDEX IF NOT EXISTS idx_timeline_clips_track_start ON timeline_clips(track_id, timeline_start_ms);
            CREATE INDEX IF NOT EXISTS idx_timeline_clips_artifact_id ON timeline_clips(artifact_id);
            CREATE INDEX IF NOT EXISTS idx_timeline_history_timeline_index ON timeline_history(timeline_id, history_index);
        ",
    },
```

- [ ] **Step 4: Run test to verify it passes**
Run: `cargo test --manifest-path src-tauri/Cargo.toml test_migration_5_applies_cleanly_over_migration_4`
Expected: PASS.

- [ ] **Step 5: Commit**
```bash
git add src-tauri/src/db/migrations.rs src-tauri/tests/db_tests.rs
git commit -m "feat(db): add Migration 5 for media and timeline core schema"
```

---

### Task 3: Timeline Domain Models, Invariants & Error Types

**Files:**
- Create: `src-tauri/src/timeline/mod.rs`
- Create: `src-tauri/src/timeline/model.rs`
- Create: `src-tauri/src/timeline/error.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/tests/timeline_models_tests.rs`

**Interfaces:**
- Consumes: `serde::{Serialize, Deserialize}`, `chrono::Utc`
- Produces: `TimelineDocument`, `TimelineTrack`, `TimelineClip`, `ClipTransform`, `CanvasConfig`, `TimelineError`

- [ ] **Step 1: Write failing test for domain invariants, clip transform defaults, and derived duration**
Create `src-tauri/tests/timeline_models_tests.rs`:
```rust
use openvideoai_lib::timeline::model::*;
use openvideoai_lib::timeline::error::TimelineError;

#[test]
fn test_clip_duration_and_invariants() {
    let mut clip = TimelineClip {
        id: "c1".to_string(),
        track_id: "t1".to_string(),
        artifact_id: "a1".to_string(),
        source_in_ms: 1000,
        source_out_ms: 5000,
        timeline_start_ms: 2000,
        volume: 1.0,
        muted: false,
        transform: ClipTransform::default(),
    };
    assert_eq!(clip.duration_ms(), 4000);
    assert_eq!(clip.timeline_end_ms(), 6000);
    assert!(clip.validate(Some(6000)).is_ok());

    // Invalid source out <= source in
    clip.source_out_ms = 1000;
    assert_eq!(clip.validate(Some(6000)), Err(TimelineError::InvalidClipRange));

    // Exceeds source duration
    clip.source_out_ms = 7000;
    assert_eq!(clip.validate(Some(6000)), Err(TimelineError::InvalidClipRange));

    // Negative timeline start
    clip.source_out_ms = 5000;
    clip.timeline_start_ms = -10;
    assert_eq!(clip.validate(Some(6000)), Err(TimelineError::InvalidClipRange));

    // Invalid volume > 2.0
    clip.timeline_start_ms = 0;
    clip.volume = 2.5;
    assert_eq!(clip.validate(Some(6000)), Err(TimelineError::InvalidOperation("Invalid volume or transform value".to_string())));

    // Invalid opacity > 1.0
    clip.volume = 1.0;
    clip.transform.opacity = 1.5;
    assert_eq!(clip.validate(Some(6000)), Err(TimelineError::InvalidOperation("Invalid volume or transform value".to_string())));

    // Missing source duration
    clip.transform.opacity = 1.0;
    assert_eq!(clip.validate(None), Err(TimelineError::MediaDurationUnknown));
}

#[test]
fn test_derived_timeline_duration() {
    let doc = TimelineDocument {
        id: "tl1".to_string(),
        project_id: "p1".to_string(),
        name: "Test".to_string(),
        revision: 1,
        canvas: CanvasConfig { width: 1920, height: 1080, frame_rate: 30.0 },
        tracks: vec![
            TimelineTrack {
                id: "tr1".to_string(),
                timeline_id: "tl1".to_string(),
                kind: TrackKind::Video,
                order: 0,
                name: "V1".to_string(),
                muted: false,
                clips: vec![TimelineClip {
                    id: "c1".to_string(),
                    track_id: "tr1".to_string(),
                    artifact_id: "a1".to_string(),
                    source_in_ms: 0,
                    source_out_ms: 3000,
                    timeline_start_ms: 1000,
                    volume: 1.0,
                    muted: false,
                    transform: ClipTransform::default(),
                }],
            },
            TimelineTrack {
                id: "tr2".to_string(),
                timeline_id: "tl1".to_string(),
                kind: TrackKind::Audio,
                order: 1,
                name: "A1".to_string(),
                muted: false,
                clips: vec![TimelineClip {
                    id: "c2".to_string(),
                    track_id: "tr2".to_string(),
                    artifact_id: "a2".to_string(),
                    source_in_ms: 0,
                    source_out_ms: 8000,
                    timeline_start_ms: 0,
                    volume: 1.0,
                    muted: false,
                    transform: ClipTransform::default(),
                }],
            }
        ],
        created_at: "2026-09-10T12:00:00Z".to_string(),
        updated_at: "2026-09-10T12:00:00Z".to_string(),
    };

    assert_eq!(doc.derived_duration_ms(), 8000);
}

#[test]
fn test_clip_transform_defaults() {
    let default_transform = ClipTransform::default();
    assert!(default_transform.is_default());
    assert_eq!(default_transform.x, 0.0);
    assert_eq!(default_transform.y, 0.0);
    assert_eq!(default_transform.scale, 1.0);
    assert_eq!(default_transform.rotation_deg, 0.0);
    assert_eq!(default_transform.opacity, 1.0);

    let mut custom = default_transform.clone();
    custom.scale = 1.5;
    assert!(!custom.is_default());
}
```

- [ ] **Step 2: Run test to verify it fails**
Run: `cargo test --manifest-path src-tauri/Cargo.toml --test timeline_models_tests`
Expected: FAIL (module not found).

- [ ] **Step 3: Implement domain models, error types, and ClipTransform**
Implement `src-tauri/src/timeline/error.rs`:
```rust
use thiserror::Error;

#[derive(Error, Debug, PartialEq, Eq)]
pub enum TimelineError {
    #[error("Timeline not found: {0}")]
    TimelineNotFound(String),
    #[error("Revision conflict: expected {expected}, actual {actual}")]
    RevisionConflict { expected: i64, actual: i64 },
    #[error("Invalid timeline operation: {0}")]
    InvalidOperation(String),
    #[error("Clips cannot overlap on the same track")]
    ClipOverlap,
    #[error("Invalid clip range")]
    InvalidClipRange,
    #[error("Referenced artifact not found: {0}")]
    ArtifactNotFound(String),
    #[error("Source media duration is unknown")]
    MediaDurationUnknown,
    #[error("Preview unavailable: {0}")]
    PreviewUnavailable(String),
    #[error("Render spec is invalid: {0}")]
    RenderSpecInvalid(String),
    #[error("FFmpeg executable unavailable: {0}")]
    FfmpegUnavailable(String),
    #[error("Render failed: {0}")]
    RenderFailed(String),
    #[error("Render output artifact import failed: {0}")]
    RenderOutputImportFailed(String),
    #[error("Database error: {0}")]
    Database(String),
}

impl From<rusqlite::Error> for TimelineError {
    fn from(err: rusqlite::Error) -> Self {
        TimelineError::Database(err.to_string())
    }
}
```

Implement `src-tauri/src/timeline/model.rs`:
```rust
use serde::{Deserialize, Serialize};
use super::error::TimelineError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrackKind {
    Video,
    Audio,
}

impl TrackKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Video => "video",
            Self::Audio => "audio",
        }
    }
}

impl std::str::FromStr for TrackKind {
    type Err = TimelineError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "video" => Ok(Self::Video),
            "audio" => Ok(Self::Audio),
            _ => Err(TimelineError::InvalidOperation(format!("Unknown track kind: {}", s))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipTransform {
    pub x: f64,
    pub y: f64,
    pub scale: f64,
    pub rotation_deg: f64,
    pub opacity: f64,
}

impl Default for ClipTransform {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            scale: 1.0,
            rotation_deg: 0.0,
            opacity: 1.0,
        }
    }
}

impl ClipTransform {
    pub fn is_default(&self) -> bool {
        (self.x.abs() < 1e-6)
            && (self.y.abs() < 1e-6)
            && ((self.scale - 1.0).abs() < 1e-6)
            && (self.rotation_deg.abs() < 1e-6)
            && ((self.opacity - 1.0).abs() < 1e-6)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineClip {
    pub id: String,
    pub track_id: String,
    pub artifact_id: String,
    pub source_in_ms: i64,
    pub source_out_ms: i64,
    pub timeline_start_ms: i64,
    pub volume: f64,
    pub muted: bool,
    pub transform: ClipTransform,
}

impl TimelineClip {
    pub fn duration_ms(&self) -> i64 {
        self.source_out_ms - self.source_in_ms
    }

    pub fn timeline_end_ms(&self) -> i64 {
        self.timeline_start_ms + self.duration_ms()
    }

    pub fn validate(&self, source_duration_ms: Option<i64>) -> Result<(), TimelineError> {
        if self.source_in_ms < 0 || self.source_out_ms <= self.source_in_ms || self.timeline_start_ms < 0 {
            return Err(TimelineError::InvalidClipRange);
        }
        if self.volume < 0.0 || self.volume > 2.0 || self.transform.opacity < 0.0 || self.transform.opacity > 1.0 || self.transform.scale <= 0.0 {
            return Err(TimelineError::InvalidOperation("Invalid volume or transform value".to_string()));
        }
        match source_duration_ms {
            Some(max_dur) => {
                if self.source_out_ms > max_dur {
                    return Err(TimelineError::InvalidClipRange);
                }
            }
            None => return Err(TimelineError::MediaDurationUnknown),
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineTrack {
    pub id: String,
    pub timeline_id: String,
    pub kind: TrackKind,
    pub order: i32,
    pub name: String,
    pub muted: bool,
    pub clips: Vec<TimelineClip>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasConfig {
    pub width: u32,
    pub height: u32,
    pub frame_rate: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineDocument {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub revision: i64,
    pub canvas: CanvasConfig,
    pub tracks: Vec<TimelineTrack>,
    pub created_at: String,
    pub updated_at: String,
}

impl TimelineDocument {
    pub fn derived_duration_ms(&self) -> i64 {
        self.tracks
            .iter()
            .flat_map(|t| t.clips.iter().map(|c| c.timeline_end_ms()))
            .max()
            .unwrap_or(0)
    }
}
```

Implement `src-tauri/src/timeline/mod.rs` and re-export in `src-tauri/src/lib.rs`.

- [ ] **Step 4: Run test to verify it passes**
Run: `cargo test --manifest-path src-tauri/Cargo.toml --test timeline_models_tests`
Expected: PASS.

- [ ] **Step 5: Commit**
```bash
git add src-tauri/src/timeline/ src-tauri/src/lib.rs src-tauri/tests/timeline_models_tests.rs
git commit -m "feat(timeline): implement domain models, invariants, transform contract, and error types"
```

---

### Task 4: Timeline Repository & Transactional History Initialization

**Files:**
- Create: `src-tauri/src/timeline/repository.rs`
- Modify: `src-tauri/src/timeline/mod.rs`
- Test: `src-tauri/tests/timeline_repo_tests.rs`

**Interfaces:**
- Consumes: `rusqlite::{Connection, params}`, actual Phase 1 `Artifact` (8 columns), `ArtifactRepository::insert`, `ArtifactRepository::link_to_project`, `MetadataRepository::insert`
- Produces: `TimelineRepository::create`, `TimelineRepository::get_by_id`, `TimelineRepository::list_by_project`, `TimelineRepository::delete`

- [ ] **Step 1: Write failing test for TimelineRepository persistence and transactional initial history snapshot**
Create `src-tauri/tests/timeline_repo_tests.rs`:
```rust
use rusqlite::Connection;
use openvideoai_lib::db::migrations::run_all_migrations;
use openvideoai_lib::projects::model::Project;
use openvideoai_lib::projects::repository::ProjectRepository;
use openvideoai_lib::artifacts::model::{Artifact, ArtifactKind};
use openvideoai_lib::artifacts::repository::ArtifactRepository;
use openvideoai_lib::artifacts::metadata::{MediaMetadata, MetadataRepository};
use openvideoai_lib::timeline::model::*;
use openvideoai_lib::timeline::repository::TimelineRepository;

#[test]
fn test_create_initializes_revision_1_and_history_snapshot_0() {
    let mut conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    run_all_migrations(&mut conn).unwrap();

    let project = Project::new("p1".to_string(), "Test Project".to_string());
    ProjectRepository::create(&conn, &project).unwrap();

    let canvas = CanvasConfig { width: 1920, height: 1080, frame_rate: 30.0 };
    let created = TimelineRepository::create(&mut conn, "tl1", "p1", "Main Timeline", canvas).unwrap();

    // Invariant: initial revision = 1
    assert_eq!(created.revision, 1);
    assert_eq!(created.name, "Main Timeline");
    assert_eq!(created.tracks.len(), 0);

    // Invariant: history_cursor = 0 on timeline row
    let cursor: i64 = conn.query_row("SELECT history_cursor FROM timelines WHERE id = 'tl1';", [], |r| r.get(0)).unwrap();
    assert_eq!(cursor, 0);

    // Invariant: history snapshot 0 exists in timeline_history table
    let (history_index, snapshot_json, op_type): (i64, String, String) = conn.query_row(
        "SELECT history_index, snapshot_json, operation_type FROM timeline_history WHERE timeline_id = 'tl1';",
        [],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    ).unwrap();

    assert_eq!(history_index, 0);
    assert_eq!(op_type, "create");
    let parsed: serde_json::Value = serde_json::from_str(&snapshot_json).unwrap();
    assert_eq!(parsed["schemaVersion"], 1);
    assert_eq!(parsed["timeline"]["id"], "tl1");
    assert_eq!(parsed["timeline"]["revision"], 1);
}

#[test]
fn test_get_timeline_assembles_tracks_and_clips_in_order() {
    let mut conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    run_all_migrations(&mut conn).unwrap();

    let project = Project::new("p1".to_string(), "Test Project".to_string());
    ProjectRepository::create(&conn, &project).unwrap();

    // Insert artifact using actual Phase 1 8-column schema & repository
    let artifact = Artifact {
        id: "art-video-1".to_string(),
        sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string(),
        kind: ArtifactKind::Video,
        mime_type: Some("video/mp4".to_string()),
        size_bytes: 1048576,
        storage_key: "videos/art-video-1.mp4".to_string(),
        original_filename: "clip1.mp4".to_string(),
        created_at: "2026-09-10T12:00:00Z".to_string(),
    };
    ArtifactRepository::insert(&conn, &artifact).unwrap();
    ArtifactRepository::link_to_project(&conn, "p1", &artifact.id, "2026-09-10T12:00:00Z").unwrap();

    let meta = MediaMetadata {
        artifact_id: artifact.id.clone(),
        probe_status: "ok".to_string(),
        duration_ms: Some(10000),
        width: Some(1920),
        height: Some(1080),
        video_codec: Some("h264".to_string()),
        audio_codec: Some("aac".to_string()),
        frame_rate: Some(30.0),
        sample_rate: Some(48000),
        channels: Some(2),
        probe_error: None,
    };
    MetadataRepository::insert(&conn, &meta).unwrap();

    let canvas = CanvasConfig { width: 1920, height: 1080, frame_rate: 30.0 };
    TimelineRepository::create(&mut conn, "tl1", "p1", "Main Timeline", canvas).unwrap();

    // Insert tracks out of order into SQLite to verify order_index ASC ordering
    conn.execute(
        "INSERT INTO timeline_tracks (id, timeline_id, kind, order_index, name, muted) VALUES ('tr-audio', 'tl1', 'audio', 1, 'A1', 0);",
        [],
    ).unwrap();
    conn.execute(
        "INSERT INTO timeline_tracks (id, timeline_id, kind, order_index, name, muted) VALUES ('tr-video', 'tl1', 'video', 0, 'V1', 0);",
        [],
    ).unwrap();

    // Insert clips out of order to verify timeline_start_ms ASC ordering
    conn.execute(
        "INSERT INTO timeline_clips (id, track_id, artifact_id, source_in_ms, source_out_ms, timeline_start_ms, volume, muted, position_x, position_y, scale, rotation_deg, opacity)
         VALUES ('c2', 'tr-video', 'art-video-1', 0, 3000, 5000, 1.0, 0, 0.0, 0.0, 1.0, 0.0, 1.0);",
        [],
    ).unwrap();
    conn.execute(
        "INSERT INTO timeline_clips (id, track_id, artifact_id, source_in_ms, source_out_ms, timeline_start_ms, volume, muted, position_x, position_y, scale, rotation_deg, opacity)
         VALUES ('c1', 'tr-video', 'art-video-1', 0, 4000, 0, 1.0, 0, 0.0, 0.0, 1.0, 0.0, 1.0);",
        [],
    ).unwrap();

    let fetched = TimelineRepository::get_by_id(&conn, "tl1").unwrap().expect("Timeline must exist");
    assert_eq!(fetched.tracks.len(), 2);
    assert_eq!(fetched.tracks[0].name, "V1");
    assert_eq!(fetched.tracks[0].order, 0);
    assert_eq!(fetched.tracks[1].name, "A1");
    assert_eq!(fetched.tracks[1].order, 1);

    assert_eq!(fetched.tracks[0].clips.len(), 2);
    assert_eq!(fetched.tracks[0].clips[0].id, "c1");
    assert_eq!(fetched.tracks[0].clips[0].timeline_start_ms, 0);
    assert_eq!(fetched.tracks[0].clips[1].id, "c2");
    assert_eq!(fetched.tracks[0].clips[1].timeline_start_ms, 5000);
}
```

- [ ] **Step 2: Run test to verify it fails**
Run: `cargo test --manifest-path src-tauri/Cargo.toml --test timeline_repo_tests`
Expected: FAIL.

- [ ] **Step 3: Implement `TimelineRepository` with transactional history initialization**
Implement `src-tauri/src/timeline/repository.rs`:
```rust
use rusqlite::{params, Connection};
use chrono::Utc;
use serde_json::json;
use super::model::*;
use super::error::TimelineError;

pub struct TimelineRepository;

impl TimelineRepository {
    pub fn create(
        conn: &mut Connection,
        id: &str,
        project_id: &str,
        name: &str,
        canvas: CanvasConfig,
    ) -> Result<TimelineDocument, TimelineError> {
        let now = Utc::now().to_rfc3339();
        let tx = conn.transaction()?;

        tx.execute(
            "INSERT INTO timelines (id, project_id, name, revision, width, height, frame_rate, history_cursor, created_at, updated_at)
             VALUES (?1, ?2, ?3, 1, ?4, ?5, ?6, 0, ?7, ?7);",
            params![id, project_id, name, canvas.width, canvas.height, canvas.frame_rate, now],
        )?;

        let initial_doc = TimelineDocument {
            id: id.to_string(),
            project_id: project_id.to_string(),
            name: name.to_string(),
            revision: 1,
            canvas,
            tracks: Vec::new(),
            created_at: now.clone(),
            updated_at: now.clone(),
        };

        let snapshot = json!({
            "schemaVersion": 1,
            "timeline": initial_doc,
        });

        tx.execute(
            "INSERT INTO timeline_history (timeline_id, history_index, snapshot_json, operation_type, created_at)
             VALUES (?1, 0, ?2, 'create', ?3);",
            params![id, snapshot.to_string(), now],
        )?;

        tx.commit()?;
        Ok(initial_doc)
    }

    pub fn get_by_id(conn: &Connection, id: &str) -> Result<Option<TimelineDocument>, TimelineError> {
        let mut stmt = conn.prepare(
            "SELECT id, project_id, name, revision, width, height, frame_rate, created_at, updated_at
             FROM timelines WHERE id = ?1;",
        )?;

        let doc = stmt.query_row(params![id], |row| {
            let width: u32 = row.get(4)?;
            let height: u32 = row.get(5)?;
            let frame_rate: f64 = row.get(6)?;
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                CanvasConfig { width, height, frame_rate },
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
            ))
        });

        let (tl_id, project_id, name, revision, canvas, created_at, updated_at) = match doc {
            Ok(v) => v,
            Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
            Err(e) => return Err(TimelineError::Database(e.to_string())),
        };

        // Load tracks in order_index ASC
        let mut track_stmt = conn.prepare(
            "SELECT id, timeline_id, kind, order_index, name, muted
             FROM timeline_tracks WHERE timeline_id = ?1 ORDER BY order_index ASC;",
        )?;

        let track_rows = track_stmt.query_map(params![&tl_id], |row| {
            let kind_str: String = row.get(2)?;
            let kind = kind_str.parse::<TrackKind>().map_err(|e| rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, Box::new(e)))?;
            let muted_int: i32 = row.get(5)?;
            Ok(TimelineTrack {
                id: row.get(0)?,
                timeline_id: row.get(1)?,
                kind,
                order: row.get(3)?,
                name: row.get(4)?,
                muted: muted_int != 0,
                clips: Vec::new(),
            })
        })?;

        let mut tracks = Vec::new();
        for t in track_rows {
            tracks.push(t?);
        }

        // Load clips for each track in timeline_start_ms ASC
        let mut clip_stmt = conn.prepare(
            "SELECT id, track_id, artifact_id, source_in_ms, source_out_ms, timeline_start_ms, volume, muted, position_x, position_y, scale, rotation_deg, opacity
             FROM timeline_clips WHERE track_id = ?1 ORDER BY timeline_start_ms ASC;",
        )?;

        for track in &mut tracks {
            let clip_rows = clip_stmt.query_map(params![&track.id], |row| {
                let muted_int: i32 = row.get(7)?;
                Ok(TimelineClip {
                    id: row.get(0)?,
                    track_id: row.get(1)?,
                    artifact_id: row.get(2)?,
                    source_in_ms: row.get(3)?,
                    source_out_ms: row.get(4)?,
                    timeline_start_ms: row.get(5)?,
                    volume: row.get(6)?,
                    muted: muted_int != 0,
                    transform: ClipTransform {
                        x: row.get(8)?,
                        y: row.get(9)?,
                        scale: row.get(10)?,
                        rotation_deg: row.get(11)?,
                        opacity: row.get(12)?,
                    },
                })
            })?;

            for c in clip_rows {
                track.clips.push(c?);
            }
        }

        Ok(Some(TimelineDocument {
            id: tl_id,
            project_id,
            name,
            revision,
            canvas,
            tracks,
            created_at,
            updated_at,
        }))
    }

    pub fn list_by_project(conn: &Connection, project_id: &str) -> Result<Vec<TimelineDocument>, TimelineError> {
        let mut stmt = conn.prepare("SELECT id FROM timelines WHERE project_id = ?1 ORDER BY created_at ASC;")?;
        let rows = stmt.query_map(params![project_id], |row| row.get::<_, String>(0))?;
        let mut docs = Vec::new();
        for id_res in rows {
            let id = id_res?;
            if let Some(doc) = Self::get_by_id(conn, &id)? {
                docs.push(doc);
            }
        }
        Ok(docs)
    }

    pub fn delete(conn: &Connection, id: &str) -> Result<bool, TimelineError> {
        let count = conn.execute("DELETE FROM timelines WHERE id = ?1;", params![id])?;
        Ok(count > 0)
    }
}
```

- [ ] **Step 4: Run test to verify it passes**
Run: `cargo test --manifest-path src-tauri/Cargo.toml --test timeline_repo_tests`
Expected: PASS.

- [ ] **Step 5: Commit**
```bash
git add src-tauri/src/timeline/repository.rs src-tauri/src/timeline/mod.rs src-tauri/tests/timeline_repo_tests.rs
git commit -m "feat(timeline): implement TimelineRepository with transactional history snapshot 0 initialization"
```

---

### Task 5: Timeline Mutation Engine & Overlap Validation

**Files:**
- Create: `src-tauri/src/timeline/mutation.rs`
- Create: `src-tauri/src/timeline/service.rs`
- Modify: `src-tauri/src/timeline/mod.rs`
- Test: `src-tauri/tests/timeline_mutation_tests.rs`

**Interfaces:**
- Consumes: `TimelineRepository`, `MetadataRepository`, `TimelineMutation`, actual Phase 1 `Artifact` and `MediaMetadata`
- Produces: `TimelineService::apply_mutation(&self, conn, timeline_id, expected_rev, mutation) -> Result<TimelineDocument, TimelineError>`

- [ ] **Step 1: Write failing tests for same-track overlap rejection, cross-track overlap acceptance, split/trim math, and optimistic revision checks**
Create `src-tauri/tests/timeline_mutation_tests.rs`:
```rust
use rusqlite::Connection;
use openvideoai_lib::db::migrations::run_all_migrations;
use openvideoai_lib::projects::model::Project;
use openvideoai_lib::projects::repository::ProjectRepository;
use openvideoai_lib::artifacts::model::{Artifact, ArtifactKind};
use openvideoai_lib::artifacts::repository::ArtifactRepository;
use openvideoai_lib::artifacts::metadata::{MediaMetadata, MetadataRepository};
use openvideoai_lib::timeline::model::*;
use openvideoai_lib::timeline::repository::TimelineRepository;
use openvideoai_lib::timeline::service::TimelineService;
use openvideoai_lib::timeline::mutation::TimelineMutation;
use openvideoai_lib::timeline::error::TimelineError;

fn setup_test_db() -> (Connection, String, String) {
    let mut conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    run_all_migrations(&mut conn).unwrap();

    let project = Project::new("p1".to_string(), "Test Project".to_string());
    ProjectRepository::create(&conn, &project).unwrap();

    let artifact = Artifact {
        id: "art-video-1".to_string(),
        sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string(),
        kind: ArtifactKind::Video,
        mime_type: Some("video/mp4".to_string()),
        size_bytes: 1048576,
        storage_key: "videos/art-video-1.mp4".to_string(),
        original_filename: "clip1.mp4".to_string(),
        created_at: "2026-09-10T12:00:00Z".to_string(),
    };
    ArtifactRepository::insert(&conn, &artifact).unwrap();
    ArtifactRepository::link_to_project(&conn, "p1", &artifact.id, "2026-09-10T12:00:00Z").unwrap();

    let meta = MediaMetadata {
        artifact_id: artifact.id.clone(),
        probe_status: "ok".to_string(),
        duration_ms: Some(20000), // 20s source
        width: Some(1920),
        height: Some(1080),
        video_codec: Some("h264".to_string()),
        audio_codec: Some("aac".to_string()),
        frame_rate: Some(30.0),
        sample_rate: Some(48000),
        channels: Some(2),
        probe_error: None,
    };
    MetadataRepository::insert(&conn, &meta).unwrap();

    let canvas = CanvasConfig { width: 1920, height: 1080, frame_rate: 30.0 };
    TimelineRepository::create(&mut conn, "tl1", "p1", "Main Timeline", canvas).unwrap();

    (conn, "tl1".to_string(), artifact.id)
}

#[test]
fn test_same_track_overlap_rejected() {
    let (mut conn, tl_id, art_id) = setup_test_db();
    let service = TimelineService::new();

    // Add track V1 (rev 1 -> 2)
    let doc = service.apply_mutation(&mut conn, &tl_id, 1, TimelineMutation::AddTrack {
        kind: TrackKind::Video,
        name: "V1".to_string(),
    }).unwrap();
    let track_id = doc.tracks[0].id.clone();

    // Add clip 1 at timeline 0..5000ms (rev 2 -> 3)
    let doc = service.apply_mutation(&mut conn, &tl_id, 2, TimelineMutation::AddClip {
        track_id: track_id.clone(),
        artifact_id: art_id.clone(),
        timeline_start_ms: 0,
        source_in_ms: Some(0),
        source_out_ms: Some(5000),
    }).unwrap();
    assert_eq!(doc.revision, 3);

    // Add overlapping clip 2 on SAME track at timeline 3000..8000ms (overlaps 3000..5000)
    let err = service.apply_mutation(&mut conn, &tl_id, 3, TimelineMutation::AddClip {
        track_id: track_id.clone(),
        artifact_id: art_id.clone(),
        timeline_start_ms: 3000,
        source_in_ms: Some(0),
        source_out_ms: Some(5000),
    }).unwrap_err();

    assert_eq!(err, TimelineError::ClipOverlap);
}

#[test]
fn test_cross_track_overlap_accepted() {
    let (mut conn, tl_id, art_id) = setup_test_db();
    let service = TimelineService::new();

    // Add track V1 and track V2
    let doc = service.apply_mutation(&mut conn, &tl_id, 1, TimelineMutation::AddTrack {
        kind: TrackKind::Video,
        name: "V1".to_string(),
    }).unwrap();
    let v1_id = doc.tracks[0].id.clone();

    let doc = service.apply_mutation(&mut conn, &tl_id, 2, TimelineMutation::AddTrack {
        kind: TrackKind::Video,
        name: "V2".to_string(),
    }).unwrap();
    let v2_id = doc.tracks[1].id.clone();

    // Add clip on V1 at 0..5000ms
    service.apply_mutation(&mut conn, &tl_id, 3, TimelineMutation::AddClip {
        track_id: v1_id,
        artifact_id: art_id.clone(),
        timeline_start_ms: 0,
        source_in_ms: Some(0),
        source_out_ms: Some(5000),
    }).unwrap();

    // Add clip on V2 at 3000..8000ms (cross-track overlap is valid)
    let doc = service.apply_mutation(&mut conn, &tl_id, 4, TimelineMutation::AddClip {
        track_id: v2_id,
        artifact_id: art_id.clone(),
        timeline_start_ms: 3000,
        source_in_ms: Some(0),
        source_out_ms: Some(5000),
    }).unwrap();

    assert_eq!(doc.revision, 5);
    assert_eq!(doc.derived_duration_ms(), 8000);
}

#[test]
fn test_split_clip_math() {
    let (mut conn, tl_id, art_id) = setup_test_db();
    let service = TimelineService::new();

    // Add track V1
    let doc = service.apply_mutation(&mut conn, &tl_id, 1, TimelineMutation::AddTrack {
        kind: TrackKind::Video,
        name: "V1".to_string(),
    }).unwrap();
    let v1_id = doc.tracks[0].id.clone();

    // Add clip: source 2000..14000 (12s dur), timelineStart 5000..17000
    let doc = service.apply_mutation(&mut conn, &tl_id, 2, TimelineMutation::AddClip {
        track_id: v1_id,
        artifact_id: art_id.clone(),
        timeline_start_ms: 5000,
        source_in_ms: Some(2000),
        source_out_ms: Some(14000),
    }).unwrap();
    let original_clip_id = doc.tracks[0].clips[0].id.clone();

    // Split at timeline 10000ms (relative offset = 5000ms, source split = 7000ms)
    let doc = service.apply_mutation(&mut conn, &tl_id, 3, TimelineMutation::SplitClip {
        clip_id: original_clip_id,
        split_timeline_ms: 10000,
    }).unwrap();

    assert_eq!(doc.tracks[0].clips.len(), 2);
    let clip_a = &doc.tracks[0].clips[0];
    let clip_b = &doc.tracks[0].clips[1];

    assert_eq!(clip_a.timeline_start_ms, 5000);
    assert_eq!(clip_a.source_in_ms, 2000);
    assert_eq!(clip_a.source_out_ms, 7000);
    assert_eq!(clip_a.duration_ms(), 5000);

    assert_eq!(clip_b.timeline_start_ms, 10000);
    assert_eq!(clip_b.source_in_ms, 7000);
    assert_eq!(clip_b.source_out_ms, 14000);
    assert_eq!(clip_b.duration_ms(), 7000);
}

#[test]
fn test_trim_clip_validates_source_bounds() {
    let (mut conn, tl_id, art_id) = setup_test_db();
    let service = TimelineService::new();

    let doc = service.apply_mutation(&mut conn, &tl_id, 1, TimelineMutation::AddTrack {
        kind: TrackKind::Video,
        name: "V1".to_string(),
    }).unwrap();
    let v1_id = doc.tracks[0].id.clone();

    let doc = service.apply_mutation(&mut conn, &tl_id, 2, TimelineMutation::AddClip {
        track_id: v1_id,
        artifact_id: art_id.clone(),
        timeline_start_ms: 0,
        source_in_ms: Some(0),
        source_out_ms: Some(5000),
    }).unwrap();
    let clip_id = doc.tracks[0].clips[0].id.clone();

    // Trim beyond source media duration (source media duration is 20000ms)
    let err = service.apply_mutation(&mut conn, &tl_id, 3, TimelineMutation::TrimClip {
        clip_id,
        source_in_ms: 0,
        source_out_ms: 25000,
    }).unwrap_err();

    assert_eq!(err, TimelineError::InvalidClipRange);
}

#[test]
fn test_mutation_enforces_optimistic_revision_concurrency() {
    let (mut conn, tl_id, _) = setup_test_db();
    let service = TimelineService::new();

    // Current revision is 1. Passing expected_revision = 99 must fail.
    let err = service.apply_mutation(&mut conn, &tl_id, 99, TimelineMutation::AddTrack {
        kind: TrackKind::Video,
        name: "V1".to_string(),
    }).unwrap_err();

    assert_eq!(err, TimelineError::RevisionConflict { expected: 99, actual: 1 });
}
```

- [ ] **Step 2: Run test to verify it fails**
Run: `cargo test --manifest-path src-tauri/Cargo.toml --test timeline_mutation_tests`
Expected: FAIL.

- [ ] **Step 3: Implement `TimelineMutation` and `TimelineService`**
Implement `src-tauri/src/timeline/mutation.rs`:
```rust
use serde::{Deserialize, Serialize};
use super::model::{ClipTransform, TrackKind};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TimelineMutation {
    AddTrack {
        kind: TrackKind,
        name: String,
    },
    ReorderTrack {
        track_id: String,
        new_order: i32,
    },
    DeleteTrack {
        track_id: String,
    },
    AddClip {
        track_id: String,
        artifact_id: String,
        timeline_start_ms: i64,
        source_in_ms: Option<i64>,
        source_out_ms: Option<i64>,
    },
    RemoveClip {
        clip_id: String,
    },
    MoveClip {
        clip_id: String,
        new_track_id: Option<String>,
        new_start_ms: i64,
    },
    TrimClip {
        clip_id: String,
        source_in_ms: i64,
        source_out_ms: i64,
    },
    SplitClip {
        clip_id: String,
        split_timeline_ms: i64,
    },
    SetClipAudio {
        clip_id: String,
        volume: f64,
        muted: bool,
    },
    SetClipTransform {
        clip_id: String,
        transform: ClipTransform,
    },
    UpdateCanvas {
        width: u32,
        height: u32,
        frame_rate: f64,
    },
}

pub fn check_same_track_overlap(
    clips: &[super::model::TimelineClip],
    new_start_ms: i64,
    new_end_ms: i64,
    ignore_clip_id: Option<&str>,
) -> bool {
    for c in clips {
        if Some(c.id.as_str()) == ignore_clip_id {
            continue;
        }
        let c_start = c.timeline_start_ms;
        let c_end = c.timeline_end_ms();
        if new_start_ms < c_end && new_end_ms > c_start {
            return true;
        }
    }
    false
}
```

Implement `src-tauri/src/timeline/service.rs`:
```rust
use rusqlite::{params, Connection};
use chrono::Utc;
use serde_json::json;
use uuid::Uuid;
use crate::artifacts::metadata::MetadataRepository;
use super::model::*;
use super::mutation::*;
use super::repository::TimelineRepository;
use super::error::TimelineError;

pub struct TimelineService;

impl TimelineService {
    pub fn new() -> Self {
        Self
    }

    pub fn apply_mutation(
        &self,
        conn: &mut Connection,
        timeline_id: &str,
        expected_revision: i64,
        mutation: TimelineMutation,
    ) -> Result<TimelineDocument, TimelineError> {
        let current = TimelineRepository::get_by_id(conn, timeline_id)?
            .ok_or_else(|| TimelineError::TimelineNotFound(timeline_id.to_string()))?;

        if current.revision != expected_revision {
            return Err(TimelineError::RevisionConflict {
                expected: expected_revision,
                actual: current.revision,
            });
        }

        let tx = conn.transaction()?;

        let op_name = match &mutation {
            TimelineMutation::AddTrack { kind, name } => {
                let max_order: Option<i32> = tx.query_row(
                    "SELECT MAX(order_index) FROM timeline_tracks WHERE timeline_id = ?1;",
                    params![timeline_id],
                    |r| r.get(0),
                )?;
                let next_order = max_order.map(|o| o + 1).unwrap_or(0);
                let track_id = format!("track-{}", Uuid::new_v4());
                tx.execute(
                    "INSERT INTO timeline_tracks (id, timeline_id, kind, order_index, name, muted)
                     VALUES (?1, ?2, ?3, ?4, ?5, 0);",
                    params![track_id, timeline_id, kind.as_str(), next_order, name],
                )?;
                "add_track"
            }
            TimelineMutation::ReorderTrack { track_id, new_order } => {
                tx.execute(
                    "UPDATE timeline_tracks SET order_index = ?1 WHERE id = ?2 AND timeline_id = ?3;",
                    params![new_order, track_id, timeline_id],
                )?;
                "reorder_track"
            }
            TimelineMutation::DeleteTrack { track_id } => {
                tx.execute(
                    "DELETE FROM timeline_tracks WHERE id = ?1 AND timeline_id = ?2;",
                    params![track_id, timeline_id],
                )?;
                "delete_track"
            }
            TimelineMutation::AddClip { track_id, artifact_id, timeline_start_ms, source_in_ms, source_out_ms } => {
                let meta = MetadataRepository::get_by_artifact_id(&tx, artifact_id)?
                    .ok_or_else(|| TimelineError::ArtifactNotFound(artifact_id.clone()))?;
                let duration_ms = meta.duration_ms.ok_or(TimelineError::MediaDurationUnknown)?;

                let in_ms = source_in_ms.unwrap_or(0);
                let out_ms = source_out_ms.unwrap_or(duration_ms);

                if in_ms < 0 || out_ms <= in_ms || out_ms > duration_ms || *timeline_start_ms < 0 {
                    return Err(TimelineError::InvalidClipRange);
                }

                let clip_end_ms = *timeline_start_ms + (out_ms - in_ms);

                // Check same-track collision
                let track = current.tracks.iter().find(|t| &t.id == track_id)
                    .ok_or_else(|| TimelineError::InvalidOperation("Track not found".to_string()))?;
                if check_same_track_overlap(&track.clips, *timeline_start_ms, clip_end_ms, None) {
                    return Err(TimelineError::ClipOverlap);
                }

                let clip_id = format!("clip-{}", Uuid::new_v4());
                tx.execute(
                    "INSERT INTO timeline_clips (id, track_id, artifact_id, source_in_ms, source_out_ms, timeline_start_ms, volume, muted, position_x, position_y, scale, rotation_deg, opacity)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1.0, 0, 0.0, 0.0, 1.0, 0.0, 1.0);",
                    params![clip_id, track_id, artifact_id, in_ms, out_ms, timeline_start_ms],
                )?;
                "add_clip"
            }
            TimelineMutation::RemoveClip { clip_id } => {
                tx.execute("DELETE FROM timeline_clips WHERE id = ?1;", params![clip_id])?;
                "remove_clip"
            }
            TimelineMutation::MoveClip { clip_id, new_track_id, new_start_ms } => {
                if *new_start_ms < 0 {
                    return Err(TimelineError::InvalidClipRange);
                }
                let mut target_track_id = String::new();
                let mut clip_opt = None;
                for t in &current.tracks {
                    for c in &t.clips {
                        if &c.id == clip_id {
                            clip_opt = Some(c.clone());
                            target_track_id = new_track_id.clone().unwrap_or_else(|| t.id.clone());
                            break;
                        }
                    }
                }
                let clip = clip_opt.ok_or_else(|| TimelineError::InvalidOperation("Clip not found".to_string()))?;
                let clip_end = *new_start_ms + clip.duration_ms();

                let target_track = current.tracks.iter().find(|t| t.id == target_track_id)
                    .ok_or_else(|| TimelineError::InvalidOperation("Target track not found".to_string()))?;

                if check_same_track_overlap(&target_track.clips, *new_start_ms, clip_end, Some(clip_id)) {
                    return Err(TimelineError::ClipOverlap);
                }

                tx.execute(
                    "UPDATE timeline_clips SET track_id = ?1, timeline_start_ms = ?2 WHERE id = ?3;",
                    params![target_track_id, new_start_ms, clip_id],
                )?;
                "move_clip"
            }
            TimelineMutation::TrimClip { clip_id, source_in_ms, source_out_ms } => {
                let mut found = None;
                for t in &current.tracks {
                    for c in &t.clips {
                        if &c.id == clip_id {
                            found = Some((t.clone(), c.clone()));
                            break;
                        }
                    }
                }
                let (track, clip) = found.ok_or_else(|| TimelineError::InvalidOperation("Clip not found".to_string()))?;
                let meta = MetadataRepository::get_by_artifact_id(&tx, &clip.artifact_id)?
                    .ok_or_else(|| TimelineError::ArtifactNotFound(clip.artifact_id.clone()))?;
                let max_dur = meta.duration_ms.ok_or(TimelineError::MediaDurationUnknown)?;

                if *source_in_ms < 0 || *source_out_ms <= *source_in_ms || *source_out_ms > max_dur {
                    return Err(TimelineError::InvalidClipRange);
                }

                let new_end_ms = clip.timeline_start_ms + (*source_out_ms - *source_in_ms);
                if check_same_track_overlap(&track.clips, clip.timeline_start_ms, new_end_ms, Some(clip_id)) {
                    return Err(TimelineError::ClipOverlap);
                }

                tx.execute(
                    "UPDATE timeline_clips SET source_in_ms = ?1, source_out_ms = ?2 WHERE id = ?3;",
                    params![source_in_ms, source_out_ms, clip_id],
                )?;
                "trim_clip"
            }
            TimelineMutation::SplitClip { clip_id, split_timeline_ms } => {
                let mut found = None;
                for t in &current.tracks {
                    for c in &t.clips {
                        if &c.id == clip_id {
                            found = Some((t.clone(), c.clone()));
                            break;
                        }
                    }
                }
                let (track, clip) = found.ok_or_else(|| TimelineError::InvalidOperation("Clip not found".to_string()))?;
                if *split_timeline_ms <= clip.timeline_start_ms || *split_timeline_ms >= clip.timeline_end_ms() {
                    return Err(TimelineError::InvalidClipRange);
                }

                let rel_offset = *split_timeline_ms - clip.timeline_start_ms;
                let source_split = clip.source_in_ms + rel_offset;

                // Update Clip A
                tx.execute(
                    "UPDATE timeline_clips SET source_out_ms = ?1 WHERE id = ?2;",
                    params![source_split, clip_id],
                )?;

                // Insert Clip B
                let clip_b_id = format!("clip-{}", Uuid::new_v4());
                tx.execute(
                    "INSERT INTO timeline_clips (id, track_id, artifact_id, source_in_ms, source_out_ms, timeline_start_ms, volume, muted, position_x, position_y, scale, rotation_deg, opacity)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13);",
                    params![
                        clip_b_id,
                        track.id,
                        clip.artifact_id,
                        source_split,
                        clip.source_out_ms,
                        split_timeline_ms,
                        clip.volume,
                        clip.muted as i32,
                        clip.transform.x,
                        clip.transform.y,
                        clip.transform.scale,
                        clip.transform.rotation_deg,
                        clip.transform.opacity,
                    ],
                )?;
                "split_clip"
            }
            TimelineMutation::SetClipAudio { clip_id, volume, muted } => {
                if *volume < 0.0 || *volume > 2.0 {
                    return Err(TimelineError::InvalidOperation("Volume must be in [0.0, 2.0]".to_string()));
                }
                tx.execute(
                    "UPDATE timeline_clips SET volume = ?1, muted = ?2 WHERE id = ?3;",
                    params![volume, *muted as i32, clip_id],
                )?;
                "set_clip_audio"
            }
            TimelineMutation::SetClipTransform { clip_id, transform } => {
                if transform.scale <= 0.0 || transform.opacity < 0.0 || transform.opacity > 1.0 {
                    return Err(TimelineError::InvalidOperation("Invalid transform parameters".to_string()));
                }
                tx.execute(
                    "UPDATE timeline_clips SET position_x = ?1, position_y = ?2, scale = ?3, rotation_deg = ?4, opacity = ?5 WHERE id = ?6;",
                    params![transform.x, transform.y, transform.scale, transform.rotation_deg, transform.opacity, clip_id],
                )?;
                "set_clip_transform"
            }
            TimelineMutation::UpdateCanvas { width, height, frame_rate } => {
                tx.execute(
                    "UPDATE timelines SET width = ?1, height = ?2, frame_rate = ?3 WHERE id = ?4;",
                    params![width, height, frame_rate, timeline_id],
                )?;
                "update_canvas"
            }
        };

        // Monotonic revision + 1
        let new_revision = current.revision + 1;
        let now = Utc::now().to_rfc3339();

        // Load updated document before commit to create snapshot
        let updated_doc = Self::load_document_tx(&tx, timeline_id, new_revision, &now)?;

        // History handling:
        // 1. Invalidate redo branch: delete entries where history_index > current cursor
        let cursor: i64 = tx.query_row("SELECT history_cursor FROM timelines WHERE id = ?1;", params![timeline_id], |r| r.get(0))?;
        tx.execute("DELETE FROM timeline_history WHERE timeline_id = ?1 AND history_index > ?2;", params![timeline_id, cursor])?;

        // 2. Insert new snapshot at cursor + 1
        let new_cursor = cursor + 1;
        let snapshot = json!({
            "schemaVersion": 1,
            "timeline": updated_doc,
        });
        tx.execute(
            "INSERT INTO timeline_history (timeline_id, history_index, snapshot_json, operation_type, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5);",
            params![timeline_id, new_cursor, snapshot.to_string(), op_name, now],
        )?;

        // 3. Prune oldest snapshots if total > 50, but NEVER prune new_cursor
        let total_snapshots: i64 = tx.query_row("SELECT count(*) FROM timeline_history WHERE timeline_id = ?1;", params![timeline_id], |r| r.get(0))?;
        if total_snapshots > 50 {
            let to_remove = total_snapshots - 50;
            tx.execute(
                "DELETE FROM timeline_history WHERE timeline_id = ?1 AND history_index IN (
                    SELECT history_index FROM timeline_history WHERE timeline_id = ?1 AND history_index != ?2 ORDER BY history_index ASC LIMIT ?3
                );",
                params![timeline_id, new_cursor, to_remove],
            )?;
        }

        // 4. Update timeline revision and history cursor
        tx.execute(
            "UPDATE timelines SET revision = ?1, history_cursor = ?2, updated_at = ?3 WHERE id = ?4;",
            params![new_revision, new_cursor, now, timeline_id],
        )?;

        tx.commit()?;
        Ok(updated_doc)
    }

    fn load_document_tx(tx: &rusqlite::Transaction, timeline_id: &str, revision: i64, updated_at: &str) -> Result<TimelineDocument, TimelineError> {
        let (project_id, name, width, height, frame_rate, created_at): (String, String, u32, u32, f64, String) = tx.query_row(
            "SELECT project_id, name, width, height, frame_rate, created_at FROM timelines WHERE id = ?1;",
            params![timeline_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
        )?;

        let mut track_stmt = tx.prepare(
            "SELECT id, timeline_id, kind, order_index, name, muted FROM timeline_tracks WHERE timeline_id = ?1 ORDER BY order_index ASC;",
        )?;
        let track_rows = track_stmt.query_map(params![timeline_id], |r| {
            let kind_str: String = r.get(2)?;
            let kind = kind_str.parse::<TrackKind>().unwrap_or(TrackKind::Video);
            let muted_int: i32 = r.get(5)?;
            Ok(TimelineTrack {
                id: r.get(0)?,
                timeline_id: r.get(1)?,
                kind,
                order: r.get(3)?,
                name: r.get(4)?,
                muted: muted_int != 0,
                clips: Vec::new(),
            })
        })?;

        let mut tracks = Vec::new();
        for t in track_rows { tracks.push(t?); }

        let mut clip_stmt = tx.prepare(
            "SELECT id, track_id, artifact_id, source_in_ms, source_out_ms, timeline_start_ms, volume, muted, position_x, position_y, scale, rotation_deg, opacity
             FROM timeline_clips WHERE track_id = ?1 ORDER BY timeline_start_ms ASC;",
        )?;

        for track in &mut tracks {
            let clip_rows = clip_stmt.query_map(params![&track.id], |r| {
                let muted_int: i32 = r.get(7)?;
                Ok(TimelineClip {
                    id: r.get(0)?,
                    track_id: r.get(1)?,
                    artifact_id: r.get(2)?,
                    source_in_ms: r.get(3)?,
                    source_out_ms: r.get(4)?,
                    timeline_start_ms: r.get(5)?,
                    volume: r.get(6)?,
                    muted: muted_int != 0,
                    transform: ClipTransform {
                        x: r.get(8)?,
                        y: r.get(9)?,
                        scale: r.get(10)?,
                        rotation_deg: r.get(11)?,
                        opacity: r.get(12)?,
                    },
                })
            })?;
            for c in clip_rows { track.clips.push(c?); }
        }

        Ok(TimelineDocument {
            id: timeline_id.to_string(),
            project_id,
            name,
            revision,
            canvas: CanvasConfig { width, height, frame_rate },
            tracks,
            created_at,
            updated_at: updated_at.to_string(),
        })
    }
}
```

- [ ] **Step 4: Run test to verify it passes**
Run: `cargo test --manifest-path src-tauri/Cargo.toml --test timeline_mutation_tests`
Expected: PASS.

- [ ] **Step 5: Commit**
```bash
git add src-tauri/src/timeline/mutation.rs src-tauri/src/timeline/service.rs src-tauri/tests/timeline_mutation_tests.rs
git commit -m "feat(timeline): implement mutation service with overlap collision and optimistic revision locking"
```

---

### Task 6: Monotonic History & Bounded Undo/Redo Engine with Optimistic Revision Control

**Files:**
- Create: `src-tauri/src/timeline/history.rs`
- Modify: `src-tauri/src/timeline/service.rs`
- Test: `src-tauri/tests/timeline_history_tests.rs`

**Interfaces:**
- Consumes: `TimelineDocument`, `timeline_history` table
- Produces: `TimelineService::undo(&self, conn, timeline_id, expected_revision) -> Result<TimelineDocument, TimelineError>`, `TimelineService::redo(&self, conn, timeline_id, expected_revision) -> Result<TimelineDocument, TimelineError>`

- [ ] **Step 1: Write failing tests for initial history undo regression, monotonic revisions, optimistic revision checks, branch invalidation, and 50-snapshot bounds**
Create `src-tauri/tests/timeline_history_tests.rs`:
```rust
use rusqlite::Connection;
use openvideoai_lib::db::migrations::run_all_migrations;
use openvideoai_lib::projects::model::Project;
use openvideoai_lib::projects::repository::ProjectRepository;
use openvideoai_lib::timeline::model::*;
use openvideoai_lib::timeline::repository::TimelineRepository;
use openvideoai_lib::timeline::service::TimelineService;
use openvideoai_lib::timeline::mutation::TimelineMutation;
use openvideoai_lib::timeline::error::TimelineError;

fn setup_test_db() -> (Connection, String) {
    let mut conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    run_all_migrations(&mut conn).unwrap();

    let project = Project::new("p1".to_string(), "Test Project".to_string());
    ProjectRepository::create(&conn, &project).unwrap();

    let canvas = CanvasConfig { width: 1920, height: 1080, frame_rate: 30.0 };
    TimelineRepository::create(&mut conn, "tl1", "p1", "Main Timeline", canvas).unwrap();

    (conn, "tl1".to_string())
}

#[test]
fn test_first_edit_can_be_undone_to_initial_blank_timeline() {
    let (mut conn, tl_id) = setup_test_db();
    let service = TimelineService::new();

    // 1. Initial timeline: revision = 1, tracks = []
    let initial = TimelineRepository::get_by_id(&conn, &tl_id).unwrap().unwrap();
    assert_eq!(initial.revision, 1);
    assert_eq!(initial.tracks.len(), 0);

    // 2. First edit: AddTrack -> revision = 2
    let after_edit = service.apply_mutation(&mut conn, &tl_id, 1, TimelineMutation::AddTrack {
        kind: TrackKind::Video,
        name: "V1".to_string(),
    }).unwrap();
    assert_eq!(after_edit.revision, 2);
    assert_eq!(after_edit.tracks.len(), 1);

    // 3. Undo with expected_revision = 2 -> must succeed!
    // Invariant: revision strictly increments to 3, state restores to 0 tracks
    let undone = service.undo(&mut conn, &tl_id, 2).unwrap();
    assert_eq!(undone.revision, 3);
    assert_eq!(undone.tracks.len(), 0, "Initial blank state must be restored on undoing first edit");

    // 4. Redo with expected_revision = 3 -> must restore V1 track at revision 4
    let redone = service.redo(&mut conn, &tl_id, 3).unwrap();
    assert_eq!(redone.revision, 4);
    assert_eq!(redone.tracks.len(), 1);
    assert_eq!(redone.tracks[0].name, "V1");
}

#[test]
fn test_undo_and_redo_strictly_monotonically_increment_revision() {
    let (mut conn, tl_id) = setup_test_db();
    let service = TimelineService::new();

    // Rev 1 -> 2
    service.apply_mutation(&mut conn, &tl_id, 1, TimelineMutation::AddTrack {
        kind: TrackKind::Video,
        name: "V1".to_string(),
    }).unwrap();

    // Rev 2 -> 3
    service.apply_mutation(&mut conn, &tl_id, 2, TimelineMutation::AddTrack {
        kind: TrackKind::Audio,
        name: "A1".to_string(),
    }).unwrap();

    // Undo: Rev 3 -> 4 (restores V1 only)
    let doc = service.undo(&mut conn, &tl_id, 3).unwrap();
    assert_eq!(doc.revision, 4);
    assert_eq!(doc.tracks.len(), 1);
    assert_eq!(doc.tracks[0].name, "V1");

    // Redo: Rev 4 -> 5 (restores V1 + A1)
    let doc = service.redo(&mut conn, &tl_id, 4).unwrap();
    assert_eq!(doc.revision, 5);
    assert_eq!(doc.tracks.len(), 2);
}

#[test]
fn test_undo_redo_enforces_optimistic_revision_concurrency() {
    let (mut conn, tl_id) = setup_test_db();
    let service = TimelineService::new();

    // Rev 1 -> 2
    service.apply_mutation(&mut conn, &tl_id, 1, TimelineMutation::AddTrack {
        kind: TrackKind::Video,
        name: "V1".to_string(),
    }).unwrap();

    // Stale undo with expected_revision = 99 -> must fail with RevisionConflict
    let err = service.undo(&mut conn, &tl_id, 99).unwrap_err();
    assert_eq!(err, TimelineError::RevisionConflict { expected: 99, actual: 2 });

    // Correct undo -> Rev 2 -> 3
    service.undo(&mut conn, &tl_id, 2).unwrap();

    // Stale redo with expected_revision = 2 -> must fail with RevisionConflict
    let err = service.redo(&mut conn, &tl_id, 2).unwrap_err();
    assert_eq!(err, TimelineError::RevisionConflict { expected: 2, actual: 3 });
}

#[test]
fn test_new_edit_after_undo_prunes_redo_branch() {
    let (mut conn, tl_id) = setup_test_db();
    let service = TimelineService::new();

    // Edit 1: V1 (rev 2)
    service.apply_mutation(&mut conn, &tl_id, 1, TimelineMutation::AddTrack {
        kind: TrackKind::Video,
        name: "V1".to_string(),
    }).unwrap();

    // Edit 2: V2 (rev 3)
    service.apply_mutation(&mut conn, &tl_id, 2, TimelineMutation::AddTrack {
        kind: TrackKind::Video,
        name: "V2".to_string(),
    }).unwrap();

    // Undo back to V1 (rev 4)
    service.undo(&mut conn, &tl_id, 3).unwrap();

    // New edit: A1 (rev 5). This MUST invalidate the redo branch (V2)
    service.apply_mutation(&mut conn, &tl_id, 4, TimelineMutation::AddTrack {
        kind: TrackKind::Audio,
        name: "A1".to_string(),
    }).unwrap();

    // Attempt redo on rev 5 -> must return InvalidOperation("Nothing to redo")
    let err = service.redo(&mut conn, &tl_id, 5).unwrap_err();
    assert_eq!(err, TimelineError::InvalidOperation("Nothing to redo".to_string()));
}

#[test]
fn test_history_bounded_to_50_snapshots() {
    let (mut conn, tl_id) = setup_test_db();
    let service = TimelineService::new();

    // Apply 55 canvas update mutations
    let mut current_rev = 1;
    for i in 0..55 {
        let doc = service.apply_mutation(&mut conn, &tl_id, current_rev, TimelineMutation::UpdateCanvas {
            width: 1920,
            height: 1080,
            frame_rate: 30.0 + (i as f64 * 0.1),
        }).unwrap();
        current_rev = doc.revision;
    }

    let count: i64 = conn.query_row(
        "SELECT count(*) FROM timeline_history WHERE timeline_id = ?1;",
        [&tl_id],
        |r| r.get(0),
    ).unwrap();
    assert!(count <= 50, "History snapshot count must be capped at 50, found: {}", count);

    // Verify current history cursor snapshot is present
    let cursor: i64 = conn.query_row(
        "SELECT history_cursor FROM timelines WHERE id = ?1;",
        [&tl_id],
        |r| r.get(0),
    ).unwrap();

    let cursor_exists: bool = conn.query_row(
        "SELECT count(*) > 0 FROM timeline_history WHERE timeline_id = ?1 AND history_index = ?2;",
        rusqlite::params![&tl_id, cursor],
        |r| r.get(0),
    ).unwrap();
    assert!(cursor_exists, "Current cursor snapshot must never be pruned");
}
```

- [ ] **Step 2: Run test to verify it fails**
Run: `cargo test --manifest-path src-tauri/Cargo.toml --test timeline_history_tests`
Expected: FAIL.

- [ ] **Step 3: Implement Undo/Redo in `TimelineService`**
In `src-tauri/src/timeline/service.rs`, implement `undo` and `redo` methods with `expected_revision` validation:
```rust
impl TimelineService {
    pub fn undo(
        &self,
        conn: &mut Connection,
        timeline_id: &str,
        expected_revision: i64,
    ) -> Result<TimelineDocument, TimelineError> {
        let current = TimelineRepository::get_by_id(conn, timeline_id)?
            .ok_or_else(|| TimelineError::TimelineNotFound(timeline_id.to_string()))?;

        if current.revision != expected_revision {
            return Err(TimelineError::RevisionConflict {
                expected: expected_revision,
                actual: current.revision,
            });
        }

        let cursor: i64 = conn.query_row(
            "SELECT history_cursor FROM timelines WHERE id = ?1;",
            params![timeline_id],
            |r| r.get(0),
        )?;

        if cursor <= 0 {
            return Err(TimelineError::InvalidOperation("Nothing to undo".to_string()));
        }

        let target_cursor = cursor - 1;
        let snapshot_json: String = conn.query_row(
            "SELECT snapshot_json FROM timeline_history WHERE timeline_id = ?1 AND history_index = ?2;",
            params![timeline_id, target_cursor],
            |r| r.get(0),
        )?;

        let parsed: serde_json::Value = serde_json::from_str(&snapshot_json)
            .map_err(|e| TimelineError::Database(format!("Corrupt snapshot JSON: {}", e)))?;
        let restored_doc: TimelineDocument = serde_json::from_value(parsed["timeline"].clone())
            .map_err(|e| TimelineError::Database(format!("Invalid snapshot format: {}", e)))?;

        let new_revision = current.revision + 1;
        let now = Utc::now().to_rfc3339();
        let tx = conn.transaction()?;

        // Transactionally restore state from snapshot
        Self::restore_timeline_state(&tx, timeline_id, &restored_doc)?;

        tx.execute(
            "UPDATE timelines SET revision = ?1, history_cursor = ?2, updated_at = ?3 WHERE id = ?4;",
            params![new_revision, target_cursor, now, timeline_id],
        )?;

        tx.commit()?;

        let mut final_doc = restored_doc;
        final_doc.revision = new_revision;
        final_doc.updated_at = now;
        Ok(final_doc)
    }

    pub fn redo(
        &self,
        conn: &mut Connection,
        timeline_id: &str,
        expected_revision: i64,
    ) -> Result<TimelineDocument, TimelineError> {
        let current = TimelineRepository::get_by_id(conn, timeline_id)?
            .ok_or_else(|| TimelineError::TimelineNotFound(timeline_id.to_string()))?;

        if current.revision != expected_revision {
            return Err(TimelineError::RevisionConflict {
                expected: expected_revision,
                actual: current.revision,
            });
        }

        let cursor: i64 = conn.query_row(
            "SELECT history_cursor FROM timelines WHERE id = ?1;",
            params![timeline_id],
            |r| r.get(0),
        )?;

        let target_cursor = cursor + 1;
        let snapshot_opt: Option<String> = conn.query_row(
            "SELECT snapshot_json FROM timeline_history WHERE timeline_id = ?1 AND history_index = ?2;",
            params![timeline_id, target_cursor],
            |r| r.get(0),
        ).optional()?;

        let snapshot_json = match snapshot_opt {
            Some(s) => s,
            None => return Err(TimelineError::InvalidOperation("Nothing to redo".to_string())),
        };

        let parsed: serde_json::Value = serde_json::from_str(&snapshot_json)
            .map_err(|e| TimelineError::Database(format!("Corrupt snapshot JSON: {}", e)))?;
        let restored_doc: TimelineDocument = serde_json::from_value(parsed["timeline"].clone())
            .map_err(|e| TimelineError::Database(format!("Invalid snapshot format: {}", e)))?;

        let new_revision = current.revision + 1;
        let now = Utc::now().to_rfc3339();
        let tx = conn.transaction()?;

        Self::restore_timeline_state(&tx, timeline_id, &restored_doc)?;

        tx.execute(
            "UPDATE timelines SET revision = ?1, history_cursor = ?2, updated_at = ?3 WHERE id = ?4;",
            params![new_revision, target_cursor, now, timeline_id],
        )?;

        tx.commit()?;

        let mut final_doc = restored_doc;
        final_doc.revision = new_revision;
        final_doc.updated_at = now;
        Ok(final_doc)
    }

    fn restore_timeline_state(tx: &rusqlite::Transaction, timeline_id: &str, doc: &TimelineDocument) -> Result<(), TimelineError> {
        // Clear current tracks (cascades to clips)
        tx.execute("DELETE FROM timeline_tracks WHERE timeline_id = ?1;", params![timeline_id])?;

        // Update canvas
        tx.execute(
            "UPDATE timelines SET width = ?1, height = ?2, frame_rate = ?3 WHERE id = ?4;",
            params![doc.canvas.width, doc.canvas.height, doc.canvas.frame_rate, timeline_id],
        )?;

        // Re-insert tracks and clips
        for track in &doc.tracks {
            tx.execute(
                "INSERT INTO timeline_tracks (id, timeline_id, kind, order_index, name, muted)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6);",
                params![track.id, timeline_id, track.kind.as_str(), track.order, track.name, track.muted as i32],
            )?;

            for clip in &track.clips {
                tx.execute(
                    "INSERT INTO timeline_clips (id, track_id, artifact_id, source_in_ms, source_out_ms, timeline_start_ms, volume, muted, position_x, position_y, scale, rotation_deg, opacity)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13);",
                    params![
                        clip.id,
                        track.id,
                        clip.artifact_id,
                        clip.source_in_ms,
                        clip.source_out_ms,
                        clip.timeline_start_ms,
                        clip.volume,
                        clip.muted as i32,
                        clip.transform.x,
                        clip.transform.y,
                        clip.transform.scale,
                        clip.transform.rotation_deg,
                        clip.transform.opacity,
                    ],
                )?;
            }
        }
        Ok(())
    }
}
```

- [ ] **Step 4: Run test to verify it passes**
Run: `cargo test --manifest-path src-tauri/Cargo.toml --test timeline_history_tests`
Expected: PASS.

- [ ] **Step 5: Commit**
```bash
git add src-tauri/src/timeline/service.rs src-tauri/tests/timeline_history_tests.rs
git commit -m "feat(timeline): implement undo/redo with optimistic revision concurrency and monotonic numbering"
```

---

### Task 7: Media Preview Gateway (Core Range Handler & Protocol)

**Files:**
- Create: `src-tauri/src/timeline/preview.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/tests/preview_gateway_tests.rs`

**Interfaces:**
- Consumes: `ArtifactResolver`, actual Phase 1 `Artifact` and `ArtifactRepository`, HTTP Range headers
- Produces: `handle_preview_range_request(artifact_id, range_header, &resolver, &conn) -> Result<PreviewResponse, PreviewError>` and Tauri custom URI scheme `openvideoai-media://artifact/<id>`

- [ ] **Step 1: Write failing tests for preview gateway range parsing, partial content streaming, 416, 404, traversal rejection, and MIME types**
Create `src-tauri/tests/preview_gateway_tests.rs`:
```rust
use rusqlite::Connection;
use std::fs::File;
use std::io::Write;
use std::sync::Arc;
use tempfile::tempdir;
use openvideoai_lib::db::migrations::run_all_migrations;
use openvideoai_lib::db::DbManager;
use openvideoai_lib::storage::local_store::LocalStore;
use openvideoai_lib::artifacts::model::{Artifact, ArtifactKind};
use openvideoai_lib::artifacts::repository::ArtifactRepository;
use openvideoai_lib::artifacts::resolver::ArtifactResolver;
use openvideoai_lib::timeline::preview::{handle_preview_range_request, PreviewResponse};

fn setup_test_env() -> (Connection, Arc<ArtifactResolver>, String) {
    let mut conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    run_all_migrations(&mut conn).unwrap();

    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let store_dir = temp_dir.path().join("store");
    std::fs::create_dir_all(&store_dir).unwrap();

    let mut disk_conn = Connection::open(&db_path).unwrap();
    disk_conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    run_all_migrations(&mut disk_conn).unwrap();

    // Create test media file of exactly 100 bytes (0x00 to 0x63)
    let file_bytes: Vec<u8> = (0..100).collect();
    let storage_key = "test_media.mp4".to_string();
    let file_path = store_dir.join(&storage_key);
    let mut f = File::create(&file_path).unwrap();
    f.write_all(&file_bytes).unwrap();
    drop(f);

    let artifact = Artifact {
        id: "art-preview-1".to_string(),
        sha256: "test-hash-123".to_string(),
        kind: ArtifactKind::Video,
        mime_type: Some("video/mp4".to_string()),
        size_bytes: 100,
        storage_key,
        original_filename: "test.mp4".to_string(),
        created_at: "2026-09-10T12:00:00Z".to_string(),
    };
    ArtifactRepository::insert(&disk_conn, &artifact).unwrap();

    let db_manager = Arc::new(DbManager::new(db_path));
    let local_store = Arc::new(LocalStore::new(store_dir));
    let resolver = Arc::new(ArtifactResolver::new(db_manager, local_store));

    (disk_conn, resolver, artifact.id)
}

#[test]
fn test_range_bytes_0_to_49_returns_206() {
    let (conn, resolver, art_id) = setup_test_env();
    let res = handle_preview_range_request(&art_id, Some("bytes=0-49"), &resolver, &conn).unwrap();

    assert_eq!(res.status_code, 206);
    assert_eq!(res.body.len(), 50);
    assert_eq!(res.body[0], 0);
    assert_eq!(res.body[49], 49);

    let content_range = res.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case("content-range")).map(|(_, v)| v.as_str());
    assert_eq!(content_range, Some("bytes 0-49/100"));
}

#[test]
fn test_open_ended_range_returns_206() {
    let (conn, resolver, art_id) = setup_test_env();
    let res = handle_preview_range_request(&art_id, Some("bytes=50-"), &resolver, &conn).unwrap();

    assert_eq!(res.status_code, 206);
    assert_eq!(res.body.len(), 50);
    assert_eq!(res.body[0], 50);
    assert_eq!(res.body[49], 99);

    let content_range = res.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case("content-range")).map(|(_, v)| v.as_str());
    assert_eq!(content_range, Some("bytes 50-99/100"));
}

#[test]
fn test_no_range_header_returns_200() {
    let (conn, resolver, art_id) = setup_test_env();
    let res = handle_preview_range_request(&art_id, None, &resolver, &conn).unwrap();

    assert_eq!(res.status_code, 200);
    assert_eq!(res.body.len(), 100);
    assert_eq!(res.body[0], 0);
    assert_eq!(res.body[99], 99);
}

#[test]
fn test_out_of_bounds_range_returns_416() {
    let (conn, resolver, art_id) = setup_test_env();
    let res = handle_preview_range_request(&art_id, Some("bytes=200-300"), &resolver, &conn).unwrap();

    assert_eq!(res.status_code, 416);
    let content_range = res.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case("content-range")).map(|(_, v)| v.as_str());
    assert_eq!(content_range, Some("bytes */100"));
}

#[test]
fn test_non_existent_artifact_returns_404() {
    let (conn, resolver, _) = setup_test_env();
    let res = handle_preview_range_request("non-existent-id", None, &resolver, &conn).unwrap();
    assert_eq!(res.status_code, 404);
}

#[test]
fn test_path_traversal_artifact_id_rejected_with_400_or_404() {
    let (conn, resolver, _) = setup_test_env();
    let res = handle_preview_range_request("../etc/passwd", None, &resolver, &conn).unwrap();
    assert!(res.status_code == 400 || res.status_code == 404);
}

#[test]
fn test_correct_mime_type_returned() {
    let (conn, resolver, art_id) = setup_test_env();
    let res = handle_preview_range_request(&art_id, None, &resolver, &conn).unwrap();

    let content_type = res.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case("content-type")).map(|(_, v)| v.as_str());
    assert_eq!(content_type, Some("video/mp4"));
}
```

- [ ] **Step 2: Run test to verify it fails**
Run: `cargo test --manifest-path src-tauri/Cargo.toml --test preview_gateway_tests`
Expected: FAIL.

- [ ] **Step 3: Implement `handle_preview_range_request` and protocol handler**
Implement `src-tauri/src/timeline/preview.rs`:
```rust
use rusqlite::Connection;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use crate::artifacts::repository::ArtifactRepository;
use crate::artifacts::resolver::ArtifactResolver;

pub struct PreviewResponse {
    pub status_code: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

pub fn handle_preview_range_request(
    artifact_id: &str,
    range_header: Option<&str>,
    resolver: &ArtifactResolver,
    conn: &Connection,
) -> Result<PreviewResponse, String> {
    // 1. Path traversal & security check
    if artifact_id.is_empty()
        || artifact_id.contains('/')
        || artifact_id.contains('\')
        || artifact_id.contains("..")
    {
        return Ok(PreviewResponse {
            status_code: 400,
            headers: vec![("Content-Type".to_string(), "text/plain".to_string())],
            body: b"Invalid artifact ID".to_vec(),
        });
    }

    // 2. Resolve artifact in DB
    let artifact = match ArtifactRepository::get_by_id(conn, artifact_id) {
        Ok(Some(a)) => a,
        Ok(None) => {
            return Ok(PreviewResponse {
                status_code: 404,
                headers: vec![("Content-Type".to_string(), "text/plain".to_string())],
                body: b"Artifact Not Found".to_vec(),
            });
        }
        Err(e) => return Err(e.to_string()),
    };

    // 3. Resolve canonical file path
    let canonical_path = match resolver.resolve_uri(&format!("artifact://{}", artifact_id)) {
        Ok(p) => p,
        Err(_) => {
            return Ok(PreviewResponse {
                status_code: 404,
                headers: vec![("Content-Type".to_string(), "text/plain".to_string())],
                body: b"Storage File Missing".to_vec(),
            });
        }
    };

    let mut file = match File::open(&canonical_path) {
        Ok(f) => f,
        Err(_) => {
            return Ok(PreviewResponse {
                status_code: 404,
                headers: vec![("Content-Type".to_string(), "text/plain".to_string())],
                body: b"Unable to open file".to_vec(),
            });
        }
    };

    let file_len = match file.metadata() {
        Ok(m) => m.len(),
        Err(_) => return Err("Failed to read file metadata".to_string()),
    };

    let mime_type = artifact.mime_type.unwrap_or_else(|| "application/octet-stream".to_string());

    // 4. Handle Range header
    if let Some(range_str) = range_header {
        if let Some(spec) = range_str.strip_prefix("bytes=") {
            let parts: Vec<&str> = spec.split('-').collect();
            if parts.len() == 2 {
                let start_res = parts[0].parse::<u64>();
                let end_res = if parts[1].is_empty() {
                    None
                } else {
                    parts[1].parse::<u64>().ok()
                };

                if let Ok(start) = start_res {
                    if start >= file_len {
                        return Ok(PreviewResponse {
                            status_code: 416,
                            headers: vec![
                                ("Content-Range".to_string(), format!("bytes */{}", file_len)),
                                ("Content-Type".to_string(), mime_type),
                            ],
                            body: Vec::new(),
                        });
                    }

                    let end = end_res.unwrap_or(file_len - 1).min(file_len - 1);
                    if start <= end {
                        let length = end - start + 1;
                        if file.seek(SeekFrom::Start(start)).is_err() {
                            return Err("Seek failed".to_string());
                        }

                        let mut buffer = vec![0u8; length as usize];
                        if file.read_exact(&mut buffer).is_err() {
                            return Err("Read failed".to_string());
                        }

                        return Ok(PreviewResponse {
                            status_code: 206,
                            headers: vec![
                                ("Content-Type".to_string(), mime_type),
                                ("Content-Range".to_string(), format!("bytes {}-{}/{}", start, end, file_len)),
                                ("Content-Length".to_string(), length.to_string()),
                                ("Accept-Ranges".to_string(), "bytes".to_string()),
                                ("Access-Control-Allow-Origin".to_string(), "*".to_string()),
                            ],
                            body: buffer,
                        });
                    }
                }
            }
        }
    }

    // Full 200 OK
    let mut buffer = Vec::with_capacity(file_len as usize);
    if file.read_to_end(&mut buffer).is_err() {
        return Err("Read to end failed".to_string());
    }

    Ok(PreviewResponse {
        status_code: 200,
        headers: vec![
            ("Content-Type".to_string(), mime_type),
            ("Content-Length".to_string(), file_len.to_string()),
            ("Accept-Ranges".to_string(), "bytes".to_string()),
            ("Access-Control-Allow-Origin".to_string(), "*".to_string()),
        ],
        body: buffer,
    })
}
```

Register `openvideoai-media` in `src-tauri/src/lib.rs` using `register_asynchronous_uri_scheme_protocol`.

- [ ] **Step 4: Run test to verify it passes**
Run: `cargo test --manifest-path src-tauri/Cargo.toml --test preview_gateway_tests`
Expected: PASS.

- [ ] **Step 5: Commit**
```bash
git add src-tauri/src/timeline/preview.rs src-tauri/src/lib.rs src-tauri/tests/preview_gateway_tests.rs
git commit -m "feat(preview): implement secure media preview gateway with byte-range streaming"
```

---

### Task 8: RenderSpec v1 Data Structures & Builder with Stream Capability Binding

**Files:**
- Create: `src-tauri/src/timeline/renderspec.rs`
- Modify: `src-tauri/src/timeline/mod.rs`
- Test: `src-tauri/tests/renderspec_tests.rs`

**Interfaces:**
- Consumes: `TimelineDocument`, `TimelineClip`, `ClipTransform`, `MediaMetadata`, committed metadata lookup trait
- Produces: `RenderSpecV1`, `RenderOutputConfig`, `RenderClip` (with immutable `has_video` and `has_audio` fields), `RenderSpecBuilder::build_from_timeline(&TimelineDocument, &impl MediaMetadataLookup) -> Result<RenderSpecV1, TimelineError>`

- [ ] **Step 1: Write failing tests for stream capability freezing (AAC, audio-less video, audio-only, missing metadata), immutable snapshot construction, empty timeline rejection, and default transform contract**
Create `src-tauri/tests/renderspec_tests.rs`:
```rust
use std::collections::HashMap;
use openvideoai_lib::artifacts::metadata::MediaMetadata;
use openvideoai_lib::timeline::model::*;
use openvideoai_lib::timeline::renderspec::*;
use openvideoai_lib::timeline::error::TimelineError;

fn create_metadata_map() -> HashMap<String, MediaMetadata> {
    let mut map = HashMap::new();
    map.insert("art-video-aac".to_string(), MediaMetadata {
        artifact_id: "art-video-aac".to_string(),
        probe_status: "ok".to_string(),
        duration_ms: Some(10000),
        width: Some(1920),
        height: Some(1080),
        video_codec: Some("h264".to_string()),
        audio_codec: Some("aac".to_string()),
        frame_rate: Some(30.0),
        sample_rate: Some(48000),
        channels: Some(2),
        probe_error: None,
    });
    map.insert("art-video-noaudio".to_string(), MediaMetadata {
        artifact_id: "art-video-noaudio".to_string(),
        probe_status: "ok".to_string(),
        duration_ms: Some(8000),
        width: Some(1920),
        height: Some(1080),
        video_codec: Some("h264".to_string()),
        audio_codec: None, // No audio stream!
        frame_rate: Some(30.0),
        sample_rate: None,
        channels: None,
        probe_error: None,
    });
    map.insert("art-audio-only".to_string(), MediaMetadata {
        artifact_id: "art-audio-only".to_string(),
        probe_status: "ok".to_string(),
        duration_ms: Some(15000),
        width: None,
        height: None,
        video_codec: None, // No video stream!
        audio_codec: Some("aac".to_string()),
        frame_rate: None,
        sample_rate: Some(48000),
        channels: Some(2),
        probe_error: None,
    });
    map
}

#[test]
fn test_video_artifact_with_aac_freezes_has_audio_true() {
    let meta_map = create_metadata_map();
    let doc = TimelineDocument {
        id: "tl1".to_string(),
        project_id: "p1".to_string(),
        name: "Test".to_string(),
        revision: 1,
        canvas: CanvasConfig { width: 1920, height: 1080, frame_rate: 30.0 },
        tracks: vec![
            TimelineTrack {
                id: "tr1".to_string(),
                timeline_id: "tl1".to_string(),
                kind: TrackKind::Video,
                order: 0,
                name: "V1".to_string(),
                muted: false,
                clips: vec![TimelineClip {
                    id: "c1".to_string(),
                    track_id: "tr1".to_string(),
                    artifact_id: "art-video-aac".to_string(),
                    source_in_ms: 0,
                    source_out_ms: 5000,
                    timeline_start_ms: 0,
                    volume: 1.0,
                    muted: false,
                    transform: ClipTransform::default(),
                }],
            }
        ],
        created_at: "2026-09-10T12:00:00Z".to_string(),
        updated_at: "2026-09-10T12:00:00Z".to_string(),
    };

    let spec = RenderSpecBuilder::build_from_timeline(&doc, &meta_map).unwrap();
    assert_eq!(spec.tracks[0].clips[0].has_video, true);
    assert_eq!(spec.tracks[0].clips[0].has_audio, true);
}

#[test]
fn test_video_artifact_without_audio_freezes_has_audio_false() {
    let meta_map = create_metadata_map();
    let doc = TimelineDocument {
        id: "tl1".to_string(),
        project_id: "p1".to_string(),
        name: "Test".to_string(),
        revision: 1,
        canvas: CanvasConfig { width: 1920, height: 1080, frame_rate: 30.0 },
        tracks: vec![
            TimelineTrack {
                id: "tr1".to_string(),
                timeline_id: "tl1".to_string(),
                kind: TrackKind::Video,
                order: 0,
                name: "V1".to_string(),
                muted: false,
                clips: vec![TimelineClip {
                    id: "c1".to_string(),
                    track_id: "tr1".to_string(),
                    artifact_id: "art-video-noaudio".to_string(),
                    source_in_ms: 0,
                    source_out_ms: 5000,
                    timeline_start_ms: 0,
                    volume: 1.0,
                    muted: false,
                    transform: ClipTransform::default(),
                }],
            }
        ],
        created_at: "2026-09-10T12:00:00Z".to_string(),
        updated_at: "2026-09-10T12:00:00Z".to_string(),
    };

    let spec = RenderSpecBuilder::build_from_timeline(&doc, &meta_map).unwrap();
    assert_eq!(spec.tracks[0].clips[0].has_video, true);
    assert_eq!(spec.tracks[0].clips[0].has_audio, false);
}

#[test]
fn test_audio_only_artifact_freezes_stream_capabilities_correctly() {
    let meta_map = create_metadata_map();
    let doc = TimelineDocument {
        id: "tl1".to_string(),
        project_id: "p1".to_string(),
        name: "Test".to_string(),
        revision: 1,
        canvas: CanvasConfig { width: 1920, height: 1080, frame_rate: 30.0 },
        tracks: vec![
            TimelineTrack {
                id: "tr1".to_string(),
                timeline_id: "tl1".to_string(),
                kind: TrackKind::Audio,
                order: 0,
                name: "A1".to_string(),
                muted: false,
                clips: vec![TimelineClip {
                    id: "c1".to_string(),
                    track_id: "tr1".to_string(),
                    artifact_id: "art-audio-only".to_string(),
                    source_in_ms: 0,
                    source_out_ms: 5000,
                    timeline_start_ms: 0,
                    volume: 1.0,
                    muted: false,
                    transform: ClipTransform::default(),
                }],
            }
        ],
        created_at: "2026-09-10T12:00:00Z".to_string(),
        updated_at: "2026-09-10T12:00:00Z".to_string(),
    };

    let spec = RenderSpecBuilder::build_from_timeline(&doc, &meta_map).unwrap();
    assert_eq!(spec.tracks[0].clips[0].has_video, false);
    assert_eq!(spec.tracks[0].clips[0].has_audio, true);
}

#[test]
fn test_missing_required_metadata_returns_typed_error() {
    let meta_map = HashMap::<String, MediaMetadata>::new(); // Empty!
    let doc = TimelineDocument {
        id: "tl1".to_string(),
        project_id: "p1".to_string(),
        name: "Test".to_string(),
        revision: 1,
        canvas: CanvasConfig { width: 1920, height: 1080, frame_rate: 30.0 },
        tracks: vec![
            TimelineTrack {
                id: "tr1".to_string(),
                timeline_id: "tl1".to_string(),
                kind: TrackKind::Video,
                order: 0,
                name: "V1".to_string(),
                muted: false,
                clips: vec![TimelineClip {
                    id: "c1".to_string(),
                    track_id: "tr1".to_string(),
                    artifact_id: "missing-artifact".to_string(),
                    source_in_ms: 0,
                    source_out_ms: 5000,
                    timeline_start_ms: 0,
                    volume: 1.0,
                    muted: false,
                    transform: ClipTransform::default(),
                }],
            }
        ],
        created_at: "2026-09-10T12:00:00Z".to_string(),
        updated_at: "2026-09-10T12:00:00Z".to_string(),
    };

    let err = RenderSpecBuilder::build_from_timeline(&doc, &meta_map).unwrap_err();
    assert_eq!(err, TimelineError::ArtifactNotFound("missing-artifact".to_string()));
}

#[test]
fn test_renderspec_v1_builds_immutable_snapshot_with_no_physical_paths() {
    let meta_map = create_metadata_map();
    let doc = TimelineDocument {
        id: "tl1".to_string(),
        project_id: "p1".to_string(),
        name: "Test Timeline".to_string(),
        revision: 4,
        canvas: CanvasConfig { width: 1920, height: 1080, frame_rate: 30.0 },
        tracks: vec![
            TimelineTrack {
                id: "tr1".to_string(),
                timeline_id: "tl1".to_string(),
                kind: TrackKind::Video,
                order: 0,
                name: "V1".to_string(),
                muted: false,
                clips: vec![TimelineClip {
                    id: "c1".to_string(),
                    track_id: "tr1".to_string(),
                    artifact_id: "art-video-aac".to_string(),
                    source_in_ms: 1000,
                    source_out_ms: 5000,
                    timeline_start_ms: 0,
                    volume: 1.0,
                    muted: false,
                    transform: ClipTransform::default(),
                }],
            }
        ],
        created_at: "2026-09-10T12:00:00Z".to_string(),
        updated_at: "2026-09-10T12:00:00Z".to_string(),
    };

    let spec = RenderSpecBuilder::build_from_timeline(&doc, &meta_map).unwrap();

    assert_eq!(spec.version, 1);
    assert_eq!(spec.project_id, "p1");
    assert_eq!(spec.timeline_id, "tl1");
    assert_eq!(spec.timeline_revision, 4);

    assert_eq!(spec.output.width, 1920);
    assert_eq!(spec.output.height, 1080);
    assert_eq!(spec.output.frame_rate, 30.0);
    assert_eq!(spec.output.container, "mp4");
    assert_eq!(spec.output.video_codec, "h264");
    assert_eq!(spec.output.audio_codec, "aac");
    assert_eq!(spec.output.pixel_format, "yuv420p");

    assert_eq!(spec.tracks.len(), 1);
    assert_eq!(spec.tracks[0].clips.len(), 1);
    assert_eq!(spec.tracks[0].clips[0].artifact_id, "art-video-aac");
    assert_eq!(spec.tracks[0].clips[0].has_video, true);
    assert_eq!(spec.tracks[0].clips[0].has_audio, true);

    // Serialization verification: contains no drive letters or slashes
    let serialized = serde_json::to_string(&spec).unwrap();
    assert!(!serialized.contains("C:\"), "RenderSpec must never contain physical filesystem paths");
    assert!(!serialized.contains("/home/"), "RenderSpec must never contain physical filesystem paths");
}

#[test]
fn test_empty_timeline_renderspec_rejected() {
    let meta_map = create_metadata_map();
    let doc = TimelineDocument {
        id: "tl1".to_string(),
        project_id: "p1".to_string(),
        name: "Empty".to_string(),
        revision: 1,
        canvas: CanvasConfig { width: 1920, height: 1080, frame_rate: 30.0 },
        tracks: vec![],
        created_at: "2026-09-10T12:00:00Z".to_string(),
        updated_at: "2026-09-10T12:00:00Z".to_string(),
    };

    let err = RenderSpecBuilder::build_from_timeline(&doc, &meta_map).unwrap_err();
    assert_eq!(err, TimelineError::RenderSpecInvalid("Timeline has no clips to render".to_string()));
}

#[test]
fn test_non_default_transform_rejected_in_phase_3() {
    let meta_map = create_metadata_map();
    let mut custom_transform = ClipTransform::default();
    custom_transform.rotation_deg = 45.0; // Non-default transform!

    let doc = TimelineDocument {
        id: "tl1".to_string(),
        project_id: "p1".to_string(),
        name: "Transform Test".to_string(),
        revision: 2,
        canvas: CanvasConfig { width: 1920, height: 1080, frame_rate: 30.0 },
        tracks: vec![
            TimelineTrack {
                id: "tr1".to_string(),
                timeline_id: "tl1".to_string(),
                kind: TrackKind::Video,
                order: 0,
                name: "V1".to_string(),
                muted: false,
                clips: vec![TimelineClip {
                    id: "c1".to_string(),
                    track_id: "tr1".to_string(),
                    artifact_id: "art-video-aac".to_string(),
                    source_in_ms: 0,
                    source_out_ms: 3000,
                    timeline_start_ms: 0,
                    volume: 1.0,
                    muted: false,
                    transform: custom_transform,
                }],
            }
        ],
        created_at: "2026-09-10T12:00:00Z".to_string(),
        updated_at: "2026-09-10T12:00:00Z".to_string(),
    };

    let err = RenderSpecBuilder::build_from_timeline(&doc, &meta_map).unwrap_err();
    assert_eq!(err, TimelineError::InvalidOperation("Non-default clip transforms not supported in Phase 3".to_string()));
}
```

- [ ] **Step 2: Run test to verify it fails**
Run: `cargo test --manifest-path src-tauri/Cargo.toml --test renderspec_tests`
Expected: FAIL.

- [ ] **Step 3: Implement `RenderSpecV1`, `MediaMetadataLookup`, and `RenderSpecBuilder`**
Implement `src-tauri/src/timeline/renderspec.rs`:
```rust
use std::collections::HashMap;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use crate::artifacts::metadata::{MediaMetadata, MetadataRepository};
use super::model::*;
use super::error::TimelineError;

pub trait MediaMetadataLookup {
    fn get_media_metadata(&self, artifact_id: &str) -> Result<Option<MediaMetadata>, TimelineError>;
}

impl MediaMetadataLookup for Connection {
    fn get_media_metadata(&self, artifact_id: &str) -> Result<Option<MediaMetadata>, TimelineError> {
        MetadataRepository::get_by_artifact_id(self, artifact_id)
            .map_err(|e| TimelineError::Database(e.to_string()))
    }
}

impl MediaMetadataLookup for HashMap<String, MediaMetadata> {
    fn get_media_metadata(&self, artifact_id: &str) -> Result<Option<MediaMetadata>, TimelineError> {
        Ok(self.get(artifact_id).cloned())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RenderOutputConfig {
    pub width: u32,
    pub height: u32,
    pub frame_rate: f64,
    pub container: String,   // "mp4"
    pub video_codec: String, // "h264"
    pub audio_codec: String, // "aac"
    pub pixel_format: String,// "yuv420p"
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RenderClip {
    pub id: String,
    pub artifact_id: String,
    pub source_in_ms: i64,
    pub source_out_ms: i64,
    pub timeline_start_ms: i64,
    pub volume: f64,
    pub muted: bool,
    pub transform: ClipTransform,
    pub has_video: bool,
    pub has_audio: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RenderTrack {
    pub id: String,
    pub kind: TrackKind,
    pub order: i32,
    pub muted: bool,
    pub clips: Vec<RenderClip>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RenderSpecV1 {
    pub version: u32, // 1
    pub project_id: String,
    pub timeline_id: String,
    pub timeline_revision: i64,
    pub output: RenderOutputConfig,
    pub tracks: Vec<RenderTrack>,
}

pub struct RenderSpecBuilder;

impl RenderSpecBuilder {
    pub fn build_from_timeline<M: MediaMetadataLookup>(
        doc: &TimelineDocument,
        metadata_lookup: &M,
    ) -> Result<RenderSpecV1, TimelineError> {
        let mut total_clips = 0;
        let mut render_tracks = Vec::new();

        for track in &doc.tracks {
            let mut render_clips = Vec::new();
            for clip in &track.clips {
                // Enforce Phase 3 transform contract: default transforms only
                if !clip.transform.is_default() {
                    return Err(TimelineError::InvalidOperation(
                        "Non-default clip transforms not supported in Phase 3".to_string(),
                    ));
                }

                // Freeze stream capability source of truth from committed artifact metadata
                let meta = metadata_lookup.get_media_metadata(&clip.artifact_id)?
                    .ok_or_else(|| TimelineError::ArtifactNotFound(clip.artifact_id.clone()))?;

                let has_video = meta.video_codec.is_some();
                let has_audio = meta.audio_codec.is_some();

                if track.kind == TrackKind::Video && !has_video {
                    return Err(TimelineError::InvalidOperation(format!(
                        "Artifact {} on video track has no video stream",
                        clip.artifact_id
                    )));
                }
                if track.kind == TrackKind::Audio && !has_audio {
                    return Err(TimelineError::InvalidOperation(format!(
                        "Artifact {} on audio track has no audio stream",
                        clip.artifact_id
                    )));
                }

                total_clips += 1;
                render_clips.push(RenderClip {
                    id: clip.id.clone(),
                    artifact_id: clip.artifact_id.clone(),
                    source_in_ms: clip.source_in_ms,
                    source_out_ms: clip.source_out_ms,
                    timeline_start_ms: clip.timeline_start_ms,
                    volume: clip.volume,
                    muted: clip.muted,
                    transform: clip.transform.clone(),
                    has_video,
                    has_audio,
                });
            }

            render_tracks.push(RenderTrack {
                id: track.id.clone(),
                kind: track.kind,
                order: track.order,
                muted: track.muted,
                clips: render_clips,
            });
        }

        if total_clips == 0 {
            return Err(TimelineError::RenderSpecInvalid("Timeline has no clips to render".to_string()));
        }

        Ok(RenderSpecV1 {
            version: 1,
            project_id: doc.project_id.clone(),
            timeline_id: doc.id.clone(),
            timeline_revision: doc.revision,
            output: RenderOutputConfig {
                width: doc.canvas.width,
                height: doc.canvas.height,
                frame_rate: doc.canvas.frame_rate,
                container: "mp4".to_string(),
                video_codec: "h264".to_string(),
                audio_codec: "aac".to_string(),
                pixel_format: "yuv420p".to_string(),
            },
            tracks: render_tracks,
        })
    }
}
```

- [ ] **Step 4: Run test to verify it passes**
Run: `cargo test --manifest-path src-tauri/Cargo.toml --test renderspec_tests`
Expected: PASS.

- [ ] **Step 5: Commit**
```bash
git add src-tauri/src/timeline/renderspec.rs src-tauri/src/timeline/mod.rs src-tauri/tests/renderspec_tests.rs
git commit -m "feat(timeline): implement RenderSpec v1 builder with stream capability freezing and metadata binding"
```

---

### Task 9: Job Type-Safe Atomic Leasing in JobManager & Repository

**Files:**
- Modify: `src-tauri/src/jobs/repository.rs`
- Modify: `src-tauri/src/jobs/manager.rs`
- Test: `src-tauri/tests/job_type_lease_tests.rs`

**Interfaces:**
- Consumes: `Connection`, `JobManager`, `JobRepository`, `JobSubmission`
- Produces: `JobRepository::acquire_lease_for_type(&conn, worker_id, job_type, lease_duration_secs)`, `JobManager::acquire_next_job_for_type(&self, job_type, lease_duration_secs)`

- [ ] **Step 1: Write failing test for type-safe atomic leasing**
Create `src-tauri/tests/job_type_lease_tests.rs`:
```rust
use std::sync::Arc;
use tempfile::tempdir;
use rusqlite::Connection;
use openvideoai_lib::db::migrations::run_all_migrations;
use openvideoai_lib::db::DbManager;
use openvideoai_lib::jobs::model::{JobState, JobSubmission};
use openvideoai_lib::jobs::manager::JobManager;

#[test]
fn test_type_filtered_atomic_leasing_only_acquires_matching_job_type() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut conn = Connection::open(&db_path).unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    run_all_migrations(&mut conn).unwrap();

    let db_manager = Arc::new(DbManager::new(db_path));
    let job_manager = Arc::new(JobManager::new(db_manager));

    // 1. Submit Job A of type "test.other"
    let sub_a = JobSubmission {
        job_type: "test.other".to_string(),
        priority: 100, // Higher priority!
        idempotency_key: Some("test:other:1".to_string()),
        project_id: None,
        payload: "{}".to_string(),
        max_retries: 3,
        payload_version: 1,
    };
    let job_a = job_manager.submit_job(sub_a).unwrap();

    // 2. Submit Job B of type "render.timeline"
    let sub_b = JobSubmission {
        job_type: "render.timeline".to_string(),
        priority: 10, // Lower priority!
        idempotency_key: Some("render:timeline:1".to_string()),
        project_id: None,
        payload: "{}".to_string(),
        max_retries: 3,
        payload_version: 1,
    };
    let job_b = job_manager.submit_job(sub_b).unwrap();

    // 3. Render worker requests job of type "render.timeline"
    let leased_opt = job_manager.acquire_next_job_for_type("render.timeline", 60).unwrap();
    let leased = leased_opt.expect("Should lease job B");

    // Invariant: Leased job must be Job B
    assert_eq!(leased.id, job_b.id);
    assert_eq!(leased.job_type, "render.timeline");
    assert_eq!(leased.status, JobState::Running);

    // Invariant: Job A must remain untouched in 'queued' state despite higher priority
    let fresh_a = job_manager.get_job(&job_a.id).unwrap().unwrap();
    assert_eq!(fresh_a.status, JobState::Queued);
    assert_eq!(fresh_a.leased_by, None);
}
```

- [ ] **Step 2: Run test to verify it fails**
Run: `cargo test --manifest-path src-tauri/Cargo.toml --test job_type_lease_tests`
Expected: FAIL (method not found).

- [ ] **Step 3: Implement `acquire_lease_for_type` in Repository and Manager**
In `src-tauri/src/jobs/repository.rs`, add:
```rust
    pub fn acquire_lease_for_type(
        conn: &Connection,
        worker_id: &str,
        job_type: &str,
        lease_duration_secs: i64,
    ) -> Result<Option<Job>, JobError> {
        let now_dt = Utc::now();
        let now = now_dt.to_rfc3339();
        let lease_expires_at =
            (now_dt + chrono::Duration::seconds(lease_duration_secs)).to_rfc3339();

        loop {
            let candidate_id: Option<String> = conn
                .query_row(
                    "SELECT id FROM jobs 
                     WHERE job_type = ?1 
                       AND (status = 'queued' OR (status = 'retryable' AND updated_at <= ?2))
                     ORDER BY priority DESC, created_at ASC 
                     LIMIT 1",
                    params![job_type, now],
                    |row| row.get(0),
                )
                .optional()?;

            let Some(job_id) = candidate_id else {
                return Ok(None);
            };

            let lease_token = uuid::Uuid::new_v4().to_string();

            let rows_affected = conn.execute(
                "UPDATE jobs 
                 SET status = 'running', 
                     leased_by = ?1, 
                     lease_token = ?2,
                     lease_expires_at = ?3, 
                     started_at = COALESCE(started_at, ?4), 
                     updated_at = ?4 
                 WHERE id = ?5 AND job_type = ?6 AND (
                     status = 'queued' 
                     OR (status = 'retryable' AND updated_at <= ?4)
                 )",
                params![worker_id, lease_token, lease_expires_at, now, job_id, job_type],
            )?;

            if rows_affected == 1 {
                let job = Self::get_by_id(conn, &job_id)?
                    .ok_or_else(|| JobError::Database("Job vanished after acquire".to_string()))?;
                return Ok(Some(job));
            }
        }
    }
```
In `src-tauri/src/jobs/manager.rs`, add:
```rust
    pub fn acquire_next_job_for_type(
        &self,
        job_type: &str,
        lease_duration_secs: i64,
    ) -> Result<Option<Job>, JobError> {
        let _ = self.reap_expired_leases();
        let conn = self.db.conn()?;
        JobRepository::acquire_lease_for_type(&conn, &self.worker_id, job_type, lease_duration_secs)
    }
```

- [ ] **Step 4: Run test to verify it passes**
Run: `cargo test --manifest-path src-tauri/Cargo.toml --test job_type_lease_tests`
Expected: PASS.

- [ ] **Step 5: Commit**
```bash
git add src-tauri/src/jobs/repository.rs src-tauri/src/jobs/manager.rs src-tauri/tests/job_type_lease_tests.rs
git commit -m "feat(jobs): implement atomic type-filtered job leasing"
```

---

### Task 10: FFmpeg Compiler & Filter Complex Engine with Timeline PTS Offset & Bounded Audio

**Files:**
- Create: `src-tauri/src/timeline/compiler.rs`
- Modify: `src-tauri/src/timeline/mod.rs`
- Test: `src-tauri/tests/ffmpeg_compiler_tests.rs`

**Interfaces:**
- Consumes: `RenderSpecV1`, resolved artifact map `HashMap<String, PathBuf>`, output file path `PathBuf`
- Produces: `FfmpegExecutionPlan` containing `inputs: Vec<ResolvedInput>`, `filter_complex: String`, `args: Vec<String>`, `derived_duration_ms: i64`

- [ ] **Step 1: Write failing tests a through i for compiler filter graph, PTS offsets, and audio bounding**
Create `src-tauri/tests/ffmpeg_compiler_tests.rs`:
```rust
use std::collections::HashMap;
use std::path::PathBuf;
use openvideoai_lib::timeline::model::*;
use openvideoai_lib::timeline::renderspec::*;
use openvideoai_lib::timeline::compiler::{FfmpegCompiler, FfmpegExecutionPlan};

fn create_base_spec() -> (RenderSpecV1, HashMap<String, PathBuf>, PathBuf) {
    let spec = RenderSpecV1 {
        version: 1,
        project_id: "p1".to_string(),
        timeline_id: "tl1".to_string(),
        timeline_revision: 3,
        output: RenderOutputConfig {
            width: 1920,
            height: 1080,
            frame_rate: 30.0,
            container: "mp4".to_string(),
            video_codec: "h264".to_string(),
            audio_codec: "aac".to_string(),
            pixel_format: "yuv420p".to_string(),
        },
        tracks: vec![
            RenderTrack {
                id: "tr-v0".to_string(),
                kind: TrackKind::Video,
                order: 0,
                muted: false,
                clips: vec![
                    RenderClip {
                        id: "c1".to_string(),
                        artifact_id: "art-video-1".to_string(),
                        source_in_ms: 0,
                        source_out_ms: 2000,
                        timeline_start_ms: 0,
                        volume: 1.0,
                        muted: false,
                        transform: ClipTransform::default(),
                        has_video: true,
                        has_audio: true,
                    },
                    RenderClip {
                        id: "c2".to_string(),
                        artifact_id: "art-video-2".to_string(),
                        source_in_ms: 0,
                        source_out_ms: 1000,
                        timeline_start_ms: 3500, // 1.5s gap after c1!
                        volume: 1.0,
                        muted: false,
                        transform: ClipTransform::default(),
                        has_video: true,
                        has_audio: false,
                    }
                ],
            }
        ],
    };

    let mut map = HashMap::new();
    map.insert("art-video-1".to_string(), PathBuf::from(r"C:\Media Storage\video 1.mp4"));
    map.insert("art-video-2".to_string(), PathBuf::from(r"C:\Media Storage\video 2.mp4"));
    let out = PathBuf::from(r"C:\Render Output\final render.mp4");

    (spec, map, out)
}

#[test]
fn test_a_video_with_embedded_audio() {
    let (mut spec, map, out) = create_base_spec();
    spec.tracks[0].clips.truncate(1); // Clip 1 has has_video: true, has_audio: true
    let plan = FfmpegCompiler::compile(&spec, &map, out).unwrap();

    // Video has PTS placement and scaling
    assert!(plan.filter_complex.contains("trim=start=0.000:end=2.000,setpts=PTS-STARTPTS+0.000/TB"));
    // Audio has trim, volume, adelay, and timeline duration trim
    assert!(plan.filter_complex.contains("atrim=start=0.000:end=2.000,asetpts=PTS-STARTPTS,volume=1.00,adelay=0|0:all=1"));
    assert!(plan.filter_complex.contains("atrim=end=2.000"));
}

#[test]
fn test_b_sequential_clips_with_gap() {
    let (spec, map, out) = create_base_spec();
    let plan = FfmpegCompiler::compile(&spec, &map, out).unwrap();

    // Invariant: Clip 2 placed at timelineStart 3500ms (3.5s) has video presentation timestamp placed at 3.5s!
    assert!(plan.filter_complex.contains("trim=start=0.000:end=1.000,setpts=PTS-STARTPTS+3.500/TB"));
    assert!(plan.filter_complex.contains("overlay=x=0:y=0:enable='between(t,3.500,4.500)'"));

    // Base canvas generated for total derived duration: 3.5s + 1.0s = 4.5s
    assert_eq!(plan.derived_duration_ms, 4500);
    assert!(plan.filter_complex.contains("color=c=black:s=1920x1080:r=30:d=4.500[base_v]"));
}

#[test]
fn test_c_overlapping_video_tracks() {
    let (mut spec, mut map, out) = create_base_spec();
    spec.tracks[0].clips.truncate(1); // Track 0 (order 0): c1 at 0..2000ms

    // Add Track 1 (order 1) with overlapping clip
    spec.tracks.push(RenderTrack {
        id: "tr-v1".to_string(),
        kind: TrackKind::Video,
        order: 1,
        muted: false,
        clips: vec![RenderClip {
            id: "c3".to_string(),
            artifact_id: "art-video-3".to_string(),
            source_in_ms: 0,
            source_out_ms: 1000,
            timeline_start_ms: 1000,
            volume: 1.0,
            muted: false,
            transform: ClipTransform::default(),
            has_video: true,
            has_audio: false,
        }],
    });
    map.insert("art-video-3".to_string(), PathBuf::from(r"C:\Media Storage\video 3.mp4"));

    let plan = FfmpegCompiler::compile(&spec, &map, out).unwrap();

    // Track 0 clip overlays onto [base_v], Track 1 clip overlays above Track 0
    let base_idx = plan.filter_complex.find("[base_v]").unwrap();
    let layer0_idx = plan.filter_complex.find("[v_layer_0]").unwrap();
    let out_v_idx = plan.filter_complex.find("[out_v]").unwrap();
    assert!(base_idx < layer0_idx);
    assert!(layer0_idx < out_v_idx);
}

#[test]
fn test_d_audio_only_track() {
    let (mut spec, mut map, out) = create_base_spec();
    spec.tracks.push(RenderTrack {
        id: "tr-a0".to_string(),
        kind: TrackKind::Audio,
        order: 1,
        muted: false,
        clips: vec![RenderClip {
            id: "ac1".to_string(),
            artifact_id: "art-audio-1".to_string(),
            source_in_ms: 0,
            source_out_ms: 3000,
            timeline_start_ms: 500,
            volume: 0.8,
            muted: false,
            transform: ClipTransform::default(),
            has_video: false,
            has_audio: true,
        }],
    });
    map.insert("art-audio-1".to_string(), PathBuf::from(r"C:\Media Storage\audio 1.wav"));

    let plan = FfmpegCompiler::compile(&spec, &map, out).unwrap();
    assert!(plan.filter_complex.contains("volume=0.80,adelay=500|500:all=1"));
}

#[test]
fn test_e_muted_clip() {
    let (mut spec, map, out) = create_base_spec();
    spec.tracks[0].clips[0].muted = true; // Clip 1 muted

    let plan = FfmpegCompiler::compile(&spec, &map, out).unwrap();
    // Clip 1 audio excluded
    assert!(!plan.filter_complex.contains("art-video-1"));
}

#[test]
fn test_f_non_30fps() {
    let (mut spec, map, out) = create_base_spec();
    spec.output.frame_rate = 24.0;

    let plan = FfmpegCompiler::compile(&spec, &map, out).unwrap();
    assert!(plan.filter_complex.contains(":r=24:"));
    assert!(plan.args.contains(&"24".to_string()));
}

#[test]
fn test_g_portrait_canvas() {
    let (mut spec, map, out) = create_base_spec();
    spec.output.width = 1080;
    spec.output.height = 1920;

    let plan = FfmpegCompiler::compile(&spec, &map, out).unwrap();
    assert!(plan.filter_complex.contains("s=1080x1920:"));
    assert!(plan.filter_complex.contains("scale=w=1080:h=1920:force_original_aspect_ratio=decrease,pad=1080:1920:(ow-iw)/2:(oh-ih)/2"));
}

#[test]
fn test_h_path_with_spaces_and_unicode() {
    let (mut spec, mut map, _) = create_base_spec();
    let unicode_path = PathBuf::from(r"C:\Media Storage\日本語 ビデオ 1.mp4");
    map.insert("art-video-1".to_string(), unicode_path.clone());
    let out_unicode = PathBuf::from(r"C:\Render Output\レンダリング output.mp4");

    let plan = FfmpegCompiler::compile(&spec, &map, out_unicode.clone()).unwrap();

    // Verify input args contain the raw unicode path as a single discrete item
    assert!(plan.args.contains(&unicode_path.to_string_lossy().to_string()));
    assert_eq!(plan.args.last().unwrap(), &out_unicode.to_string_lossy().to_string());
}

#[test]
fn test_i_renderspec_contains_no_physical_paths() {
    let (spec, _, _) = create_base_spec();
    let json = serde_json::to_string(&spec).unwrap();
    assert!(!json.contains("C:\"));
    assert!(!json.contains("/home/"));
    assert!(!json.contains(".mp4"));
}

#[test]
fn test_audio_mix_duration_policy_bounded_to_timeline() {
    let (mut spec, mut map, out) = create_base_spec();
    // Two audio sources where first is shorter than second
    spec.tracks[0].clips.clear();
    spec.tracks.push(RenderTrack {
        id: "tr-a0".to_string(),
        kind: TrackKind::Audio,
        order: 0,
        muted: false,
        clips: vec![
            RenderClip {
                id: "ac1".to_string(),
                artifact_id: "art-audio-short".to_string(),
                source_in_ms: 0,
                source_out_ms: 2000, // 2s
                timeline_start_ms: 0,
                volume: 1.0,
                muted: false,
                transform: ClipTransform::default(),
                has_video: false,
                has_audio: true,
            },
            RenderClip {
                id: "ac2".to_string(),
                artifact_id: "art-audio-long".to_string(),
                source_in_ms: 0,
                source_out_ms: 6000, // 6s
                timeline_start_ms: 1000,
                volume: 1.0,
                muted: false,
                transform: ClipTransform::default(),
                has_video: false,
                has_audio: true,
            },
        ],
    });
    map.insert("art-audio-short".to_string(), PathBuf::from(r"C:\Media Storage\short.wav"));
    map.insert("art-audio-long".to_string(), PathBuf::from(r"C:\Media Storage\long.wav"));

    let plan = FfmpegCompiler::compile(&spec, &map, out).unwrap();

    // Derived duration: 1000 + 6000 = 7000ms (7.000s)
    // Audio mix uses duration=longest and then atrim to exact duration
    assert!(plan.filter_complex.contains("amix=inputs=2:duration=longest"));
    assert!(plan.filter_complex.contains("atrim=end=7.000"));
}
```

- [ ] **Step 2: Run test to verify it fails**
Run: `cargo test --manifest-path src-tauri/Cargo.toml --test ffmpeg_compiler_tests`
Expected: FAIL.

- [ ] **Step 3: Implement `FfmpegCompiler` with PTS timeline offsets and longest amix bounded to timeline duration**
Implement `src-tauri/src/timeline/compiler.rs`:
```rust
use std::collections::HashMap;
use std::path::PathBuf;
use super::model::TrackKind;
use super::renderspec::{RenderClip, RenderSpecV1};
use super::error::TimelineError;

#[derive(Debug, Clone)]
pub struct ResolvedInput {
    pub index: usize,
    pub artifact_id: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct FfmpegExecutionPlan {
    pub inputs: Vec<ResolvedInput>,
    pub filter_complex: String,
    pub args: Vec<String>,
    pub output_path: PathBuf,
    pub derived_duration_ms: i64,
}

pub struct FfmpegCompiler;

impl FfmpegCompiler {
    pub fn compile(
        spec: &RenderSpecV1,
        resolved_artifacts: &HashMap<String, PathBuf>,
        output_path: PathBuf,
    ) -> Result<FfmpegExecutionPlan, TimelineError> {
        let mut inputs: Vec<ResolvedInput> = Vec::new();
        let mut artifact_to_input_idx: HashMap<String, usize> = HashMap::new();

        // 1. Gather distinct inputs in deterministic order
        for track in &spec.tracks {
            for clip in &track.clips {
                if !artifact_to_input_idx.contains_key(&clip.artifact_id) {
                    let path = resolved_artifacts.get(&clip.artifact_id)
                        .ok_or_else(|| TimelineError::ArtifactNotFound(clip.artifact_id.clone()))?;
                    let idx = inputs.len();
                    inputs.push(ResolvedInput {
                        index: idx,
                        artifact_id: clip.artifact_id.clone(),
                        path: path.clone(),
                    });
                    artifact_to_input_idx.insert(clip.artifact_id.clone(), idx);
                }
            }
        }

        // 2. Compute derived duration across all clips
        let mut max_end_ms: i64 = 0;
        for track in &spec.tracks {
            for clip in &track.clips {
                let dur = clip.source_out_ms - clip.source_in_ms;
                let end = clip.timeline_start_ms + dur;
                if end > max_end_ms {
                    max_end_ms = end;
                }
            }
        }

        if max_end_ms == 0 {
            return Err(TimelineError::RenderSpecInvalid("Timeline has zero duration".to_string()));
        }

        let dur_s = max_end_ms as f64 / 1000.0;
        let width = spec.output.width;
        let height = spec.output.height;
        let fps = spec.output.frame_rate;

        let mut filter_parts: Vec<String> = Vec::new();

        // 3. Generate base black canvas
        let fps_str = if fps.fract() == 0.0 { format!("{}", fps as u64) } else { format!("{:.2}", fps) };
        filter_parts.push(format!("color=c=black:s={}x{}:r={}:d={:.3}[base_v]", width, height, fps_str, dur_s));

        // 4. Sort video tracks by order ASC (0 is bottom)
        let mut video_tracks: Vec<_> = spec.tracks.iter().filter(|t| t.kind == TrackKind::Video && !t.muted).collect();
        video_tracks.sort_by_key(|t| t.order);

        let mut current_v_label = "[base_v]".to_string();
        let mut overlay_step = 0;

        for track in video_tracks {
            let mut clips: Vec<_> = track.clips.iter().filter(|c| !c.muted && c.has_video).cloned().collect();
            clips.sort_by_key(|c| c.timeline_start_ms);

            for clip in clips {
                let in_idx = artifact_to_input_idx[&clip.artifact_id];
                let in_s = clip.source_in_ms as f64 / 1000.0;
                let out_s = clip.source_out_ms as f64 / 1000.0;
                let start_s = clip.timeline_start_ms as f64 / 1000.0;
                let end_s = start_s + (out_s - in_s);

                let trimmed_label = format!("[v_clip_{}_trimmed]", overlay_step);
                let scaled_label = format!("[v_clip_{}_scaled]", overlay_step);
                let next_v_label = format!("[v_layer_{}]", overlay_step);

                // Trim & Timeline PTS offset: setpts=PTS-STARTPTS+<startSeconds>/TB
                filter_parts.push(format!(
                    "[{}:v]trim=start={:.3}:end={:.3},setpts=PTS-STARTPTS+{:.3}/TB{}",
                    in_idx, in_s, out_s, start_s, trimmed_label
                ));

                // Scale to fit canvas preserving aspect ratio
                filter_parts.push(format!(
                    "{}scale=w={}:h={}:force_original_aspect_ratio=decrease,pad={}:{}:(ow-iw)/2:(oh-ih)/2,setsar=1{}",
                    trimmed_label, width, height, width, height, scaled_label
                ));

                // Overlay with enable between(t, start, end)
                filter_parts.push(format!(
                    "{}{}overlay=x=0:y=0:enable='between(t,{:.3},{:.3})'{}",
                    current_v_label, scaled_label, start_s, end_s, next_v_label
                ));

                current_v_label = next_v_label;
                overlay_step += 1;
            }
        }

        if overlay_step == 0 {
            filter_parts.push(format!("{}[out_v]", current_v_label.replace("[base_v]", "[base_v]format=yuv420p")));
        } else {
            filter_parts.push(format!("{}[out_v]", current_v_label.replace(&format!("[v_layer_{}]", overlay_step - 1), &format!("[v_layer_{}]format=yuv420p", overlay_step - 1))));
        }

        // 5. Audio compilation: collect all clips with has_audio and volume > 0
        let mut audio_streams: Vec<String> = Vec::new();
        let mut a_step = 0;

        for track in &spec.tracks {
            if track.muted { continue; }

            for clip in &track.clips {
                if clip.muted || clip.volume <= 0.0 || !clip.has_audio { continue; }

                let in_idx = artifact_to_input_idx[&clip.artifact_id];
                let in_s = clip.source_in_ms as f64 / 1000.0;
                let out_s = clip.source_out_ms as f64 / 1000.0;
                let delay_ms = clip.timeline_start_ms;
                let a_label = format!("[a_{}]", a_step);

                filter_parts.push(format!(
                    "[{}:a]atrim=start={:.3}:end={:.3},asetpts=PTS-STARTPTS,volume={:.2},adelay={}|{}:all=1{}",
                    in_idx, in_s, out_s, clip.volume, delay_ms, delay_ms, a_label
                ));
                audio_streams.push(a_label);
                a_step += 1;
            }
        }

        if audio_streams.is_empty() {
            // Silence matching exact derived duration
            filter_parts.push(format!("anullsrc=r=48000:cl=stereo,atrim=end={:.3}[out_a]", dur_s));
        } else if audio_streams.len() == 1 {
            filter_parts.push(format!("{}[out_a]", audio_streams[0].replace(&audio_streams[0], &format!("{}atrim=end={:.3},aformat=sample_fmts=fltp:sample_rates=48000:channel_layouts=stereo", audio_streams[0], dur_s))));
        } else {
            let mix_inputs = audio_streams.concat();
            // amix duration=longest, strictly trimmed to timeline duration
            filter_parts.push(format!(
                "{}amix=inputs={}:duration=longest,atrim=end={:.3},aformat=sample_fmts=fltp:sample_rates=48000:channel_layouts=stereo[out_a]",
                mix_inputs, audio_streams.len(), dur_s
            ));
        }

        let filter_complex = filter_parts.join(";");

        // 6. Assemble discrete arguments array
        let mut args = vec!["-y".to_string()];
        for input in &inputs {
            args.push("-i".to_string());
            args.push(input.path.to_string_lossy().to_string());
        }

        args.push("-filter_complex".to_string());
        args.push(filter_complex.clone());
        args.push("-map".to_string());
        args.push("[out_v]".to_string());
        args.push("-map".to_string());
        args.push("[out_a]".to_string());
        args.push("-c:v".to_string());
        args.push(spec.output.video_codec.clone());
        args.push("-pix_fmt".to_string());
        args.push(spec.output.pixel_format.clone());
        args.push("-r".to_string());
        args.push(fps_str);
        args.push("-c:a".to_string());
        args.push(spec.output.audio_codec.clone());
        args.push("-b:a".to_string());
        args.push("192k".to_string());
        args.push("-ar".to_string());
        args.push("48000".to_string());
        args.push(output_path.to_string_lossy().to_string());

        Ok(FfmpegExecutionPlan {
            inputs,
            filter_complex,
            args,
            output_path,
            derived_duration_ms: max_end_ms,
        })
    }
}
```

- [ ] **Step 4: Run test to verify it passes**
Run: `cargo test --manifest-path src-tauri/Cargo.toml --test ffmpeg_compiler_tests`
Expected: PASS.

- [ ] **Step 5: Commit**
```bash
git add src-tauri/src/timeline/compiler.rs src-tauri/src/timeline/mod.rs src-tauri/tests/ffmpeg_compiler_tests.rs
git commit -m "feat(timeline): implement deterministic FFmpeg compiler with PTS timeline placement and bounded audio mix"
```

---

### Task 11: Local Render Worker Lifecycle, Heartbeat Fencing & Child Cancellation

**Files:**
- Create: `src-tauri/src/timeline/worker.rs`
- Modify: `src-tauri/src/timeline/mod.rs`
- Test: `src-tauri/tests/render_worker_tests.rs`

**Interfaces:**
- Consumes: `JobManager`, `ArtifactResolver`, `ArtifactImporter`, `RenderSpecV1`, `FfmpegCompiler`, `tools::detector::find_executable`
- Produces: `start_local_render_worker`, `LocalRenderWorkerHandle` with clean shutdown, heartbeat lease renewal, process cancellation kill, and stage checkpoints

- [ ] **Step 1: Write failing tests for public worker lifecycle orchestration, valid fixture requirement, and process cancellation cleanup**
Create `src-tauri/tests/render_worker_tests.rs`:
```rust
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tempfile::tempdir;
use rusqlite::Connection;
use openvideoai_lib::db::migrations::run_all_migrations;
use openvideoai_lib::db::DbManager;
use openvideoai_lib::storage::local_store::LocalStore;
use openvideoai_lib::artifacts::model::{Artifact, ArtifactKind};
use openvideoai_lib::artifacts::repository::ArtifactRepository;
use openvideoai_lib::artifacts::metadata::{MediaMetadata, MetadataRepository};
use openvideoai_lib::artifacts::resolver::ArtifactResolver;
use openvideoai_lib::artifacts::importer::ArtifactImporter;
use openvideoai_lib::jobs::model::{JobState, JobSubmission};
use openvideoai_lib::jobs::manager::JobManager;
use openvideoai_lib::timeline::model::*;
use openvideoai_lib::timeline::renderspec::*;
use openvideoai_lib::timeline::worker::{start_local_render_worker, LocalRenderWorker};

#[tokio::test]
async fn test_render_worker_public_loop_orchestration() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let store_dir = temp_dir.path().join("store");
    let staging_dir = temp_dir.path().join("staging");
    std::fs::create_dir_all(&store_dir).unwrap();
    std::fs::create_dir_all(&staging_dir).unwrap();

    let mut disk_conn = Connection::open(&db_path).unwrap();
    disk_conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    run_all_migrations(&mut disk_conn).unwrap();

    let db_manager = Arc::new(DbManager::new(db_path));
    let local_store = Arc::new(LocalStore::new(store_dir));
    let resolver = Arc::new(ArtifactResolver::new(db_manager.clone(), local_store.clone()));
    let importer = Arc::new(ArtifactImporter::new(db_manager.clone(), local_store.clone(), staging_dir.clone()));
    let job_manager = Arc::new(JobManager::new(db_manager.clone()));

    // 1. Create project and insert VALID video artifact fixture with metadata
    disk_conn.execute("INSERT INTO projects (id, name, created_at, updated_at) VALUES ('p1', 'P1', datetime('now'), datetime('now'));", []).unwrap();
    let source_path = temp_dir.path().join("test_video.mp4");
    std::fs::write(&source_path, b"mock video payload").unwrap();
    let imported_source = importer.import("p1", &source_path).unwrap();

    let meta = MediaMetadata {
        artifact_id: imported_source.artifact.artifact_id.clone(),
        probe_status: "ok".to_string(),
        duration_ms: Some(5000),
        width: Some(1920),
        height: Some(1080),
        video_codec: Some("h264".to_string()),
        audio_codec: Some("aac".to_string()),
        frame_rate: Some(30.0),
        sample_rate: Some(48000),
        channels: Some(2),
        probe_error: None,
    };
    MetadataRepository::insert(&disk_conn, &meta).unwrap();

    // 2. Build VALID minimal RenderSpec with 1 track, 1 clip
    let spec = RenderSpecV1 {
        version: 1,
        project_id: "p1".to_string(),
        timeline_id: "tl1".to_string(),
        timeline_revision: 1,
        output: RenderOutputConfig {
            width: 1920,
            height: 1080,
            frame_rate: 30.0,
            container: "mp4".to_string(),
            video_codec: "h264".to_string(),
            audio_codec: "aac".to_string(),
            pixel_format: "yuv420p".to_string(),
        },
        tracks: vec![RenderTrack {
            id: "tr1".to_string(),
            kind: TrackKind::Video,
            order: 0,
            muted: false,
            clips: vec![RenderClip {
                id: "c1".to_string(),
                artifact_id: imported_source.artifact.artifact_id.clone(),
                source_in_ms: 0,
                source_out_ms: 3000,
                timeline_start_ms: 0,
                volume: 1.0,
                muted: false,
                transform: ClipTransform::default(),
                has_video: true,
                has_audio: true,
            }],
        }],
    };

    let payload = serde_json::json!({
        "schemaVersion": 1,
        "renderSpec": spec,
        "rendererProfileVersion": 1,
    });

    // 3. Submit render job
    let submission = JobSubmission {
        job_type: "render.timeline".to_string(),
        priority: 10,
        idempotency_key: Some("render.timeline:tl1:1:1:1".to_string()),
        project_id: Some("p1".to_string()),
        payload: payload.to_string(),
        max_retries: 3,
        payload_version: 1,
    };
    let job = job_manager.submit_job(submission).unwrap();
    assert_eq!(job.status, JobState::Queued);

    // 4. Start worker background loop
    let handle = start_local_render_worker(
        "render-worker-loop".to_string(),
        job_manager.clone(),
        resolver.clone(),
        importer.clone(),
        staging_dir.clone(),
        PathBuf::from("mock-ffmpeg"),
        None,
        Duration::from_millis(50),
        true, // mock mode
    );

    // 5. Poll job status until Completed
    let mut completed = false;
    for _ in 0..50 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let fresh = job_manager.get_job(&job.id).unwrap().unwrap();
        if fresh.status == JobState::Completed {
            completed = true;
            break;
        }
    }
    assert!(completed, "Render worker loop failed to process job to completed state");

    // 6. Verify 7 checkpoints recorded
    let checkpoints = job_manager.get_checkpoints(&job.id).unwrap();
    let stages: Vec<String> = checkpoints.into_iter().map(|c| c.stage).collect();
    assert_eq!(stages, vec![
        "validate_spec",
        "resolve_artifacts",
        "compile_ffmpeg",
        "render_media",
        "validate_output",
        "import_output_artifact",
        "finalize",
    ]);

    // 7. Clean shutdown
    handle.shutdown().await;
}

#[tokio::test]
async fn test_render_job_cancellation_kills_child_and_cleans_up() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let store_dir = temp_dir.path().join("store");
    let staging_dir = temp_dir.path().join("staging");
    std::fs::create_dir_all(&store_dir).unwrap();
    std::fs::create_dir_all(&staging_dir).unwrap();

    let mut disk_conn = Connection::open(&db_path).unwrap();
    disk_conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    run_all_migrations(&mut disk_conn).unwrap();

    let db_manager = Arc::new(DbManager::new(db_path));
    let local_store = Arc::new(LocalStore::new(store_dir));
    let resolver = Arc::new(ArtifactResolver::new(db_manager.clone(), local_store.clone()));
    let importer = Arc::new(ArtifactImporter::new(db_manager.clone(), local_store.clone(), staging_dir.clone()));
    let job_manager = Arc::new(JobManager::new(db_manager.clone()));

    disk_conn.execute("INSERT INTO projects (id, name, created_at, updated_at) VALUES ('p1', 'P1', datetime('now'), datetime('now'));", []).unwrap();

    let submission = JobSubmission {
        job_type: "render.timeline".to_string(),
        priority: 10,
        idempotency_key: Some("render.timeline:tl1:cancel:1:1".to_string()),
        project_id: Some("p1".to_string()),
        payload: "{}".to_string(),
        max_retries: 3,
        payload_version: 1,
    };
    let job = job_manager.submit_job(submission).unwrap();

    // Acquire lease to simulate running
    let running_job = job_manager.acquire_next_job_for_type("render.timeline", 60).unwrap().unwrap();
    assert_eq!(running_job.status, JobState::Running);

    // Cancel job
    let cancelled = job_manager.cancel_job(&running_job.id).unwrap();
    assert_eq!(cancelled.status, JobState::Cancelled);

    let worker = LocalRenderWorker::new(
        "render-worker-cancel".to_string(),
        job_manager.clone(),
        resolver.clone(),
        importer.clone(),
        staging_dir.clone(),
        PathBuf::from("mock-ffmpeg"),
        None,
        false,
    );

    // Worker attempts to process cancelled job
    let err = worker.process_one_job(&running_job).unwrap_err();
    assert_eq!(err, "Job was cancelled");

    // Staging files cleaned up
    let staging_entries = std::fs::read_dir(&staging_dir).unwrap().count();
    assert_eq!(staging_entries, 0, "Staging directory must be empty after cancellation");

    // No artifact created for cancelled job
    let artifacts = ArtifactRepository::list_for_project(&disk_conn, "p1").unwrap();
    assert_eq!(artifacts.len(), 0, "No artifact should be created for cancelled job");

    // Attempting to complete cancelled job with lease token is rejected by CAS
    let complete_res = job_manager.complete_job_with_token(&running_job.id, running_job.lease_token.as_deref(), Some("{}".to_string()));
    assert!(complete_res.is_err(), "Cannot complete a cancelled job");
}
```

- [ ] **Step 2: Run test to verify it fails**
Run: `cargo test --manifest-path src-tauri/Cargo.toml --test render_worker_tests`
Expected: FAIL.

- [ ] **Step 3: Implement `start_local_render_worker` and `LocalRenderWorker`**
Implement `src-tauri/src/timeline/worker.rs`:
```rust
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use uuid::Uuid;
use crate::artifacts::importer::ArtifactImporter;
use crate::artifacts::metadata::probe_media;
use crate::artifacts::model::ArtifactRef;
use crate::artifacts::resolver::ArtifactResolver;
use crate::jobs::manager::JobManager;
use crate::jobs::model::{Job, JobState};
use super::compiler::FfmpegCompiler;
use super::renderspec::RenderSpecV1;

pub struct LocalRenderWorkerHandle {
    shutdown_tx: watch::Sender<bool>,
    join_handle: JoinHandle<()>,
}

impl LocalRenderWorkerHandle {
    pub async fn shutdown(self) {
        let _ = self.shutdown_tx.send(true);
        let _ = self.join_handle.await;
    }
}

pub struct LocalRenderWorker {
    pub worker_id: String,
    pub job_manager: Arc<JobManager>,
    pub resolver: Arc<ArtifactResolver>,
    pub importer: Arc<ArtifactImporter>,
    pub staging_dir: PathBuf,
    pub ffmpeg_path: PathBuf,
    pub ffprobe_path: Option<PathBuf>,
    pub is_mock: bool,
}

impl LocalRenderWorker {
    pub fn new(
        worker_id: String,
        job_manager: Arc<JobManager>,
        resolver: Arc<ArtifactResolver>,
        importer: Arc<ArtifactImporter>,
        staging_dir: PathBuf,
        ffmpeg_path: PathBuf,
        ffprobe_path: Option<PathBuf>,
        is_mock: bool,
    ) -> Self {
        Self {
            worker_id,
            job_manager,
            resolver,
            importer,
            staging_dir,
            ffmpeg_path,
            ffprobe_path,
            is_mock,
        }
    }

    pub fn process_one_job(&self, job: &Job) -> Result<ArtifactRef, String> {
        let current = self.job_manager.get_job(&job.id).map_err(|e| e.to_string())?
            .ok_or_else(|| "Job not found".to_string())?;
        if current.status == JobState::Cancelled {
            return Err("Job was cancelled".to_string());
        }

        // 1. validate_spec
        self.job_manager.record_checkpoint_with_token(&job.id, job.lease_token.as_deref(), "validate_spec", "{}")
            .map_err(|e| e.to_string())?;

        let temp_output = self.staging_dir.join(format!("output_{}.mp4", Uuid::new_v4()));

        if self.is_mock {
            // 2. resolve_artifacts
            self.job_manager.record_checkpoint_with_token(&job.id, job.lease_token.as_deref(), "resolve_artifacts", "{}")
                .map_err(|e| e.to_string())?;
            // 3. compile_ffmpeg
            self.job_manager.record_checkpoint_with_token(&job.id, job.lease_token.as_deref(), "compile_ffmpeg", "{}")
                .map_err(|e| e.to_string())?;
            // 4. render_media
            self.job_manager.record_checkpoint_with_token(&job.id, job.lease_token.as_deref(), "render_media", "{}")
                .map_err(|e| e.to_string())?;

            std::fs::write(&temp_output, b"mock render output mp4").map_err(|e| e.to_string())?;

            // 5. validate_output
            self.job_manager.record_checkpoint_with_token(&job.id, job.lease_token.as_deref(), "validate_output", "{}")
                .map_err(|e| e.to_string())?;
            // 6. import_output_artifact
            self.job_manager.record_checkpoint_with_token(&job.id, job.lease_token.as_deref(), "import_output_artifact", "{}")
                .map_err(|e| e.to_string())?;

            let project_id = job.project_id.as_deref().unwrap_or("p1");
            let imported = self.importer.import(project_id, &temp_output).map_err(|e| e.to_string())?;
            let _ = std::fs::remove_file(&temp_output);

            // 7. finalize
            self.job_manager.record_checkpoint_with_token(&job.id, job.lease_token.as_deref(), "finalize", "{}")
                .map_err(|e| e.to_string())?;

            let result_json = serde_json::to_string(&imported.artifact).map_err(|e| e.to_string())?;
            self.job_manager.complete_job_with_token(&job.id, job.lease_token.as_deref(), Some(result_json))
                .map_err(|e| e.to_string())?;

            return Ok(imported.artifact);
        }

        let parsed_payload: serde_json::Value = serde_json::from_str(&job.payload).map_err(|e| e.to_string())?;
        let spec: RenderSpecV1 = serde_json::from_value(parsed_payload["renderSpec"].clone())
            .map_err(|e| format!("Invalid RenderSpec: {}", e))?;

        // 2. resolve_artifacts
        self.job_manager.record_checkpoint_with_token(&job.id, job.lease_token.as_deref(), "resolve_artifacts", "{}")
            .map_err(|e| e.to_string())?;
        let mut resolved_map = HashMap::new();
        for track in &spec.tracks {
            for clip in &track.clips {
                if !resolved_map.contains_key(&clip.artifact_id) {
                    let path = self.resolver.resolve_uri(&format!("artifact://{}", clip.artifact_id))
                        .map_err(|e| format!("Artifact {} not found: {}", clip.artifact_id, e))?;
                    resolved_map.insert(clip.artifact_id.clone(), path);
                }
            }
        }

        // 3. compile_ffmpeg
        self.job_manager.record_checkpoint_with_token(&job.id, job.lease_token.as_deref(), "compile_ffmpeg", "{}")
            .map_err(|e| e.to_string())?;
        let plan = FfmpegCompiler::compile(&spec, &resolved_map, temp_output.clone())
            .map_err(|e| e.to_string())?;

        // 4. render_media
        self.job_manager.record_checkpoint_with_token(&job.id, job.lease_token.as_deref(), "render_media", "{}")
            .map_err(|e| e.to_string())?;

        let mut child = Command::new(&self.ffmpeg_path)
            .args(&plan.args)
            .spawn()
            .map_err(|e| format!("Failed to spawn FFmpeg: {}", e))?;

        let status = loop {
            match child.try_wait() {
                Ok(Some(s)) => break s,
                Ok(None) => {
                    std::thread::sleep(Duration::from_millis(500));
                    match self.job_manager.heartbeat_with_token(&job.id, job.lease_token.as_deref(), 60) {
                        Ok(true) => {
                            if let Ok(Some(j)) = self.job_manager.get_job(&job.id) {
                                if j.status == JobState::Cancelled {
                                    let _ = child.kill();
                                    let _ = std::fs::remove_file(&temp_output);
                                    return Err("Job was cancelled".to_string());
                                }
                            }
                        }
                        _ => {
                            let _ = child.kill();
                            let _ = std::fs::remove_file(&temp_output);
                            return Err("Lost job lease fence".to_string());
                        }
                    }
                }
                Err(e) => {
                    let _ = child.kill();
                    let _ = std::fs::remove_file(&temp_output);
                    return Err(format!("Error waiting on FFmpeg: {}", e));
                }
            }
        };

        if !status.success() {
            let _ = std::fs::remove_file(&temp_output);
            return Err("FFmpeg exited with error".to_string());
        }

        // 5. validate_output
        self.job_manager.record_checkpoint_with_token(&job.id, job.lease_token.as_deref(), "validate_output", "{}")
            .map_err(|e| e.to_string())?;
        if !temp_output.is_file() || std::fs::metadata(&temp_output).map(|m| m.len()).unwrap_or(0) == 0 {
            let _ = std::fs::remove_file(&temp_output);
            return Err("Render output file is empty or missing".to_string());
        }

        let probe = probe_media("render_probe", &temp_output, self.ffprobe_path.as_deref());
        if probe.probe_status == "ok" && probe.duration_ms.unwrap_or(0) == 0 {
            let _ = std::fs::remove_file(&temp_output);
            return Err("Rendered media probe reported 0 duration".to_string());
        }

        // 6. import_output_artifact
        self.job_manager.record_checkpoint_with_token(&job.id, job.lease_token.as_deref(), "import_output_artifact", "{}")
            .map_err(|e| e.to_string())?;
        let imported = self.importer.import(&spec.project_id, &temp_output).map_err(|e| e.to_string())?;
        let _ = std::fs::remove_file(&temp_output);

        // 7. finalize
        self.job_manager.record_checkpoint_with_token(&job.id, job.lease_token.as_deref(), "finalize", "{}")
            .map_err(|e| e.to_string())?;
        let result_json = serde_json::to_string(&imported.artifact).map_err(|e| e.to_string())?;
        self.job_manager.complete_job_with_token(&job.id, job.lease_token.as_deref(), Some(result_json))
            .map_err(|e| e.to_string())?;

        Ok(imported.artifact)
    }
}

pub fn start_local_render_worker(
    worker_id: String,
    job_manager: Arc<JobManager>,
    resolver: Arc<ArtifactResolver>,
    importer: Arc<ArtifactImporter>,
    staging_dir: PathBuf,
    ffmpeg_path: PathBuf,
    ffprobe_path: Option<PathBuf>,
    poll_interval: Duration,
    is_mock: bool,
) -> LocalRenderWorkerHandle {
    let (shutdown_tx, mut shutdown_rx) = watch::channel(false);
    let worker = Arc::new(LocalRenderWorker::new(
        worker_id,
        job_manager.clone(),
        resolver,
        importer,
        staging_dir,
        ffmpeg_path,
        ffprobe_path,
        is_mock,
    ));

    let join_handle = tokio::spawn(async move {
        loop {
            if *shutdown_rx.borrow() {
                break;
            }

            match job_manager.acquire_next_job_for_type("render.timeline", 60) {
                Ok(Some(job)) => {
                    let w = worker.clone();
                    let _ = tokio::task::spawn_blocking(move || {
                        let _ = w.process_one_job(&job);
                    }).await;
                }
                Ok(None) => {}
                Err(e) => {
                    tracing::warn!("Render worker failed to acquire job: {}", e);
                }
            }

            tokio::select! {
                _ = tokio::time::sleep(poll_interval) => {}
                _ = shutdown_rx.changed() => {
                    if *shutdown_rx.borrow() {
                        break;
                    }
                }
            }
        }
    });

    LocalRenderWorkerHandle {
        shutdown_tx,
        join_handle,
    }
}
```

- [ ] **Step 4: Run test to verify it passes**
Run: `cargo test --manifest-path src-tauri/Cargo.toml --test render_worker_tests`
Expected: PASS.

- [ ] **Step 5: Commit**
```bash
git add src-tauri/src/timeline/worker.rs src-tauri/src/timeline/mod.rs src-tauri/tests/render_worker_tests.rs
git commit -m "feat(timeline): implement LocalRenderWorker background loop, type-safe lease acquisition, and cancellation handling"
```

---

### Task 12: Backend IPC Commands & Tauri Registration

**Files:**
- Create: `src-tauri/src/timeline/commands.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/tests/timeline_ipc_tests.rs`

**Interfaces:**
- Consumes: `TimelineService`, `JobManager`, `RenderSpecBuilder`, `DbManager`, `MetadataRepository`
- Produces: Tauri IPC commands `create_timeline`, `get_timeline`, `list_project_timelines`, `delete_timeline`, `apply_timeline_mutation`, `undo_timeline`, `redo_timeline`, `build_render_spec`, `submit_timeline_render`

- [ ] **Step 1: Write failing tests for IPC command handlers**
Create `src-tauri/tests/timeline_ipc_tests.rs`:
```rust
use std::sync::Arc;
use tempfile::tempdir;
use rusqlite::Connection;
use openvideoai_lib::db::migrations::run_all_migrations;
use openvideoai_lib::db::DbManager;
use openvideoai_lib::projects::model::Project;
use openvideoai_lib::projects::repository::ProjectRepository;
use openvideoai_lib::timeline::model::*;
use openvideoai_lib::timeline::commands::*;
use openvideoai_lib::jobs::manager::JobManager;
use openvideoai_lib::timeline::mutation::TimelineMutation;

#[test]
fn test_ipc_timeline_crud_and_mutations() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut conn = Connection::open(&db_path).unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    run_all_migrations(&mut conn).unwrap();

    let project = Project::new("p1".to_string(), "P1".to_string());
    ProjectRepository::create(&conn, &project).unwrap();

    let db_manager = Arc::new(DbManager::new(db_path));

    // 1. Create timeline via command
    let doc = cmd_create_timeline(&db_manager, "p1", "Main", 1920, 1080, 30.0).unwrap();
    assert_eq!(doc.revision, 1);
    assert_eq!(doc.name, "Main");

    // 2. Fetch timeline
    let fetched = cmd_get_timeline(&db_manager, &doc.id).unwrap().unwrap();
    assert_eq!(fetched.id, doc.id);

    // 3. Apply mutation
    let updated = cmd_apply_timeline_mutation(&db_manager, &doc.id, 1, TimelineMutation::AddTrack {
        kind: TrackKind::Video,
        name: "V1".to_string(),
    }).unwrap();
    assert_eq!(updated.revision, 2);
    assert_eq!(updated.tracks.len(), 1);

    // 4. Undo mutation with matching expected revision
    let undone = cmd_undo_timeline(&db_manager, &doc.id, 2).unwrap();
    assert_eq!(undone.revision, 3);
    assert_eq!(undone.tracks.len(), 0);

    // 5. Redo mutation with matching expected revision
    let redone = cmd_redo_timeline(&db_manager, &doc.id, 3).unwrap();
    assert_eq!(redone.revision, 4);
    assert_eq!(redone.tracks.len(), 1);
}
```

- [ ] **Step 2: Run test to verify it fails**
Run: `cargo test --manifest-path src-tauri/Cargo.toml --test timeline_ipc_tests`
Expected: FAIL.

- [ ] **Step 3: Implement IPC commands in `src-tauri/src/timeline/commands.rs`**
Implement typed commands:
```rust
use std::sync::Arc;
use tauri::State;
use crate::db::DbManager;
use crate::jobs::manager::JobManager;
use crate::jobs::model::JobSubmission;
use super::model::*;
use super::mutation::TimelineMutation;
use super::repository::TimelineRepository;
use super::service::TimelineService;
use super::renderspec::RenderSpecBuilder;
use super::error::TimelineError;

#[tauri::command]
pub fn cmd_create_timeline(
    db: &Arc<DbManager>,
    project_id: &str,
    name: &str,
    width: u32,
    height: u32,
    frame_rate: f64,
) -> Result<TimelineDocument, String> {
    let mut conn = db.conn().map_err(|e| e.to_string())?;
    let id = format!("timeline-{}", uuid::Uuid::new_v4());
    let canvas = CanvasConfig { width, height, frame_rate };
    TimelineRepository::create(&mut conn, &id, project_id, name, canvas).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn cmd_get_timeline(
    db: &Arc<DbManager>,
    timeline_id: &str,
) -> Result<Option<TimelineDocument>, String> {
    let conn = db.conn().map_err(|e| e.to_string())?;
    TimelineRepository::get_by_id(&conn, timeline_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn cmd_list_project_timelines(
    db: &Arc<DbManager>,
    project_id: &str,
) -> Result<Vec<TimelineDocument>, String> {
    let conn = db.conn().map_err(|e| e.to_string())?;
    TimelineRepository::list_by_project(&conn, project_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn cmd_delete_timeline(
    db: &Arc<DbManager>,
    timeline_id: &str,
) -> Result<bool, String> {
    let conn = db.conn().map_err(|e| e.to_string())?;
    TimelineRepository::delete(&conn, timeline_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn cmd_apply_timeline_mutation(
    db: &Arc<DbManager>,
    timeline_id: &str,
    expected_revision: i64,
    mutation: TimelineMutation,
) -> Result<TimelineDocument, String> {
    let mut conn = db.conn().map_err(|e| e.to_string())?;
    let service = TimelineService::new();
    service.apply_mutation(&mut conn, timeline_id, expected_revision, mutation).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn cmd_undo_timeline(
    db: &Arc<DbManager>,
    timeline_id: &str,
    expected_revision: i64,
) -> Result<TimelineDocument, String> {
    let mut conn = db.conn().map_err(|e| e.to_string())?;
    let service = TimelineService::new();
    service.undo(&mut conn, timeline_id, expected_revision).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn cmd_redo_timeline(
    db: &Arc<DbManager>,
    timeline_id: &str,
    expected_revision: i64,
) -> Result<TimelineDocument, String> {
    let mut conn = db.conn().map_err(|e| e.to_string())?;
    let service = TimelineService::new();
    service.redo(&mut conn, timeline_id, expected_revision).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn cmd_submit_timeline_render(
    db: &Arc<DbManager>,
    job_manager: &Arc<JobManager>,
    timeline_id: &str,
    expected_revision: i64,
) -> Result<String, String> {
    let conn = db.conn().map_err(|e| e.to_string())?;
    let doc = TimelineRepository::get_by_id(&conn, timeline_id).map_err(|e| e.to_string())?
        .ok_or_else(|| "Timeline not found".to_string())?;

    if doc.revision != expected_revision {
        return Err(format!("Revision conflict: expected {}, actual {}", expected_revision, doc.revision));
    }

    let spec = RenderSpecBuilder::build_from_timeline(&doc, &conn).map_err(|e| e.to_string())?;

    let payload = serde_json::json!({
        "schemaVersion": 1,
        "renderSpec": spec,
        "rendererProfileVersion": 1,
    });

    let idempotency_key = format!("render.timeline:{}:{}:1:1", doc.id, doc.revision);
    let submission = JobSubmission {
        job_type: "render.timeline".to_string(),
        priority: 10,
        idempotency_key: Some(idempotency_key),
        project_id: Some(doc.project_id),
        payload: payload.to_string(),
        max_retries: 3,
        payload_version: 1,
    };

    let job = job_manager.submit_job(submission).map_err(|e| e.to_string())?;
    Ok(job.id)
}
```
Register commands in `src-tauri/src/lib.rs`.

- [ ] **Step 4: Run test to verify it passes**
Run: `cargo test --manifest-path src-tauri/Cargo.toml --test timeline_ipc_tests`
Expected: PASS.

- [ ] **Step 5: Commit**
```bash
git add src-tauri/src/timeline/commands.rs src-tauri/src/lib.rs src-tauri/tests/timeline_ipc_tests.rs
git commit -m "feat(timeline): implement Tauri IPC commands for timeline CRUD, mutations, and render submission"
```

---

### Task 13: Minimal Timeline Editor UI & Frontend Integration

**Files:**
- Create: `src/types/timeline.ts`
- Create: `src/services/timeline.ts`
- Create: `src/components/editor/EditorLayout.tsx`
- Create: `src/components/editor/AssetBin.tsx`
- Create: `src/components/editor/PreviewPlayer.tsx`
- Create: `src/components/editor/TimelineTrackView.tsx`
- Create: `src/components/editor/TimelineClipView.tsx`
- Create: `src/components/editor/TimelineToolbar.tsx`
- Create: `src/components/editor/EditorView.tsx`
- Modify: `src/App.tsx`
- Test: `tests/unit/timeline-service.test.ts`
- Test: `tests/unit/editor-ui.test.tsx`

**Interfaces:**
- Consumes: `@tauri-apps/api/core` invoke, `openvideoai-media://artifact/<id>` protocol
- Produces: Interactive editor UI with Asset Bin, Preview Player, Multi-track Timeline, Trim/Split/Move interactions, Undo/Redo, Canvas Presets, and Render execution

- [ ] **Step 1: Write frontend unit tests for Timeline Service and Editor UI**
Create `tests/unit/timeline-service.test.ts`:
```typescript
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { timelineService } from '../../src/services/timeline';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

describe('timelineService', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('creates timeline with default canvas', async () => {
    const mockTimeline = {
      id: 'tl1',
      projectId: 'p1',
      name: 'Main Timeline',
      revision: 1,
      canvas: { width: 1920, height: 1080, frameRate: 30.0 },
      tracks: [],
      createdAt: '2026-09-10T12:00:00Z',
      updatedAt: '2026-09-10T12:00:00Z',
    };
    vi.mocked(invoke).mockResolvedValueOnce(mockTimeline);

    const res = await timelineService.createTimeline('p1', 'Main Timeline', 1920, 1080, 30.0);
    expect(invoke).toHaveBeenCalledWith('cmd_create_timeline', {
      projectId: 'p1',
      name: 'Main Timeline',
      width: 1920,
      height: 1080,
      frameRate: 30.0,
    });
    expect(res.id).toBe('tl1');
  });

  it('applies mutation with expected revision', async () => {
    const updated = {
      id: 'tl1',
      revision: 2,
      tracks: [{ id: 'tr1', name: 'V1', kind: 'video', order: 0, muted: false, clips: [] }],
    };
    vi.mocked(invoke).mockResolvedValueOnce(updated);

    const mutation = { type: 'add_track', kind: 'video', name: 'V1' };
    const res = await timelineService.applyMutation('tl1', 1, mutation);

    expect(invoke).toHaveBeenCalledWith('cmd_apply_timeline_mutation', {
      timelineId: 'tl1',
      expectedRevision: 1,
      mutation,
    });
    expect(res.revision).toBe(2);
  });

  it('submits render with expected revision', async () => {
    vi.mocked(invoke).mockResolvedValueOnce('job-render-123');

    const jobId = await timelineService.submitRender('tl1', 2);
    expect(invoke).toHaveBeenCalledWith('cmd_submit_timeline_render', {
      timelineId: 'tl1',
      expectedRevision: 2,
    });
    expect(jobId).toBe('job-render-123');
  });
});
```

Create `tests/unit/editor-ui.test.tsx`:
```typescript
import React from 'react';
import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { EditorView } from '../../src/components/editor/EditorView';
import { timelineService } from '../../src/services/timeline';

vi.mock('../../src/services/timeline', () => ({
  timelineService: {
    getTimeline: vi.fn(),
    applyMutation: vi.fn(),
    undoTimeline: vi.fn(),
    redoTimeline: vi.fn(),
    submitRender: vi.fn(),
  },
}));

describe('EditorView', () => {
  it('renders toolbar, preview player, asset bin, and timeline tracks', async () => {
    const mockDoc = {
      id: 'tl1',
      projectId: 'p1',
      name: 'Editor Timeline',
      revision: 1,
      canvas: { width: 1920, height: 1080, frameRate: 30.0 },
      tracks: [
        { id: 'tr1', timelineId: 'tl1', kind: 'video', order: 0, name: 'V1', muted: false, clips: [] },
      ],
      createdAt: '2026-09-10T12:00:00Z',
      updatedAt: '2026-09-10T12:00:00Z',
    };
    vi.mocked(timelineService.getTimeline).mockResolvedValue(mockDoc);

    render(<EditorView projectId="p1" initialTimelineId="tl1" />);

    expect(await screen.findByText('Editor Timeline')).toBeInTheDocument();
    expect(screen.getByText('Render')).toBeInTheDocument();
    expect(screen.getByText('V1')).toBeInTheDocument();
  });

  it('handles undo and redo clicks with expected revision', async () => {
    const mockDoc = {
      id: 'tl1',
      projectId: 'p1',
      name: 'Editor Timeline',
      revision: 2,
      canvas: { width: 1920, height: 1080, frameRate: 30.0 },
      tracks: [],
      createdAt: '2026-09-10T12:00:00Z',
      updatedAt: '2026-09-10T12:00:00Z',
    };
    vi.mocked(timelineService.getTimeline).mockResolvedValue(mockDoc);
    vi.mocked(timelineService.undoTimeline).mockResolvedValue({ ...mockDoc, revision: 3 });

    render(<EditorView projectId="p1" initialTimelineId="tl1" />);

    const undoBtn = await screen.findByTitle('Undo');
    fireEvent.click(undoBtn);

    expect(timelineService.undoTimeline).toHaveBeenCalledWith('tl1', 2);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**
Run: `npm test`
Expected: FAIL.

- [ ] **Step 3: Implement TypeScript types, service, and editor components**
Implement `src/types/timeline.ts`, `src/services/timeline.ts`, and editor components with responsive Tailwind CSS.

- [ ] **Step 4: Run frontend tests and build verification**
Run: `npm test`
Run: `npm run build`
Expected: PASS (exit code 0).

- [ ] **Step 5: Commit**
```bash
git add src/types/timeline.ts src/services/timeline.ts src/components/editor/ src/App.tsx tests/unit/timeline-service.test.ts tests/unit/editor-ui.test.tsx
git commit -m "feat(ui): implement minimal multi-track timeline editor, asset bin, preview player, and render trigger"
```

---

### Task 14: End-to-End Synthetic FFmpeg Render Acceptance Test

**Files:**
- Create: `src-tauri/tests/render_acceptance_tests.rs`

**Interfaces:**
- Consumes: Host FFmpeg/FFprobe, `LocalRenderWorker`, `JobManager`, `ArtifactImporter`, `ArtifactResolver`
- Produces: Synthetic end-to-end render proof of 4500ms duration (Clip A 2s + 1.5s gap + Clip B 1s) validating MP4 container, H.264 video, AAC audio, and database reload persistence

- [ ] **Step 1: Write the end-to-end acceptance test**
Create `src-tauri/tests/render_acceptance_tests.rs`:
```rust
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::tempdir;
use rusqlite::Connection;
use openvideoai_lib::db::migrations::run_all_migrations;
use openvideoai_lib::db::DbManager;
use openvideoai_lib::storage::local_store::LocalStore;
use openvideoai_lib::artifacts::model::{Artifact, ArtifactKind};
use openvideoai_lib::artifacts::repository::ArtifactRepository;
use openvideoai_lib::artifacts::metadata::{probe_media, MetadataRepository};
use openvideoai_lib::artifacts::resolver::ArtifactResolver;
use openvideoai_lib::artifacts::importer::ArtifactImporter;
use openvideoai_lib::jobs::model::JobSubmission;
use openvideoai_lib::jobs::manager::JobManager;
use openvideoai_lib::timeline::model::*;
use openvideoai_lib::timeline::repository::TimelineRepository;
use openvideoai_lib::timeline::service::TimelineService;
use openvideoai_lib::timeline::mutation::TimelineMutation;
use openvideoai_lib::timeline::renderspec::RenderSpecBuilder;
use openvideoai_lib::timeline::worker::LocalRenderWorker;
use openvideoai_lib::tools::detector::find_executable;

#[test]
fn test_e2e_synthetic_ffmpeg_render_acceptance() {
    let ffmpeg_exe = match find_executable("ffmpeg", None) {
        Some(p) => p,
        None => {
            eprintln!("SKIP: ffmpeg not detected on host system");
            return;
        }
    };
    let ffprobe_exe = find_executable("ffprobe", None);

    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let store_dir = temp_dir.path().join("store");
    let staging_dir = temp_dir.path().join("staging");
    std::fs::create_dir_all(&store_dir).unwrap();
    std::fs::create_dir_all(&staging_dir).unwrap();

    let mut disk_conn = Connection::open(&db_path).unwrap();
    disk_conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    run_all_migrations(&mut disk_conn).unwrap();

    let db_manager = Arc::new(DbManager::new(db_path.clone()));
    let local_store = Arc::new(LocalStore::new(store_dir));
    let resolver = Arc::new(ArtifactResolver::new(db_manager.clone(), local_store.clone()));
    let importer = Arc::new(ArtifactImporter::new(db_manager.clone(), local_store.clone(), staging_dir.clone()));
    let job_manager = Arc::new(JobManager::new(db_manager.clone()));
    let service = TimelineService::new();

    // 1. Generate synthetic video fixture 1 (2s 320x240 @ 30fps with 1000Hz test audio)
    let clip_a_src = temp_dir.path().join("clip_a.mp4");
    let status_a = std::process::Command::new(&ffmpeg_exe)
        .args(&[
            "-y", "-f", "lavfi", "-i", "testsrc=size=320x240:rate=30:duration=2",
            "-f", "lavfi", "-i", "sine=frequency=1000:duration=2",
            "-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "aac",
            clip_a_src.to_str().unwrap()
        ])
        .status()
        .unwrap();
    assert!(status_a.success(), "Failed to generate fixture Clip A");

    // Generate synthetic video fixture 2 (1s 320x240 @ 30fps with 440Hz test audio)
    let clip_b_src = temp_dir.path().join("clip_b.mp4");
    let status_b = std::process::Command::new(&ffmpeg_exe)
        .args(&[
            "-y", "-f", "lavfi", "-i", "testsrc=size=320x240:rate=30:duration=1",
            "-f", "lavfi", "-i", "sine=frequency=440:duration=1",
            "-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "aac",
            clip_b_src.to_str().unwrap()
        ])
        .status()
        .unwrap();
    assert!(status_b.success(), "Failed to generate fixture Clip B");

    // 2. Import into Artifact Core
    disk_conn.execute("INSERT INTO projects (id, name, created_at, updated_at) VALUES ('p1', 'Acceptance Project', datetime('now'), datetime('now'));", []).unwrap();
    let imported_a = importer.import("p1", &clip_a_src).unwrap();
    let imported_b = importer.import("p1", &clip_b_src).unwrap();

    // 3. Create Timeline (320x240, 30fps)
    let canvas = CanvasConfig { width: 320, height: 240, frame_rate: 30.0 };
    let tl = TimelineRepository::create(&mut disk_conn, "tl_acc", "p1", "Acceptance Timeline", canvas).unwrap();

    // Add Track V1
    let tl = service.apply_mutation(&mut disk_conn, &tl.id, 1, TimelineMutation::AddTrack {
        kind: TrackKind::Video,
        name: "V1".to_string(),
    }).unwrap();
    let v1_id = tl.tracks[0].id.clone();

    // Add Clip A: 0..2000ms at timelineStart 0ms
    let tl = service.apply_mutation(&mut disk_conn, &tl.id, 2, TimelineMutation::AddClip {
        track_id: v1_id.clone(),
        artifact_id: imported_a.artifact.artifact_id.clone(),
        timeline_start_ms: 0,
        source_in_ms: Some(0),
        source_out_ms: Some(2000),
    }).unwrap();

    // Add Clip B: 0..1000ms at timelineStart 3500ms (introducing 1500ms black canvas gap!)
    let tl = service.apply_mutation(&mut disk_conn, &tl.id, 3, TimelineMutation::AddClip {
        track_id: v1_id,
        artifact_id: imported_b.artifact.artifact_id.clone(),
        timeline_start_ms: 3500,
        source_in_ms: Some(0),
        source_out_ms: Some(1000),
    }).unwrap();

    // Derived duration math: 3500ms + 1000ms = 4500ms (4.5s)
    assert_eq!(tl.derived_duration_ms(), 4500);

    // 4. Build RenderSpec v1 with stream capabilities bound from metadata
    let spec = RenderSpecBuilder::build_from_timeline(&tl, &disk_conn).unwrap();
    assert_eq!(spec.output.width, 320);
    assert_eq!(spec.output.height, 240);

    // 5. Submit render job
    let payload = serde_json::json!({
        "schemaVersion": 1,
        "renderSpec": spec,
        "rendererProfileVersion": 1,
    });
    let submission = JobSubmission {
        job_type: "render.timeline".to_string(),
        priority: 10,
        idempotency_key: Some(format!("render.timeline:tl_acc:{}:1:1", tl.revision)),
        project_id: Some("p1".to_string()),
        payload: payload.to_string(),
        max_retries: 3,
        payload_version: 1,
    };
    let job = job_manager.submit_job(submission).unwrap();

    // 6. Execute through LocalRenderWorker
    let worker = LocalRenderWorker::new(
        "render-worker-acceptance".to_string(),
        job_manager.clone(),
        resolver.clone(),
        importer.clone(),
        staging_dir.clone(),
        ffmpeg_exe.clone(),
        ffprobe_exe.clone(),
        false, // real mode
    );

    let output_artifact = worker.process_one_job(&job).unwrap();
    assert_eq!(output_artifact.kind, ArtifactKind::Video);

    // 7. Validate rendered output via FFprobe
    let output_path = resolver.resolve_uri(&output_artifact.uri).unwrap();
    assert!(output_path.is_file());
    assert!(output_path.metadata().unwrap().len() > 0);

    let meta = probe_media(&output_artifact.artifact_id, &output_path, ffprobe_exe.as_deref());
    if meta.probe_status == "ok" {
        assert_eq!(meta.video_codec.as_deref(), Some("h264"));
        assert_eq!(meta.audio_codec.as_deref(), Some("aac"));
        assert_eq!(meta.width, Some(320));
        assert_eq!(meta.height, Some(240));
        // Verify 4500ms duration with tolerance of 100ms
        let dur = meta.duration_ms.expect("Duration must be probed");
        assert!((dur - 4500).abs() <= 100, "Expected ~4500ms duration, got: {}", dur);
    }

    // 8. Verify timeline state persists across clean DB reopen
    drop(disk_conn);
    let reopened = Connection::open(&db_path).unwrap();
    let reloaded = TimelineRepository::get_by_id(&reopened, "tl_acc").unwrap().unwrap();
    assert_eq!(reloaded.revision, 4);
    assert_eq!(reloaded.derived_duration_ms(), 4500);
    assert_eq!(reloaded.tracks[0].clips.len(), 2);
}
```

- [ ] **Step 2: Run acceptance test**
Run: `cargo test --manifest-path src-tauri/Cargo.toml --test render_acceptance_tests -- --nocapture`
Expected: PASS (or documented skip if ffmpeg is unavailable).

- [ ] **Step 3: Commit**
```bash
git add src-tauri/tests/render_acceptance_tests.rs
git commit -m "test(acceptance): add end-to-end synthetic FFmpeg render test with exact duration validation"
```


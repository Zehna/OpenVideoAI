# OpenVideoAI Phase 1 — Project & Artifact Core Design Specification

- **Date:** 2026-09-08
- **Phase:** 1
- **Target Version:** v0.2.0
- **Base Version / Tag:** `v0.1.0` (`cb22c51`)
- **Branch:** `phase/1-project-artifact-core`

---

## 1. Executive Summary & Goals

The goal of Phase 1 is to establish a rock-solid, local-first **Project & Artifact Core** for OpenVideoAI. It builds directly upon the Phase 0 desktop foundation (Tauri 2, SQLite WAL mode, structured tracing, and tool detection) to deliver:

1. **Durable Project Lifecycle:** Create, list, retrieve, and update projects stored reliably in SQLite with transactional consistency.
2. **Global Content-Addressed Artifact Store:** Ingest media files (video, audio, image, subtitle, json, other) into a global content-addressed filesystem hierarchy (`artifacts/<sha256[0:2]>/<sha256>`), preventing duplicate physical files across or within projects.
3. **Streaming Cryptographic Hashing:** Compute SHA-256 during staging without loading full media files into memory.
4. **Abstract Artifact Identity:** Expose assets to frontend and future modules strictly via `ArtifactRef` (`artifact://<artifactId>`). Physical filesystem paths remain an internal storage detail and are never leaked to public contracts.
5. **Non-Blocking Metadata Probing:** Query media streams and container metadata via local `ffprobe` when available, gracefully degrading to `unavailable` or `failed` without crashing imports or corrupting state.
6. **Autosave & Startup Reconciliation:** Debounce project title and description edits on the frontend; automatically reconcile and sweep leftover staging files on application startup.
7. **Minimal Interactive Projects & Assets UI:** Provide a streamlined desktop view to create projects, switch active projects, trigger file imports, inspect media metadata, and verify cross-project deduplication end-to-end.

---

## 2. Scope Boundaries

### In Scope
- SQLite schema migration version 2 (`projects`, `artifacts`, `artifact_media_metadata`, `project_artifacts`).
- Streaming SHA-256 calculation and atomic staging-to-store file transitions.
- Global deduplication (same file in same project = single link; same file across projects = single stored binary).
- FFprobe JSON output parsing for duration, resolution, codecs, frame rate, sample rate, and audio channels.
- Tauri IPC commands for project CRUD and artifact import/listing.
- Frontend projects list, project details, debounced autosave, and asset list.
- Automated tests across Rust backend and React frontend.
- Verification script gatekeeping (`scripts/verify.ps1`).

### Strict Non-Goals (Scope Gates)
- **NO** AI provider integration (Gemini, OpenAI, Anthropic, Ollama, etc.).
- **NO** Speech-to-text (Whisper), Text-to-Speech (TTS), or generative AI agents.
- **NO** Timeline editor, canvas, drag-and-drop sequencing, video playback preview, or export renderer.
- **NO** Waveform generation or thumbnail image caching pipeline.
- **NO** Kaggle, Activepieces, cloud relays, web servers, or external databases (Postgres, Redis).
- **NO** Modifying or weakening Phase 0 code, tests, or health dashboard functionality.

---

## 3. Architecture & Key Technical Decisions

### 3.1 Global Content-Addressed Store vs. Per-Project Copies

```text
Projects
   ¦
   +-- Project Alpha ---+
   ¦                    ¦
   +-- Project Beta ----+--? ArtifactRef ("artifact://<id>")
                        ¦
                        ?
                 Artifact Registry
                        ¦
                     SHA-256
                        ¦
                        ?
           Local Content-Addressed Store
       (artifacts/<hash[0:2]>/<hash>)
```

- **Selected:** Global Content-Addressed Local Artifact Store. Identical files (identified by SHA-256 digest) reside in exactly one canonical file on disk, regardless of how many projects reference them.
- **Rejected Alternative A (Per-Project Copies):** Duplicating video files per project leads to rapid disk exhaustion on local desktop environments and breaks asset sharing across timelines.
- **Rejected Alternative B (Filesystem Path Referencing):** Referencing original paths breaks as soon as files are moved, renamed, stored on disconnected external drives, or deleted from user Downloads folders.

### 3.2 Canonical Storage Hierarchy

Artifact files reside in the Tauri application data directory resolved at runtime:

```text
%APPDATA%\com.openvideoai.studio\
+-- openvideoai.db              # SQLite database (WAL mode, foreign keys ON)
+-- staging\                    # In-progress streamed imports (temporary)
¦   +-- stage_<uuid>.tmp
+-- artifacts\                  # Content-addressed canonical storage
    +-- 3a\
    ¦   +-- 3a7b9c...
    +-- f1\
        +-- f14920...
```

- **Storage Key:** `artifacts/<sha256[0:2]>/<sha256>`
- **Partitioning:** First 2 hex characters of the SHA-256 digest form the parent directory to prevent filesystem directory node bottlenecks when thousands of assets are imported.
- **Atomic Promotion:** The import pipeline writes to `staging/stage_<uuid>.tmp` while computing SHA-256. Once finished, if the hash exists in the database, the staging file is discarded; if new, it is atomically moved (`fs::rename`) into `artifacts/<prefix>/<hash>`.

---

## 4. Core Contracts & Schemas

### 4.1 TypeScript Public Interface Contracts

```ts
export interface Project {
  id: string;
  name: string;
  description: string | null;
  createdAt: string;
  updatedAt: string;
}

export type ArtifactKind =
  | "video"
  | "audio"
  | "image"
  | "subtitle"
  | "json"
  | "other";

export interface ArtifactRef {
  artifactId: string;
  kind: ArtifactKind;
  uri: string; // "artifact://<artifactId>"
  sha256: string;
  sizeBytes: number;
  mimeType: string | null;
}

export interface MediaMetadata {
  artifactId: string;
  probeStatus: "ok" | "unavailable" | "failed";
  durationMs: number | null;
  width: number | null;
  height: number | null;
  videoCodec: string | null;
  audioCodec: string | null;
  frameRate: number | null;
  sampleRate: number | null;
  channels: number | null;
  probeError: string | null;
}

export interface ImportedArtifact {
  artifact: ArtifactRef;
  originalFilename: string;
  addedAt: string;
  metadata: MediaMetadata | null;
  isDeduplicated: boolean;
}
```

### 4.2 SQLite Database Schema (Migration Version 2)

```sql
-- Migration 2: project_and_artifact_core
CREATE TABLE IF NOT EXISTS projects (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS artifacts (
    id TEXT PRIMARY KEY,
    sha256 TEXT NOT NULL UNIQUE,
    kind TEXT NOT NULL,
    mime_type TEXT,
    size_bytes INTEGER NOT NULL,
    storage_key TEXT NOT NULL UNIQUE,
    original_filename TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS artifact_media_metadata (
    artifact_id TEXT PRIMARY KEY REFERENCES artifacts(id) ON DELETE CASCADE,
    probe_status TEXT NOT NULL,
    duration_ms INTEGER,
    width INTEGER,
    height INTEGER,
    video_codec TEXT,
    audio_codec TEXT,
    frame_rate REAL,
    sample_rate INTEGER,
    channels INTEGER,
    probe_error TEXT
);

CREATE TABLE IF NOT EXISTS project_artifacts (
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    artifact_id TEXT NOT NULL REFERENCES artifacts(id) ON DELETE CASCADE,
    added_at TEXT NOT NULL,
    PRIMARY KEY(project_id, artifact_id)
);

CREATE INDEX IF NOT EXISTS idx_artifacts_sha256 ON artifacts(sha256);
CREATE INDEX IF NOT EXISTS idx_project_artifacts_project ON project_artifacts(project_id);
```

---

## 5. Import Pipeline & Deduplication Logic

```text
[Select Source Path]
        ¦
        ?
[Validate Source File: exists, regular file, readable]
        ¦
        ?
[Create Staging File: staging/stage_<uuid>.tmp]
        ¦
        ?
[Stream Source Stream: copy bytes + update SHA-256 hasher]
        ¦
        ?
[Check SHA-256 against artifacts table]
        +-- Exists:
        ¦     - Discard staging file (remove)
        ¦     - Reuse existing artifact ID
        ¦     - isDeduplicated = true
        +-- Does Not Exist:
              - Ensure parent directory artifacts/<sha256[0:2]>/ exists
              - Atomic rename staging file to artifacts/<sha256[0:2]>/<sha256>
              - Infer ArtifactKind & MIME type from original extension
              - Insert row into artifacts table
              - Probe metadata with FFprobe (if available)
              - Insert row into artifact_media_metadata table
              - isDeduplicated = false
        ¦
        ?
[Link to Project]
  - INSERT OR IGNORE INTO project_artifacts (project_id, artifact_id, added_at)
  - Touch projects.updated_at = datetime('now')
        ¦
        ?
[Return ImportedArtifact with ArtifactRef and Metadata]
```

### 5.1 Fault Tolerance & Rollback
- If streaming fails or disk runs out of space, the staging file is immediately deleted and no database transaction is opened.
- If FFprobe encounters an error, hangs, or is missing, `probeStatus` is recorded as `"unavailable"` or `"failed"` along with `probeError`. The artifact import itself completes successfully.
- If database insertion fails, any newly created canonical file is cleaned up, and no orphan records remain.
- Staging directory is swept on startup (`clean_stale_staging`), removing leftover partial uploads from sudden shutdowns or crashes.

---

## 6. IPC Command Surface (Tauri Commands)

```rust
#[tauri::command]
fn create_project(name: String, description: Option<String>, state: State<AppState>) -> Result<Project, String>;

#[tauri::command]
fn list_projects(state: State<AppState>) -> Result<Vec<Project>, String>;

#[tauri::command]
fn get_project(project_id: String, state: State<AppState>) -> Result<Project, String>;

#[tauri::command]
fn update_project(project_id: String, name: String, description: Option<String>, state: State<AppState>) -> Result<Project, String>;

#[tauri::command]
fn import_artifact(project_id: String, source_path: String, state: State<AppState>) -> Result<ImportedArtifact, String>;

#[tauri::command]
fn list_project_artifacts(project_id: String, state: State<AppState>) -> Result<Vec<ImportedArtifact>, String>;
```

---

## 7. Frontend User Experience

- **Header / Navigation:** Seamlessly switch between "Projects" and Phase 0 "System Health".
- **Projects List:** Shows all projects, created date, updated date, and a "Create Project" modal/form.
- **Project Detail View:**
  - Editable project title and description with automatic debounced saving (400ms debounce).
  - Clean saved/saving indicators ("All changes saved" / "Saving...").
  - "Import Media" file picker (or drag-and-drop / file path prompt in dialog mode).
  - Asset List showing original filename, kind badge, file size in human-readable units (MB/GB), duration/resolution if available, metadata probe status, and a "Deduplicated" badge when content was reused.

---

## 8. Verification & Acceptance Criteria

1. All quality gates pass via `scripts/verify.ps1`:
   - `npm run typecheck`
   - `npm run test`
   - `npm run build`
   - `cargo fmt --manifest-path src-tauri/Cargo.toml --check`
   - `cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings`
   - `cargo test --manifest-path src-tauri/Cargo.toml`
2. All 15 required backend unit & integration tests pass with temporary databases.
3. All 7 required frontend tests pass with Vitest.
4. Runtime end-to-end verification confirms deduplication and restart persistence using real media.

# OpenVideoAI — Phase 0 Foundation Design Specification

- **Date:** 2026-09-07
- **Document Version:** 1.0.0
- **Status:** Approved by User
- **Phase:** Phase 0 (Foundation)
- **Target OS:** Windows (desktop-first)

---

## 1. Objective and Scope

### 1.1 Objective
Establish a solid, trustworthy Windows desktop foundation for **OpenVideoAI** (`openvideoai`). This foundation validates the integration of Tauri 2, React, TypeScript, Rust core, SQLite local state, structured logging, and executable discovery (FFmpeg/FFprobe) before any timeline or AI production modules are built.

### 1.2 Strict Scope Boundaries
**Included in Phase 0:**
1. Tauri 2 desktop application scaffolded with React, TypeScript (strict mode), and Vite.
2. Modular Rust core (`db`, `health`, `logging`, `tools`) integrated via Tauri commands.
3. SQLite initialization with WAL mode, schema version tracking, idempotent migrations, and real health queries.
4. Robust FFmpeg and FFprobe binary detection from PATH (with future custom path hook), version string parsing, and graceful unavailability handling.
5. Rust structured logging emitting timestamp, level, module target, and message without secrets or environment leaks.
6. Real health snapshot UI dashboard with status indicators, details, and an interactive refresh trigger.
7. Unit and integration test suites for both frontend and backend.
8. Automated verification script `scripts/verify.ps1` enforcing all zero-warning quality gates.

**Explicitly Excluded from Phase 0 (Non-Goals):**
- No AI providers (Gemini, OpenAI, local LLMs).
- No audio transcription (Whisper / Faster-Whisper).
- No voice synthesis (Edge-TTS / Piper).
- No remote compute (Kaggle worker, Cloud runner).
- No automation adapters (Activepieces, webhooks).
- No video rendering engine, timeline editor, or media import pipeline.
- No Docker, PostgreSQL, Redis, or cloud storage abstractions.
- No speculative empty directories or mock data for future phases.

---

## 2. Architecture & Data Contracts

### 2.1 Backend Component Hierarchy (`src-tauri/src`)
```
src-tauri/src/
├── main.rs                 # Tauri application entry point and command registration
├── lib.rs                  # App setup, state container, and library export
├── logging/
│   └── mod.rs              # Structured logging initialization (tracing / env_logger)
├── db/
│   ├── mod.rs              # Database connection manager, migration runner, health check
│   └── migrations.rs       # Idempotent SQL schema migrations
├── tools/
│   ├── mod.rs              # Tool detection orchestration
│   ├── detector.rs         # Execution, PATH discovery, and timeout handling
│   └── parser.rs           # Safe version extraction logic
└── health/
    ├── mod.rs              # Health aggregation service
    └── models.rs           # HealthSnapshot serde models and DTOs
```

### 2.2 Frontend Component Hierarchy (`src/`)
```
src/
├── main.tsx                # React DOM entry point
├── app/
│   ├── App.tsx             # Root layout and theme wrapper
│   └── App.css             # Base styles and dashboard styling
├── features/
│   └── health/
│       ├── types.ts        # TypeScript mirrors of backend HealthSnapshot DTOs
│       ├── api.ts          # Tauri IPC client for invoking get_health_snapshot
│       ├── HealthDashboard.tsx      # Dashboard container with state management
│       └── components/
│           ├── StatusCard.tsx       # Reusable status widget (App, DB, FFmpeg, FFprobe)
│           └── HealthDetails.tsx    # Detailed diagnostic breakdown
└── shared/
    └── components/
        └── Button.tsx      # Base button component
```

### 2.3 Health Snapshot Data Contract

The backend exposes a single authoritative command: `get_health_snapshot`.

```typescript
export interface ToolStatus {
  available: boolean;
  path: string | null;
  version: string | null;
}

export interface DatabaseStatus {
  ok: boolean;
  path: string;
}

export interface HealthSnapshot {
  appVersion: string;
  database: DatabaseStatus;
  ffmpeg: ToolStatus;
  ffprobe: ToolStatus;
}
```

Rust equivalent structs:
```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ToolStatus {
    pub available: boolean,
    pub path: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseStatus {
    pub ok: bool,
    pub path: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HealthSnapshot {
    pub app_version: String,
    pub database: DatabaseStatus,
    pub ffmpeg: ToolStatus,
    pub ffprobe: ToolStatus,
}
```

---

## 3. Subsystem Specifications

### 3.1 Database Subsystem (`db`)
- **Engine:** SQLite using `rusqlite` with the `bundled` feature flag to ensure zero external C-dependency issues on Windows.
- **Storage Path:** Persisted inside Tauri's resolved `app_data_dir()` in directory `OpenVideoAI/openvideoai.db`. Supports overriding path for unit and integration testing.
- **Journal Mode:** Write-Ahead Logging (`PRAGMA journal_mode = WAL;`) and synchronous normal (`PRAGMA synchronous = NORMAL;`) configured on initialization.
- **Migration Engine:**
  - Dedicated table `_migrations (id INTEGER PRIMARY KEY, version INTEGER UNIQUE NOT NULL, applied_at TEXT NOT NULL)`.
  - Migration scripts defined as immutable arrays of versioned SQL commands.
  - Migration runner runs in a single transaction per migration; idempotent on subsequent startups.
- **Health Verification:** Executes an active query: `SELECT COUNT(*) FROM _migrations;` to ensure read/write readiness, returning the real filesystem path.

### 3.2 Tool Detection Subsystem (`tools`)
- **Discovery Strategy:**
  1. If an explicit path is configured in future-compatible settings, test that path first.
  2. Otherwise, check standard PATH lookup for `ffmpeg` and `ffprobe` (including `.exe` suffix on Windows).
- **Execution:**
  - Execute binary with `-version`.
  - Set a safety execution timeout (e.g., 2000ms) to prevent hanging the backend.
  - Catch process spawning errors (e.g. `NotFound`, permission denied) and non-zero exit codes.
- **Version Parsing:**
  - Inspect stdout for standard patterns: `ffmpeg version ([^\s]+)` or `ffprobe version ([^\s]+)`.
  - Extract the exact version token (e.g., `9.0.1-full_build-www.gyan.dev` or `7.1`).
  - If output is unrecognizable or process fails, return `ToolStatus { available: false, path: None, version: None }` without panicking or bubbling errors that would crash the app.

### 3.3 Structured Logging Subsystem (`logging`)
- Configured via `tracing` and `tracing-subscriber` (or `env_logger`).
- Log format outputs:
  - ISO-8601 or formatted local timestamp
  - Log level (`INFO`, `WARN`, `ERROR`, `DEBUG`)
  - Target module (e.g., `openvideoai::db`, `openvideoai::tools`)
  - Log message
- Sanitization: Redacts or excludes potential secrets, tokens, or environment dumps.

### 3.4 User Interface Subsystem (`ui`)
- Responsive and clean desktop interface built with React and Tailwind CSS.
- **Dashboard Sections:**
  1. Header: Application title, version, and global health badge.
  2. Status Grid:
     - **Application**: Core version, Tauri runtime status.
     - **Database**: Connection status (Ready / Error), resolved database path.
     - **FFmpeg**: Detected badge, binary path, parsed version (or warning alert if absent).
     - **FFprobe**: Detected badge, binary path, parsed version (or warning alert if absent).
  3. Action Bar: "Refresh Status" button triggering a fresh IPC call. Loading and error states properly presented.
- **Resilience:** If FFmpeg or FFprobe is missing, the UI displays clear guidance and warning styling, but remains completely interactive and non-blocking.

---

## 4. Testing & Verification Strategy

### 4.1 Rust Tests
- `db_initialization_is_idempotent`: Running migrations twice on the same DB produces identical schema state without errors.
- `db_health_check_returns_valid_status`: Real DB health query verifies connection and path.
- `ffmpeg_version_parsing`: Tests parse standard FFmpeg output, Gyan.dev Windows builds, Linux package builds, and malformed strings.
- `ffprobe_version_parsing`: Tests parse standard FFprobe output and malformed strings.
- `missing_executable_handling`: Invoking non-existent executable returns `available: false` cleanly without panic.

### 4.2 Frontend Tests
- Renders healthy snapshot correctly with all green indicators, paths, and versions.
- Renders missing FFmpeg/FFprobe gracefully with informative warnings and no crashes.
- Clicking "Refresh" triggers API call and updates view state.
- Handles IPC failure cleanly by presenting an error banner.

### 4.3 Automated Verification Gate (`scripts/verify.ps1`)
Runs the full battery of tests and static analysis:
1. `npm run typecheck` (TypeScript strict check)
2. `npm run test` (Vitest unit tests)
3. `npm run build` (Vite production build)
4. `cargo fmt --check` (Rust formatting)
5. `cargo clippy -- -D warnings` (Rust linting with zero warnings allowed)
6. `cargo test` (Rust unit and integration tests)

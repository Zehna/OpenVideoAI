# Phase 0 Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a rock-solid, verifiable Tauri 2 + React + Rust + SQLite desktop foundation with real FFmpeg/FFprobe detection, structured logging, health dashboard, and zero-warning verification gates.

**Architecture:** The desktop core runs on Tauri 2 in Rust with modular subsystems (`db`, `tools`, `health`, `logging`). It exposes a single typed IPC command `get_health_snapshot` returning active database and binary statuses to a modern React dashboard.

**Tech Stack:** Tauri 2, Rust (1.98+), rusqlite (bundled), serde/serde_json, tracing/tracing-subscriber, React 19, TypeScript 5, Vite 6, Vitest, Tailwind CSS.

**Spec:** `docs/superpowers/specs/2026-09-07-openvideoai-foundation-design.md`

## Global Constraints
- **Zero Mocking:** All health queries, database connections, and tool detections must reflect live physical execution.
- **Strict Scope:** Phase 0 foundation only. No AI providers, no Whisper, no TTS, no remote compute, no timeline editor, no media pipeline.
- **Quality Gates:** 100% pass on TypeScript typecheck, Vitest, Cargo test, `cargo fmt --check`, and `cargo clippy -- -D warnings`.
- **Platform:** Windows desktop target, native execution, safe executable and path handling.

---

### Task 1: Project Scaffolding & Configuration

**Files:**
- Create: `package.json`
- Create: `tsconfig.json`
- Create: `tsconfig.node.json`
- Create: `vite.config.ts`
- Create: `vitest.config.ts`
- Create: `index.html`
- Create: `src-tauri/Cargo.toml`
- Create: `src-tauri/tauri.conf.json`
- Create: `src-tauri/build.rs`
- Create: `src-tauri/src/main.rs`
- Create: `src-tauri/src/lib.rs`
- Create: `src/vite-env.d.ts`
- Create: `src/main.tsx`
- Create: `src/app/App.tsx`
- Create: `src/app/App.css`

**Interfaces:**
- Consumes: None
- Produces: Runnable baseline project where `npm run build` and `cargo check` succeed.

- [ ] **Step 1: Create package.json and project configs**
Set up npm dependencies including `@tauri-apps/api`, `@tauri-apps/cli`, `react`, `react-dom`, `lucide-react`, `vitest`, `@testing-library/react`, `@testing-library/jest-dom`, `jsdom`, `typescript`, `vite`.

- [ ] **Step 2: Create src-tauri configuration and Cargo.toml**
Configure Tauri 2 with `identifier: "com.openvideoai.studio"`, `productName: "OpenVideoAI"`.
Configure Cargo dependencies: `tauri` (v2), `serde`, `serde_json`, `rusqlite` (features = ["bundled"]), `tracing`, `tracing-subscriber`, `thiserror`, `tempfile` (dev-dependency).

- [ ] **Step 3: Create minimal entry points**
Create `index.html`, `src/main.tsx`, `src/app/App.tsx`, `src-tauri/build.rs`, `src-tauri/src/main.rs`, `src-tauri/src/lib.rs`.

- [ ] **Step 4: Verify baseline builds**
Run: `npm install; npm run build; cargo check --manifest-path src-tauri/Cargo.toml`
Expected: PASS with exit code 0.

- [ ] **Step 5: Commit**
```bash
git add package.json tsconfig*.json vite.config.ts vitest.config.ts index.html src/ src-tauri/ package-lock.json
git commit -m "chore: scaffold Tauri 2, React, and Rust project workspace"
```

---

### Task 2: Rust Structured Logging Subsystem

**Files:**
- Create: `src-tauri/src/logging/mod.rs`
- Test: `src-tauri/tests/logging_tests.rs`

**Interfaces:**
- Consumes: None
- Produces: `pub fn init_logging() -> Result<(), Box<dyn std::error::Error>>`

- [ ] **Step 1: Write the failing integration test**
In `src-tauri/tests/logging_tests.rs`, test that `init_logging()` initializes without panicking and can be called safely.

- [ ] **Step 2: Run test to verify it fails**
Run: `cargo test --test logging_tests`
Expected: FAIL with compilation error (module `logging` missing).

- [ ] **Step 3: Implement logging subsystem**
Implement `src-tauri/src/logging/mod.rs` using `tracing-subscriber` with standard formatting (timestamp, level, target, message) and filter from `RUST_LOG` defaulting to `info`.

- [ ] **Step 4: Run test to verify it passes**
Run: `cargo test --test logging_tests`
Expected: PASS.

- [ ] **Step 5: Commit**
```bash
git add src-tauri/src/logging/mod.rs src-tauri/tests/logging_tests.rs
git commit -m "feat(backend): implement structured logging subsystem"
```

---

### Task 3: Database Subsystem with WAL, Idempotent Migrations & Health Query

**Files:**
- Create: `src-tauri/src/db/migrations.rs`
- Create: `src-tauri/src/db/mod.rs`
- Test: `src-tauri/tests/db_tests.rs`

**Interfaces:**
- Consumes: SQLite filesystem path
- Produces:
  - `pub struct DbManager { path: PathBuf, ... }`
  - `DbManager::new(path: PathBuf) -> Result<Self, DbError>`
  - `DbManager::run_migrations(&self) -> Result<(), DbError>`
  - `DbManager::check_health(&self) -> Result<DatabaseStatus, DbError>`

- [ ] **Step 1: Write failing database unit & integration tests**
In `src-tauri/tests/db_tests.rs`:
- Test 1: `test_db_initialization_and_wal_mode` (creates DB, verifies WAL pragma).
- Test 2: `test_db_migrations_are_idempotent` (running migrations multiple times succeeds and preserves schema).
- Test 3: `test_db_health_query` (executes `SELECT COUNT(*) FROM _migrations`, returns valid `DatabaseStatus`).

- [ ] **Step 2: Run tests to verify they fail**
Run: `cargo test --test db_tests`
Expected: FAIL with unresolved imports.

- [ ] **Step 3: Implement migrations and DbManager**
- `src-tauri/src/db/migrations.rs`: Migration definition array and table `_migrations (id INTEGER PRIMARY KEY, version INTEGER UNIQUE NOT NULL, applied_at TEXT NOT NULL)`.
- `src-tauri/src/db/mod.rs`: `DbManager` connection pool / mutex wrapper, WAL configuration (`PRAGMA journal_mode = WAL;`, `PRAGMA synchronous = NORMAL;`), `check_health()` executing live query.

- [ ] **Step 4: Run tests to verify they pass**
Run: `cargo test --test db_tests`
Expected: PASS (all 3 tests green).

- [ ] **Step 5: Commit**
```bash
git add src-tauri/src/db/ src-tauri/tests/db_tests.rs
git commit -m "feat(backend): implement SQLite manager with WAL and idempotent migrations"
```

---

### Task 4: Tool Detection Subsystem for FFmpeg & FFprobe

**Files:**
- Create: `src-tauri/src/tools/parser.rs`
- Create: `src-tauri/src/tools/detector.rs`
- Create: `src-tauri/src/tools/mod.rs`
- Test: `src-tauri/tests/tools_tests.rs`

**Interfaces:**
- Consumes: Tool name or optional custom path
- Produces:
  - `pub fn parse_tool_version(tool: &str, output: &str) -> Option<String>`
  - `pub fn detect_tool(tool_name: &str, custom_path: Option<&Path>) -> ToolStatus`
  - `pub struct ToolDetector`

- [ ] **Step 1: Write failing tool detection and parsing tests**
In `src-tauri/tests/tools_tests.rs`:
- Test 1: `test_parse_ffmpeg_version_gyan_build` (real Windows gyan.dev version string).
- Test 2: `test_parse_ffmpeg_version_linux_standard` (`ffmpeg version 6.1.1 Copyright...`).
- Test 3: `test_parse_ffprobe_version` (real ffprobe version string).
- Test 4: `test_parse_invalid_version_output` (returns `None`).
- Test 5: `test_detect_missing_executable` (querying `"nonexistent_binary_xyz"` returns `ToolStatus { available: false, path: None, version: None }` without error).
- Test 6: `test_detect_ffmpeg_when_available` (locates real `ffmpeg` on current machine PATH).

- [ ] **Step 2: Run tests to verify they fail**
Run: `cargo test --test tools_tests`
Expected: FAIL with unresolved imports.

- [ ] **Step 3: Implement parser and detector**
- `src-tauri/src/tools/parser.rs`: Safe extraction of version token using regex or string splitting.
- `src-tauri/src/tools/detector.rs`: Discovery via `where.exe` / PATH lookup or custom path; runs `<exe> -version` with timeout; catches exit status.
- `src-tauri/src/tools/mod.rs`: Exposes `detect_tool`.

- [ ] **Step 4: Run tests to verify they pass**
Run: `cargo test --test tools_tests`
Expected: PASS (all 6 tests green).

- [ ] **Step 5: Commit**
```bash
git add src-tauri/src/tools/ src-tauri/tests/tools_tests.rs
git commit -m "feat(backend): implement FFmpeg and FFprobe discovery and version parsing"
```

---

### Task 5: Backend Health Aggregation Service & Tauri IPC Command

**Files:**
- Create: `src-tauri/src/health/models.rs`
- Create: `src-tauri/src/health/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/main.rs`
- Test: `src-tauri/tests/health_tests.rs`

**Interfaces:**
- Consumes: `DbManager`, `ToolDetector`
- Produces:
  - `#[tauri::command] pub fn get_health_snapshot(...) -> Result<HealthSnapshot, String>`
  - Tauri application state management

- [ ] **Step 1: Write failing health aggregation test**
In `src-tauri/tests/health_tests.rs`:
- Test `test_health_snapshot_aggregation` verifying that `HealthSnapshot` aggregates appVersion, database status, and tools correctly.

- [ ] **Step 2: Run test to verify it fails**
Run: `cargo test --test health_tests`
Expected: FAIL with compilation error.

- [ ] **Step 3: Implement health models, service, and Tauri command**
- `src-tauri/src/health/models.rs`: `HealthSnapshot`, `DatabaseStatus`, `ToolStatus` with serde camelCase serialization.
- `src-tauri/src/health/mod.rs`: `HealthService::get_snapshot(&self) -> HealthSnapshot`.
- In `src-tauri/src/lib.rs`: register state and `get_health_snapshot` Tauri command. Ensure DB path uses `app.path().app_data_dir()`.

- [ ] **Step 4: Run test to verify it passes**
Run: `cargo test`
Expected: All backend tests PASS.

- [ ] **Step 5: Commit**
```bash
git add src-tauri/src/health/ src-tauri/src/lib.rs src-tauri/src/main.rs src-tauri/tests/health_tests.rs
git commit -m "feat(backend): aggregate health snapshot and register Tauri IPC command"
```

---

### Task 6: Frontend Health Dashboard UI & Tests

**Files:**
- Create: `src/features/health/types.ts`
- Create: `src/features/health/api.ts`
- Create: `src/shared/components/Button.tsx`
- Create: `src/features/health/components/StatusCard.tsx`
- Create: `src/features/health/components/HealthDetails.tsx`
- Create: `src/features/health/HealthDashboard.tsx`
- Modify: `src/app/App.tsx`
- Test: `src/features/health/__tests__/HealthDashboard.test.tsx`

**Interfaces:**
- Consumes: Tauri IPC command `get_health_snapshot`
- Produces: Interactive Health Dashboard UI and passing frontend tests.

- [ ] **Step 1: Write failing frontend tests**
In `src/features/health/__tests__/HealthDashboard.test.tsx`:
- Test 1: Renders healthy snapshot with Database Ready, FFmpeg detected, FFprobe detected.
- Test 2: Renders missing FFmpeg gracefully with clear warning and no crash.
- Test 3: Clicking "Refresh Status" triggers `get_health_snapshot` again and updates UI.
- Test 4: Handles IPC error gracefully by displaying an alert banner.

- [ ] **Step 2: Run tests to verify they fail**
Run: `npm run test`
Expected: FAIL (components not found).

- [ ] **Step 3: Implement types, API client, and components**
- `types.ts`: Mirror backend contracts (`HealthSnapshot`, `DatabaseStatus`, `ToolStatus`).
- `api.ts`: Calls `@tauri-apps/api/core::invoke("get_health_snapshot")`.
- `Button.tsx`, `StatusCard.tsx`, `HealthDetails.tsx`, `HealthDashboard.tsx`.
- Connect into `App.tsx`.

- [ ] **Step 4: Run tests to verify they pass**
Run: `npm run test`
Expected: PASS (all tests green).

- [ ] **Step 5: Verify frontend build and typecheck**
Run: `npm run typecheck; npm run build`
Expected: PASS with 0 errors.

- [ ] **Step 6: Commit**
```bash
git add src/
git commit -m "feat(frontend): implement Health Dashboard UI and component tests"
```

---

### Task 7: Verification Script & End-to-End Quality Gates

**Files:**
- Create: `scripts/verify.ps1`

**Interfaces:**
- Consumes: All project verification commands
- Produces: Exit code 0 if and only if all gates pass.

- [ ] **Step 1: Create scripts/verify.ps1**
Script that halts on error and executes in sequence:
1. `npm run typecheck`
2. `npm run test`
3. `npm run build`
4. `cargo fmt --manifest-path src-tauri/Cargo.toml --check`
5. `cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings`
6. `cargo test --manifest-path src-tauri/Cargo.toml`

- [ ] **Step 2: Execute verification script**
Run: `powershell -ExecutionPolicy Bypass -File scripts/verify.ps1`
Expected: PASS with all gates green.

- [ ] **Step 3: Test desktop application launch**
Launch Tauri application or execute `cargo test` / `tauri build --debug` to verify desktop shell stability.

- [ ] **Step 4: Commit**
```bash
git add scripts/verify.ps1
git commit -m "chore: add automated verification script"
```

# OpenVideoAI — Phase 2 Durable Jobs & Events Implementation Plan

- **Date:** 2026-09-10
- **Phase:** 2 (Durable Jobs & Events)
- **Target Branch:** `phase/2-durable-jobs-events`
- **Base Commit:** `v0.2.0` (`d179f16`)

---

## 1. Plan Overview

Phase 2 introduces a durable, SQLite-backed asynchronous job engine and typed domain event bus to OpenVideoAI. This establishes the orchestration backbone required by subsequent phases (media proxies, Whisper transcription, speech synthesis, and video rendering).

---

## 2. Step-by-Step Execution Order

### Task 1: SQLite Migration 3 (Jobs, Checkpoints, Domain Events)
- **Target Files:**
  - `src-tauri/src/db/mod.rs`
  - `src-tauri/tests/db_tests.rs`
- **Changes:**
  - Add Migration 3 defining tables `jobs`, `job_checkpoints`, and `domain_events`.
  - Add indices for `(status, priority DESC, created_at ASC)`, `idempotency_key`, `project_id`, and `event_type`.
  - Add test verifying Migration 3 executes idempotently and enforces foreign keys.

### Task 2: Core Job Data Models & Repository
- **Target Files:**
  - `src-tauri/src/jobs/mod.rs`
  - `src-tauri/src/jobs/model.rs`
  - `src-tauri/src/jobs/repository.rs`
  - `src-tauri/tests/job_tests.rs`
- **Changes:**
  - Implement `JobState`, `Job`, `JobCheckpoint`, and `JobSubmission`.
  - Implement `JobRepository` with methods:
    - `enqueue`: Idempotent job insertion with priority.
    - `acquire_lease`: Atomic dequeue selecting highest priority `queued` or expired `running` job.
    - `heartbeat`: Extend `lease_expires_at` for active worker.
    - `record_checkpoint`: Persist intermediate pipeline stage state.
    - `complete_job`: Transition to `completed` with output JSON.
    - `fail_job`: Transition to `retryable` (if retries remaining) or `dead_letter`.
    - `cancel_job`: Transition to `cancelled`.
  - Add unit tests verifying priority dequeue, idempotency, and state transitions.

### Task 3: Worker Lease Manager & Recovery Engine
- **Target Files:**
  - `src-tauri/src/jobs/manager.rs`
  - `src-tauri/tests/job_recovery_tests.rs`
- **Changes:**
  - Implement `JobManager` with background worker loop and lease reaper.
  - Periodic / startup recovery: reclaims abandoned or expired leases from crashed workers.
  - Exponential backoff calculation for retryable jobs.
  - Cancellation token support for running jobs.

### Task 4: Typed Domain Event Bus & Persistence
- **Target Files:**
  - `src-tauri/src/events/mod.rs`
  - `src-tauri/src/events/model.rs`
  - `src-tauri/src/events/bus.rs`
  - `src-tauri/tests/event_tests.rs`
- **Changes:**
  - Implement `DomainEvent<T>` envelope.
  - Implement `EventBus` backed by `tokio::sync::broadcast` and SQLite event persistence.
  - Connect `JobManager` to emit `JobQueued`, `JobStarted`, `JobCheckpointReached`, `JobCompleted`, and `JobFailed`.

### Task 5: Backend IPC Commands & Tauri Bridge
- **Target Files:**
  - `src-tauri/src/jobs/commands.rs`
  - `src-tauri/src/events/commands.rs`
  - `src-tauri/src/lib.rs`
- **Changes:**
  - Expose Tauri commands:
    - `submit_job`, `get_job`, `cancel_job`, `list_jobs`, `get_job_checkpoints`, `list_events`.
  - Forward broadcast domain events to the webview window via Tauri event `domain-event`.

### Task 6: Frontend Jobs & Queue Inspector UI
- **Target Files:**
  - `src/features/jobs/types.ts`
  - `src/features/jobs/api.ts`
  - `src/features/jobs/components/JobList.tsx`
  - `src/features/jobs/components/JobDetailModal.tsx`
  - `src/app/App.tsx`
  - `src/features/jobs/__tests__/JobList.test.tsx`
- **Changes:**
  - Add "Jobs" navigation tab in `App.tsx`.
  - Display job queue with status badges, priority, retry counts, elapsed time, and error details.
  - Provide manual retry / cancellation buttons and checkpoint inspector.

### Task 7: Quality Gates & End-to-End Verification
- **Target Files:**
  - `scripts/verify.ps1`
  - `src-tauri/tests/acceptance_jobs_tests.rs`
- **Changes:**
  - Run full suite: TypeScript check, Vitest tests, build, `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
  - Ensure zero warnings, zero errors, and clean working tree.

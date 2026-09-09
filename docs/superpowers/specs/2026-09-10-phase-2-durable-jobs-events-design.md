# OpenVideoAI — Phase 2 Durable Jobs & Events Design Specification

- **Date:** 2026-09-10
- **Document Version:** 1.0.0
- **Status:** Proposed / Under Review
- **Phase:** Phase 2 (Durable Jobs & Events)
- **Target Version:** v0.3
- **Branch:** `phase/2-durable-jobs-events`
- **Base:** `v0.2.0` (`d179f16`)

---

## 1. Objective and Scope

### 1.1 Objective
Establish an asynchronous, crash-resilient, transactional job orchestration and event notification core in OpenVideoAI. All future computationally intensive or multi-stage operations (media proxy generation, Whisper transcription, speech synthesis, AI script planning, FFmpeg video rendering) will execute as durable background jobs with checkpointing and state tracking rather than blocking foreground UI calls.

### 1.2 Strict Scope Boundaries
**Included in Phase 2:**
1. **SQLite Durable Job Queue (Migration 3):**
   - Transactional state transitions: `queued`, `running`, `waiting_for_capacity`, `retryable`, `completed`, `failed`, `cancelled`, `dead_letter`.
   - Priority-based scheduling (higher integer runs first; FIFO for equal priority).
   - Idempotency key support to prevent duplicate task scheduling across retries or double clicks.
   - Configurable retry policy with exponential backoff and transition to `dead_letter` upon exhaustion.
   - Pipeline checkpoints (`stage`, `checkpoint_data` JSON) allowing complex pipelines to resume from the last successful stage.
2. **Worker Lease & Heartbeat Abstraction:**
   - Worker concurrency limits (configurable local worker pool).
   - Leased execution (`leased_by`, `lease_expires_at`).
   - Periodic heartbeat renewal while work is in progress.
   - Automatic lease expiration recovery: if a worker process crashes or is terminated, stale running jobs are reclaimed and re-queued upon next queue sweep/startup.
3. **Typed Domain Event Bus:**
   - Standard domain event envelope: `DomainEvent<T>` (`eventId`, `type`, `version`, `timestamp`, `projectId`, `jobId`, `payload`).
   - In-memory event distribution to local subscribers and real-time push to frontend via Tauri event bridge (`domain-event`).
   - SQLite domain event logging for durable auditability.
4. **IPC Commands & Minimal Frontend Queue Inspector:**
   - Tauri IPC commands: `submit_job`, `get_job`, `cancel_job`, `list_jobs`, `get_job_checkpoints`, `list_events`.
   - Frontend "Jobs" management tab: real-time status list, priority badge, retry counters, error inspector, and manual cancellation.
5. **Quality Gates & Automated Verification:**
   - Unit and integration tests covering priority ordering, worker crash recovery, idempotency deduplication, retry exponential backoff, DLQ transition, checkpoint persistence, and event emissions.
   - Full verification via `scripts/verify.ps1`.

**Explicitly Excluded from Phase 2 (Non-Goals):**
- No real AI models or remote providers (Gemini, OpenAI).
- No actual Whisper transcription or Edge-TTS speech generation (dummy/test task runners only).
- No timeline editor, canvas, or video compositor (Phase 3: Media & Timeline Core).
- No Kaggle worker adapter or cloud cluster runners (Phase 1.0).
- No Activepieces webhooks or external workflow engine.
- No Docker, Redis, or PostgreSQL.

---

## 2. Architecture & Data Contracts

### 2.1 State Machine
```text
               ┌──────────────┐
               │    queued    │◄─────────────────┐
               └──────┬───────┘                  │
                      │ dequeue / acquire lease  │
                      ▼                          │
               ┌──────────────┐                  │
         ┌────►│   running    │                  │
         │     └──────┬───────┘                  │
heartbeat│            │                          │
renewal  │      ┌─────┴──────────────────┐       │
         │      │                        │       │
         └──────┤                        ▼       │
                │                ┌──────────────┐│
                │                │  retryable   │┘ (exponential backoff)
                │                └──────┬───────┘
                │                       │ (exceeded max_retries)
                │                       ▼
                │                ┌──────────────┐
                │                │ dead_letter  │
                │                └──────────────┘
                │
                ├────────────────────────┬──────────────────────┐
                ▼                        ▼                      ▼
         ┌──────────────┐         ┌──────────────┐       ┌──────────────┐
         │  completed   │         │    failed    │       │  cancelled   │
         └──────────────┘         └──────────────┘       └──────────────┘
```

### 2.2 TypeScript Data Contracts

```ts
export type JobState =
  | "queued"
  | "running"
  | "waiting_for_capacity"
  | "retryable"
  | "completed"
  | "failed"
  | "cancelled"
  | "dead_letter";

export interface Job {
  id: string;
  jobType: string;
  status: JobState;
  priority: number;
  idempotencyKey: string | null;
  projectId: string | null;
  payload: string; // JSON string
  result: string | null; // JSON string
  error: string | null;
  retryCount: number;
  maxRetries: number;
  leasedBy: string | null;
  leaseExpiresAt: string | null;
  createdAt: string;
  updatedAt: string;
  startedAt: string | null;
  completedAt: string | null;
}

export interface JobCheckpoint {
  id: string;
  jobId: string;
  stage: string;
  checkpointData: string; // JSON string
  createdAt: string;
}

export interface DomainEvent<T = unknown> {
  eventId: string;
  type: string;
  version: 1;
  timestamp: string;
  projectId?: string | null;
  jobId?: string | null;
  payload: T;
}
```

---

## 3. Database Schema (Migration 3)

```sql
-- 1. Jobs Table
CREATE TABLE IF NOT EXISTS jobs (
    id TEXT PRIMARY KEY,
    job_type TEXT NOT NULL,
    status TEXT NOT NULL,
    priority INTEGER NOT NULL DEFAULT 0,
    idempotency_key TEXT UNIQUE,
    project_id TEXT REFERENCES projects(id) ON DELETE SET NULL,
    payload TEXT NOT NULL,
    result TEXT,
    error TEXT,
    retry_count INTEGER NOT NULL DEFAULT 0,
    max_retries INTEGER NOT NULL DEFAULT 3,
    leased_by TEXT,
    lease_expires_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    started_at TEXT,
    completed_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_jobs_status_priority ON jobs(status, priority DESC, created_at ASC);
CREATE INDEX IF NOT EXISTS idx_jobs_project_id ON jobs(project_id);
CREATE INDEX IF NOT EXISTS idx_jobs_idempotency_key ON jobs(idempotency_key);

-- 2. Job Checkpoints Table
CREATE TABLE IF NOT EXISTS job_checkpoints (
    id TEXT PRIMARY KEY,
    job_id TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    stage TEXT NOT NULL,
    checkpoint_data TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_job_checkpoints_job_id ON job_checkpoints(job_id, created_at ASC);

-- 3. Domain Events Log Table
CREATE TABLE IF NOT EXISTS domain_events (
    id TEXT PRIMARY KEY,
    event_type TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1,
    project_id TEXT,
    job_id TEXT,
    payload TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_domain_events_type_created ON domain_events(event_type, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_domain_events_job_id ON domain_events(job_id);
```

---

## 4. Backend Subsystem Design

### 4.1 Modules (`src-tauri/src/`)
- `jobs/`
  - `model.rs`: Structs and enums (`Job`, `JobState`, `JobCheckpoint`, `JobSubmission`).
  - `repository.rs`: Transactional queries for enqueue, acquire lease, heartbeat, complete, fail, record checkpoint.
  - `manager.rs`: `JobManager` coordinating worker threads, periodic lease reaper, and task dispatching.
  - `commands.rs`: Tauri IPC wrappers.
- `events/`
  - `model.rs`: `DomainEvent<T>`, standard event envelope and payloads.
  - `bus.rs`: `EventBus` backed by `tokio::sync::broadcast` with asynchronous SQLite log writer.
  - `commands.rs`: IPC queries for recent domain events.

### 4.2 Crash Recovery & Lease Reaping
On startup and periodically during runtime (e.g., every 5 seconds):
- Find jobs where `status = 'running'` and `lease_expires_at < NOW()`.
- If `retry_count < max_retries`:
  - Increment `retry_count`.
  - Set `status = 'retryable'` or `'queued'`.
  - Record a warning domain event (`JobLeaseExpired`).
- Else:
  - Set `status = 'dead_letter'`.
  - Record an error domain event (`JobMovedToDeadLetter`).

---

## 5. Verification Plan

1. **Rust Tests (`tests/job_tests.rs`, `tests/event_tests.rs`):**
   - Priority queue ordering (higher priority dequeued first).
   - Idempotent submission returns existing job without duplicate insertion.
   - Worker lease acquisition and heartbeat extension.
   - Expired lease reaping re-queues stale jobs.
   - Retry logic transitions to retryable, then dead_letter upon exhausting `max_retries`.
   - Checkpoint creation and retrieval.
   - Event bus broadcast and SQLite event persistence.
2. **Frontend Component Tests (`src/features/jobs/__tests__/`):**
   - `JobList.test.tsx`: Renders active and completed jobs with badges.
   - `JobDetails.test.tsx`: Displays checkpoints, payloads, and error messages.
3. **Full Pipeline (`scripts/verify.ps1`):**
   - All 6 quality gates pass without warnings.

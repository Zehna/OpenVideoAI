# Phase 4 — AI Content Planning Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver OpenVideoAI v0.5 AI Content Planning: Topic/Script → durable, versioned, editable, source-aware `ContentPlan` revisions through the existing Job Manager, with Gemini as the first real provider, a deterministic mock provider, grounding/citation integrity, granular regeneration, approval/archive workflows, and a usable AI Planner UI.

**Architecture:** Add a focused Rust `content_planning` subsystem layered on the existing SQLite, Job Manager, Event Bus, and Tauri command patterns. `content.plan` jobs carry immutable planning snapshots; `LocalContentPlanWorker` performs two-stage generation, bounded repair, lease heartbeats, cancellation/fencing checks, validation, and transactional revision publication. The React frontend uses typed Tauri wrappers and an explicit-save structured editor; it never receives secrets, physical paths, or raw provider responses.

**Tech Stack:** Rust 2021, rusqlite 0.32, serde/serde_json, thiserror, uuid, chrono, tokio, reqwest 0.12 with `json` + `rustls-tls`, Tauri 2; TypeScript 5.7, React 19, Tailwind CSS 4, Vitest 3, React Testing Library.

**Spec:** `docs/superpowers/specs/2026-09-12-phase-4-ai-content-planning-design.md`

## Global Constraints

- **Base:** `v0.4.0` / commit `0f08683`; approved design head before implementation planning is `d54e367`.
- **Target branch:** `phase/4-ai-content-planning`.
- **Target version:** `v0.5.0`; do not tag until release-close audit authorizes it.
- **Migrations 1–5 are immutable.** Phase 4 schema is Migration 6 only.
- **Single durable job type:** every planning operation uses `jobs.job_type = "content.plan"`; operation lives in the immutable payload.
- **Immutable revisions:** durable content mutations create strictly monotonic revisions; historical revision JSON and revision-scoped source snapshots are never overwritten.
- **Optimistic concurrency:** manual save/approval/archive use `expected_revision`; regeneration uses `base_revision` and discards stale output.
- **Canonical aggregates:** `estimatedDurationMs = sum(scene.durationEstimateMs)` and `fullNarration = ordered scene narrations joined by "\n\n"`; backend recomputes both before validation/persistence.
- **Grounding:** `supported`, `model_knowledge`, `uncertain`, `modified`; fake/dangling source IDs are rejected; manual narration edits deterministically invalidate all claims on that scene.
- **Provider boundary:** provider implementations never write SQLite, create revisions, submit jobs, mutate timelines, resolve filesystem paths, or expose secrets.
- **Secrets:** `GEMINI_API_KEY` is runtime-only and must not enter jobs, revisions, source snapshots, events, logs, or persisted frontend state.
- **Worker safety:** cancelled/stale workers must never publish a revision; all checkpoints/publication/completion are lease-token fenced.
- **Repair bound:** at most 3 model calls per generation stage (initial + 2 repairs).
- **No Phase 5+ creep:** no TTS, Whisper, captions, stock execution, autonomous web research, AI image/video generation, timeline insertion, ComfyUI, Kaggle, remote workers, server mode, or collaboration.
- **Quality gate:** preserve `scripts/verify.ps1` behavior including Rust build-job and debuginfo safeguards.

---

## Planned File Structure

### Rust subsystem

- Create `src-tauri/src/content_planning/mod.rs` — module exports.
- Create `src-tauri/src/content_planning/model.rs` — domain/request/job/summary/provider-status types.
- Create `src-tauri/src/content_planning/error.rs` — sanitized internal/public error categories.
- Create `src-tauri/src/content_planning/derivation.rs` — canonical aggregate recomputation and normalization.
- Create `src-tauri/src/content_planning/grounding.rs` — source/claim validation and deterministic manual invalidation.
- Create `src-tauri/src/content_planning/validation.rs` — planning request and ContentPlan domain validation.
- Create `src-tauri/src/content_planning/repository.rs` — Migration-6 persistence, immutable revision/source reads/writes, summaries/history.
- Create `src-tauri/src/content_planning/service.rs` — manual save, approval, archive, revision conflict rules, transactional events.
- Create `src-tauri/src/content_planning/prompt.rs` — versioned provider-neutral prompt construction.
- Create `src-tauri/src/content_planning/planner.rs` — two-stage orchestration + bounded repair + regeneration isolation.
- Create `src-tauri/src/content_planning/provider/mod.rs` — `AIProvider`, request/response/error contracts.
- Create `src-tauri/src/content_planning/provider/config.rs` — `ProviderConfigResolver`, `GEMINI_API_KEY`, safe status.
- Create `src-tauri/src/content_planning/provider/mock.rs` — deterministic offline provider.
- Create `src-tauri/src/content_planning/provider/gemini.rs` — HTTP transport, structured JSON extraction, usage/error mapping.
- Create `src-tauri/src/content_planning/jobs.rs` — immutable payloads, operation validation, idempotency keys, submission helpers.
- Create `src-tauri/src/content_planning/worker.rs` — `LocalContentPlanWorker` lifecycle/checkpoints/fencing.
- Create `src-tauri/src/content_planning/commands.rs` — typed Tauri IPC surface.
- Modify `src-tauri/src/db/migrations.rs`, `src-tauri/src/lib.rs`, `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`.

### Rust tests

- Modify `src-tauri/tests/db_tests.rs`.
- Create `src-tauri/tests/content_plan_model_tests.rs`.
- Create `src-tauri/tests/content_plan_grounding_tests.rs`.
- Create `src-tauri/tests/content_plan_repository_tests.rs`.
- Create `src-tauri/tests/content_plan_service_tests.rs`.
- Create `src-tauri/tests/content_plan_provider_tests.rs`.
- Create `src-tauri/tests/content_plan_jobs_tests.rs`.
- Create `src-tauri/tests/content_plan_worker_tests.rs`.
- Create `src-tauri/tests/content_plan_ipc_tests.rs`.
- Create `src-tauri/tests/content_plan_live_gemini_tests.rs`.

### Frontend

- Create `src/features/planner/types.ts` — UI-domain mirrors.
- Create `src/features/planner/api.ts` — typed `invoke` wrappers.
- Create `src/features/planner/components/PlannerView.tsx` — list/create/editor view state.
- Create `src/features/planner/components/PlanList.tsx` — project plan list.
- Create `src/features/planner/components/NewPlanForm.tsx` — Topic/Script + sources + provider status.
- Create `src/features/planner/components/ContentPlanEditor.tsx` — explicit-save plan editor shell.
- Create `src/features/planner/components/SceneEditor.tsx` — editable scene fields and claims.
- Create `src/features/planner/components/GroundingBadge.tsx` — supported/model-knowledge/uncertain/modified display.
- Create `src/features/planner/components/RevisionHistory.tsx` — read-only history/revision loading.
- Create `src/features/planner/__tests__/PlannerView.test.tsx`.
- Create `src/features/planner/__tests__/ContentPlanEditor.test.tsx`.
- Modify `src/features/projects/components/ProjectDetail.tsx`, `src/features/projects/__tests__/ProjectDetail.test.tsx`, `src/app/App.tsx`.

---

## Task 1: Migration 6 — Content Plan, Revision, and Revision-Scoped Source Tables

**Files:**
- Modify: `src-tauri/src/db/migrations.rs`
- Modify/Test: `src-tauri/tests/db_tests.rs`

**Interfaces:**
- Consumes: existing `MIGRATIONS` runner and released Migration-5 schema.
- Produces: `content_plans`, `content_plan_revisions`, `content_plan_sources` plus indexes; Migration 5→6 upgrade proof.

- [ ] **Step 1: Add failing fresh-schema and upgrade tests**

Add tests that assert these exact schema properties:

```rust
assert_table_exists(&conn, "content_plans");
assert_table_exists(&conn, "content_plan_revisions");
assert_table_exists(&conn, "content_plan_sources");

conn.execute(
    "INSERT INTO projects (id,name,created_at,updated_at) VALUES ('p1','P',datetime('now'),datetime('now'))",
    [],
).unwrap();
conn.execute(
    "INSERT INTO content_plans (id,project_id,title,created_at,updated_at) VALUES ('cp1','p1','Plan',datetime('now'),datetime('now'))",
    [],
).unwrap();
conn.execute(
    "INSERT INTO content_plan_revisions (plan_id,revision,parent_revision,operation_type,content_json,generation_metadata_json,created_by,created_at)
     VALUES ('cp1',1,NULL,'generate','{}',NULL,'ai',datetime('now'))",
    [],
).unwrap();
conn.execute(
    "INSERT INTO content_plan_sources (plan_id,revision,source_id,source_type,title,content_snapshot,content_hash,created_at)
     VALUES ('cp1',1,'src_1','text','S','v1','h1',datetime('now'))",
    [],
).unwrap();
conn.execute(
    "INSERT INTO content_plan_revisions (plan_id,revision,parent_revision,operation_type,content_json,generation_metadata_json,created_by,created_at)
     VALUES ('cp1',2,1,'manual_edit','{}',NULL,'user',datetime('now'))",
    [],
).unwrap();
conn.execute(
    "INSERT INTO content_plan_sources (plan_id,revision,source_id,source_type,title,content_snapshot,content_hash,created_at)
     VALUES ('cp1',2,'src_1','text','S','v2','h2',datetime('now'))",
    [],
).unwrap();
```

Assert both source rows remain and deleting revision 2 does not alter revision 1.

For the upgrade test, create a Migration-5 database with representative rows in `projects`, `artifacts`, `jobs`, `domain_events`, and `timelines`, mark migrations 1–5 as applied, run the normal migration runner, and assert every pre-existing row still exists.

- [ ] **Step 2: Run only Migration-6 tests and verify RED**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml migration_6 -- --nocapture
```

Expected: FAIL because Migration 6 tables do not exist.

- [ ] **Step 3: Add Migration 6 exactly after Migration 5**

Add:

```rust
Migration {
    version: 6,
    name: "ai_content_planning",
    sql: "
        CREATE TABLE IF NOT EXISTS content_plans (
            id TEXT PRIMARY KEY,
            project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
            title TEXT NOT NULL,
            current_revision INTEGER NOT NULL DEFAULT 1,
            status TEXT NOT NULL DEFAULT 'draft',
            approved_revision INTEGER,
            approved_at TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS content_plan_revisions (
            plan_id TEXT NOT NULL REFERENCES content_plans(id) ON DELETE CASCADE,
            revision INTEGER NOT NULL,
            parent_revision INTEGER,
            operation_type TEXT NOT NULL,
            content_json TEXT NOT NULL,
            generation_metadata_json TEXT,
            created_by TEXT NOT NULL,
            created_at TEXT NOT NULL,
            PRIMARY KEY(plan_id, revision)
        );

        CREATE TABLE IF NOT EXISTS content_plan_sources (
            plan_id TEXT NOT NULL,
            revision INTEGER NOT NULL,
            source_id TEXT NOT NULL,
            source_type TEXT NOT NULL,
            title TEXT NOT NULL,
            author TEXT,
            url TEXT,
            published_at TEXT,
            locator TEXT,
            content_snapshot TEXT NOT NULL,
            content_hash TEXT NOT NULL,
            created_at TEXT NOT NULL,
            PRIMARY KEY(plan_id, revision, source_id),
            FOREIGN KEY(plan_id, revision)
                REFERENCES content_plan_revisions(plan_id, revision)
                ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_content_plans_project ON content_plans(project_id);
        CREATE INDEX IF NOT EXISTS idx_content_plan_revisions_plan_revision ON content_plan_revisions(plan_id, revision);
        CREATE INDEX IF NOT EXISTS idx_content_plan_sources_plan_revision ON content_plan_sources(plan_id, revision);
        CREATE INDEX IF NOT EXISTS idx_content_plan_sources_plan_source ON content_plan_sources(plan_id, source_id);
    ",
},
```

Do not edit Migration 1–5 text.

- [ ] **Step 4: Run DB tests and verify GREEN**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test db_tests
```

Expected: PASS, including real Migration-5→6 preservation assertions.

- [ ] **Step 5: Commit**

```powershell
git add src-tauri/src/db/migrations.rs src-tauri/tests/db_tests.rs
git commit -m "feat(content-plan): add migration 6 schema"
```

---

## Task 2: Domain Types, Canonical Derivation, and Validation

**Files:**
- Create: `src-tauri/src/content_planning/mod.rs`
- Create: `src-tauri/src/content_planning/model.rs`
- Create: `src-tauri/src/content_planning/error.rs`
- Create: `src-tauri/src/content_planning/derivation.rs`
- Create: `src-tauri/src/content_planning/validation.rs`
- Modify: `src-tauri/src/lib.rs` only to expose `pub mod content_planning;`
- Test: `src-tauri/tests/content_plan_model_tests.rs`

**Interfaces:**
- Produces: `PlanningRequest`, `PlanningSource`, `SourceRef`, `ContentPlan`, `ContentScene`, `Claim`, `PlanStatus`, `GroundingStatus`, `MediaIntent`, `GenerationMetadata`, `ContentPlanSummary`, `ContentPlanRevisionSummary`, `PlannerProviderStatus`, `derive_plan_fields`, `validate_planning_request`, `validate_content_plan`.

- [ ] **Step 1: Write failing serialization/derivation/domain tests**

Test camelCase JSON, enum snake_case values, duplicate scene/order rejection, non-positive duration rejection, title/narration requirements, and canonical aggregates:

```rust
let mut plan = fixture_plan();
plan.scenes[0].narration = "First".into();
plan.scenes[0].duration_estimate_ms = 1_000;
plan.scenes[1].narration = "Second".into();
plan.scenes[1].duration_estimate_ms = 2_000;
derive_plan_fields(&mut plan);
assert_eq!(plan.full_narration, "First\n\nSecond");
assert_eq!(plan.estimated_duration_ms, 3_000);
```

- [ ] **Step 2: Run tests and verify RED**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test content_plan_model_tests
```

Expected: compile failure because the module/types do not exist.

- [ ] **Step 3: Implement the Phase-4 model contracts**

Use serde camelCase on structs and snake_case on enums. Keep Phase-4 `GenerationOptions` intentionally empty/YAGNI-compatible:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct GenerationOptions {}
```

Use `i64` milliseconds and `i64` revisions. `ContentPlan` contains derived/materialized `estimated_duration_ms` and `full_narration` fields.

- [ ] **Step 4: Implement canonical derivation**

```rust
pub fn derive_plan_fields(plan: &mut ContentPlan) {
    plan.scenes.sort_by_key(|s| s.order);
    plan.estimated_duration_ms = plan.scenes.iter().map(|s| s.duration_estimate_ms).sum();
    plan.full_narration = plan
        .scenes
        .iter()
        .map(|s| s.narration.trim())
        .collect::<Vec<_>>()
        .join("\n\n");
    plan.source_refs = derive_plan_source_refs(&plan.scenes);
}
```

Deduplicate plan-level `SourceRef` summaries by stable `(source_id, locator)` ordering.

- [ ] **Step 5: Implement public error categories and validators**

Define `ContentPlanningError` with stable user-safe codes from the spec. Implement `code(&self) -> &'static str` and `Display` without SQL/provider raw bodies/secrets.

Validation must check scene/claim IDs, scene order uniqueness, positive durations, non-empty title/narration, valid media intent by enum deserialization, and configured duration tolerance. Keep tolerance centralized as `DEFAULT_DURATION_TOLERANCE: f64 = 0.15`.

- [ ] **Step 6: Run model tests and full Rust tests**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test content_plan_model_tests
cargo test --manifest-path src-tauri/Cargo.toml
```

Expected: PASS.

- [ ] **Step 7: Commit**

```powershell
git add src-tauri/src/content_planning src-tauri/src/lib.rs src-tauri/tests/content_plan_model_tests.rs
git commit -m "feat(content-plan): add domain model and validation"
```

---

## Task 3: Grounding Integrity and Deterministic Manual Invalidation

**Files:**
- Create: `src-tauri/src/content_planning/grounding.rs`
- Modify: `src-tauri/src/content_planning/mod.rs`
- Test: `src-tauri/tests/content_plan_grounding_tests.rs`

**Interfaces:**
- Consumes: `ContentPlan`, `PlanningSource`, `GroundingStatus`.
- Produces: `validate_grounding(plan, sources)`, `invalidate_manual_grounding(base, edited)`.

- [ ] **Step 1: Write failing grounding tests**

Cover unknown source ID rejection, `supported` without sources, valid `model_knowledge`, uncertain conflicting references, narration-edit invalidation for all scene claims, direct claim edit invalidation for only that claim, and non-factual visual edit preserving grounding.

- [ ] **Step 2: Run tests and verify RED**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test content_plan_grounding_tests
```

- [ ] **Step 3: Implement source whitelist validation**

Build `HashSet<&str>` from the revision's `PlanningSource.source_id`. For every claim source ref and every scene summary ref, reject IDs outside that set with `ContentPlanningError::SourceGroundingInvalid`.

Enforce:

```rust
if claim.grounding_status == GroundingStatus::Supported
    && (claim.source_refs.is_empty() || !claim.grounding_valid)
{
    return Err(ContentPlanningError::SourceGroundingInvalid(...));
}
```

- [ ] **Step 4: Implement deterministic edit invalidation**

Match scenes by stable `scene.id`. If narration changes, mark every edited-scene claim `Modified` and `grounding_valid = false`. Otherwise compare claims by `claim.id`; if text changes, invalidate only that claim. Preserve `source_refs` as historical provenance.

- [ ] **Step 5: Run grounding + model tests**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test content_plan_grounding_tests
cargo test --manifest-path src-tauri/Cargo.toml --test content_plan_model_tests
```

Expected: PASS.

- [ ] **Step 6: Commit**

```powershell
git add src-tauri/src/content_planning src-tauri/tests/content_plan_grounding_tests.rs
git commit -m "feat(content-plan): enforce grounding integrity"
```

---

## Task 4: Repository — Immutable Revisions, Source Snapshots, Summaries, and History

**Files:**
- Create: `src-tauri/src/content_planning/repository.rs`
- Modify: `src-tauri/src/content_planning/mod.rs`
- Test: `src-tauri/tests/content_plan_repository_tests.rs`

**Interfaces:**
- Produces repository operations:

```rust
ContentPlanRepository::get_current(conn, plan_id)
ContentPlanRepository::get_revision(conn, plan_id, revision)
ContentPlanRepository::list_by_project(conn, project_id, include_archived)
ContentPlanRepository::list_history(conn, plan_id)
ContentPlanRepository::get_sources(conn, plan_id, revision)
ContentPlanRepository::create_initial_tx(tx, plan, sources, created_by)
ContentPlanRepository::append_revision_tx(tx, plan, sources, operation_type, created_by)
ContentPlanRepository::set_approval_tx(tx, plan_id, expected_revision, now)
ContentPlanRepository::archive_tx(tx, plan_id, expected_revision, now)
```

- [ ] **Step 1: Write failing repository tests**

Assert initial creation is atomic, title listing does not parse all revision JSON, revision numbers are monotonic, old revision JSON remains byte-identical after later revisions, same `source_id` can have different snapshots in revisions 1 and 2, history is ordered, archived plans are excluded by default, and deleting the project cascades all plan rows.

- [ ] **Step 2: Run repository tests and verify RED**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test content_plan_repository_tests
```

- [ ] **Step 3: Implement row mapping and read APIs**

Deserialize `content_json` into `ContentPlan` only for current/specific revision reads. `list_by_project` selects relational projection fields only:

```sql
SELECT id, project_id, title, current_revision, status,
       approved_revision, approved_at, created_at, updated_at
FROM content_plans
WHERE project_id = ?1 AND (?2 = 1 OR status <> 'archived')
ORDER BY updated_at DESC;
```

- [ ] **Step 4: Implement transactional initial/revision writes**

`create_initial_tx` inserts plan row + revision 1 + every source row in the caller transaction. `append_revision_tx` requires `plan.revision = current_revision + 1`, inserts immutable revision/source rows, then updates `content_plans.title/current_revision/status/updated_at` with a guarded `WHERE current_revision = expected` update.

Never use `INSERT OR REPLACE` for historical revision/source rows.

- [ ] **Step 5: Implement approval/archive guarded updates**

Use `UPDATE ... WHERE id=? AND current_revision=?` and require `rows_affected == 1`; otherwise return `ContentPlanRevisionConflict` or `ContentPlanNotFound` after a read.

- [ ] **Step 6: Run repository and DB tests**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test content_plan_repository_tests
cargo test --manifest-path src-tauri/Cargo.toml --test db_tests
```

- [ ] **Step 7: Commit**

```powershell
git add src-tauri/src/content_planning src-tauri/tests/content_plan_repository_tests.rs
git commit -m "feat(content-plan): add immutable revision repository"
```

---

## Task 5: ContentPlanService — Manual Save, Approval, Archive, and Transactional Domain Events

**Files:**
- Create: `src-tauri/src/content_planning/service.rs`
- Modify: `src-tauri/src/content_planning/mod.rs`
- Test: `src-tauri/tests/content_plan_service_tests.rs`

**Interfaces:**
- Consumes: `DbManager`, `EventBus`, repository, grounding/derivation/validation.
- Produces:

```rust
ContentPlanService::save_manual_revision(plan_id, expected_revision, edited)
ContentPlanService::approve(plan_id, expected_revision)
ContentPlanService::archive(plan_id, expected_revision)
ContentPlanService::publish_generated_revision(...)
```

- [ ] **Step 1: Write failing service tests**

Cover manual edit N→N+1 draft, stale revision conflict, edit-after-approved preserving approved revision, archive read-only rejection, transactional event rows, and rollback when revision/source insert fails.

- [ ] **Step 2: Run tests and verify RED**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test content_plan_service_tests
```

- [ ] **Step 3: Implement manual-save pipeline in exact order**

```text
load base revision + sources
assert not archived
assert expected_revision == current_revision
invalidate_manual_grounding(base, edited)
derive_plan_fields(edited)
validate_content_plan(edited)
validate_grounding(edited, sources)
set revision = expected_revision + 1
set status = draft
begin transaction
append revision + copied source snapshots
record content_plan.revision_created event in same transaction
commit
broadcast after commit
```

- [ ] **Step 4: Implement approval/archive**

Approval revalidates current plan + grounding before guarded status update; archive uses guarded optimistic update. Persist `content_plan.approved` / `content_plan.archived` in the same transaction and broadcast after commit.

- [ ] **Step 5: Implement generated-publication helper**

Provide one service method used by the worker that accepts the already-normalized/validated candidate, expected base revision (or `None` for initial creation), source snapshot set, operation type, generation metadata, and optional `job_id`. This method is the only worker-facing path that writes plan revisions.

- [ ] **Step 6: Run service/repository/grounding tests**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test content_plan_service_tests
cargo test --manifest-path src-tauri/Cargo.toml --test content_plan_repository_tests
cargo test --manifest-path src-tauri/Cargo.toml --test content_plan_grounding_tests
```

- [ ] **Step 7: Commit**

```powershell
git add src-tauri/src/content_planning src-tauri/tests/content_plan_service_tests.rs
git commit -m "feat(content-plan): add revision workflow service"
```

---

## Task 6: Provider Contracts, Runtime Config, Prompt Builder, and Deterministic MockProvider

**Files:**
- Create: `src-tauri/src/content_planning/provider/mod.rs`
- Create: `src-tauri/src/content_planning/provider/config.rs`
- Create: `src-tauri/src/content_planning/provider/mock.rs`
- Create: `src-tauri/src/content_planning/prompt.rs`
- Modify: `src-tauri/src/content_planning/mod.rs`
- Test: `src-tauri/tests/content_plan_provider_tests.rs`

**Interfaces:**
- Produces `AIProvider`, `ProviderError`, `ProviderConfigResolver`, `PromptBuilder`, `MockProvider`.

Define an object-safe provider interface suitable for worker injection:

```rust
pub trait AIProvider: Send + Sync {
    fn name(&self) -> &'static str;
    fn generate_planning_draft(&self, input: &ProviderRequest) -> Result<ProviderResponse, ProviderError>;
    fn generate_content_plan(&self, input: &ProviderRequest) -> Result<ProviderResponse, ProviderError>;
    fn regenerate_plan(&self, input: &ProviderRequest) -> Result<ProviderResponse, ProviderError>;
    fn regenerate_scene(&self, input: &ProviderRequest) -> Result<ProviderResponse, ProviderError>;
    fn regenerate_narration(&self, input: &ProviderRequest) -> Result<ProviderResponse, ProviderError>;
}
```

Use blocking trait methods initially so the existing worker can call them inside `spawn_blocking`; Gemini may use a blocking `reqwest::blocking::Client` or a small runtime bridge only after compatibility is verified. Keep network work off the UI thread.

- [ ] **Step 1: Add failing provider/config/prompt tests**

Test missing `GEMINI_API_KEY` → configured false / `provider_auth_missing`, status does not expose the key, prompt contains allowed source IDs and explicit anti-invention instructions, and `MockProvider` produces byte-stable output for the same request.

- [ ] **Step 2: Run provider tests and verify RED**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test content_plan_provider_tests
```

- [ ] **Step 3: Implement ProviderConfigResolver**

Read only `std::env::var("GEMINI_API_KEY")` for `gemini-default`. Return a secret-bearing internal config type that does not implement `Serialize` or `Debug`. Expose a separate safe `PlannerProviderStatus`.

- [ ] **Step 4: Implement PromptBuilder**

Version templates with `PROMPT_TEMPLATE_VERSION: i32 = 1`. Emit source blocks with backend-owned IDs and instructions equivalent to:

```text
Cite only IDs in ALLOWED_SOURCES.
Never invent source IDs, titles, authors, URLs, or locators.
Use model_knowledge or uncertain when supplied sources do not support a factual claim.
```

- [ ] **Step 5: Implement deterministic MockProvider outcomes**

Use an enum fixture mode (`Success`, `Malformed`, `Repairable`, `RepairExhausted`, `RateLimited`, `Timeout`, `AuthFailed`, `Unavailable`) selected by constructor. Never use random IDs in mock output; derive stable scene/claim IDs from indexes.

- [ ] **Step 6: Run provider tests**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test content_plan_provider_tests
```

- [ ] **Step 7: Commit**

```powershell
git add src-tauri/src/content_planning src-tauri/tests/content_plan_provider_tests.rs
git commit -m "feat(content-plan): add provider boundary and mock provider"
```

---

## Task 7: Two-Stage Planner and Bounded Structured-Output Repair

**Files:**
- Create: `src-tauri/src/content_planning/planner.rs`
- Modify: `src-tauri/src/content_planning/mod.rs`
- Modify/Test: `src-tauri/tests/content_plan_provider_tests.rs`

**Interfaces:**
- Produces `ContentPlanner` methods for generate/regenerate operations.
- Consumes provider-neutral JSON responses, then always applies backend derivation/validation/grounding.

- [ ] **Step 1: Add failing orchestration tests**

Assert normal generate calls draft then expansion exactly once; malformed first response can be repaired; repair exhaustion stops after 3 calls in that stage; rate-limit/timeout/unavailable remain retryable; auth errors are non-retryable; fake citations fail even if JSON schema is structurally valid.

- [ ] **Step 2: Run tests and verify RED**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test content_plan_provider_tests planner_
```

- [ ] **Step 3: Implement one bounded repair helper**

```rust
const MAX_MODEL_CALLS_PER_STAGE: usize = 3;
```

For each attempt: call provider → deserialize → normalize derived fields → domain/grounding validate. On structured/domain output failure, rebuild a repair request containing sanitized validation issues, not secrets or raw unrestricted source data beyond the original bounded context. Stop after attempt 3.

- [ ] **Step 4: Implement regeneration isolation**

For `regenerate_scene`, replace only the target scene in a cloned base plan; compare every non-target scene to the base after provider parsing and reject provider attempts to alter them. For `regenerate_narration`, copy only target narration/claims/sourceRefs into the base scene and leave title/duration/visual/media/search/prompts/order unchanged. Recompute plan aggregates afterward.

- [ ] **Step 5: Run provider and grounding tests**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test content_plan_provider_tests
cargo test --manifest-path src-tauri/Cargo.toml --test content_plan_grounding_tests
```

- [ ] **Step 6: Commit**

```powershell
git add src-tauri/src/content_planning src-tauri/tests/content_plan_provider_tests.rs
git commit -m "feat(content-plan): add two-stage planner orchestration"
```

---

## Task 8: GeminiProvider HTTP Transport and Live-Test Gate

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/Cargo.lock`
- Create: `src-tauri/src/content_planning/provider/gemini.rs`
- Modify: `src-tauri/src/content_planning/provider/mod.rs`
- Create/Test: `src-tauri/tests/content_plan_live_gemini_tests.rs`
- Modify/Test: `src-tauri/tests/content_plan_provider_tests.rs`

**Interfaces:**
- Produces `GeminiProvider` implementing `AIProvider` and normalized `ProviderError` categories.

- [ ] **Step 1: Verify dependency compatibility before editing Cargo files**

Run:

```powershell
cargo search reqwest --limit 1
cargo tree --manifest-path src-tauri/Cargo.toml
```

Then add the approved transport dependency:

```toml
reqwest = { version = "0.12", default-features = false, features = ["blocking", "json", "rustls-tls"] }
```

Use `blocking` because the provider trait is executed in the background worker's blocking task; do not block Tauri IPC/UI threads.

- [ ] **Step 2: Add transport/error-mapping tests without live network**

Factor response classification into pure helpers and test HTTP 401/403 → auth failure, 429 → rate limited, 408/timeout → timeout, 5xx → unavailable, malformed response → structured-output-invalid.

- [ ] **Step 3: Implement Gemini request construction against the verified current REST contract**

Before writing the URL/body, consult the current official Gemini API documentation and record the verified endpoint/model/schema fields in code comments next to the transport. Use `response_mime_type = application/json` and schema-constrained response configuration only if verified for the selected model. Do not introduce unverified SDK method names.

- [ ] **Step 4: Sanitize provider failures**

Never include API key, full request body, full source snapshots, or unrestricted raw response in returned/logged errors. Record only status class, provider code if safe, and normalized category.

- [ ] **Step 5: Add live acceptance test gating**

```rust
#[test]
fn live_gemini_content_plan_smoke() {
    let Ok(_) = std::env::var("GEMINI_API_KEY") else {
        eprintln!("SKIP: GEMINI_API_KEY not set");
        return;
    };
    // Call the real provider with a tiny topic and assert valid structured output.
}
```

The test must validate no invented source IDs and non-empty scenes/narration. It must not report a fake success when the environment variable is absent; console output must explicitly say it was skipped.

- [ ] **Step 6: Run offline provider suite and optionally live test**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test content_plan_provider_tests
cargo test --manifest-path src-tauri/Cargo.toml --test content_plan_live_gemini_tests -- --nocapture
```

- [ ] **Step 7: Commit**

```powershell
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/content_planning/provider src-tauri/tests/content_plan_provider_tests.rs src-tauri/tests/content_plan_live_gemini_tests.rs
git commit -m "feat(content-plan): add Gemini provider transport"
```

---

## Task 9: Durable Job Payloads, Idempotency, and Submission Helpers

**Files:**
- Create: `src-tauri/src/content_planning/jobs.rs`
- Modify: `src-tauri/src/content_planning/model.rs`
- Modify: `src-tauri/src/content_planning/mod.rs`
- Test: `src-tauri/tests/content_plan_jobs_tests.rs`

**Interfaces:**
- Produces `PlanningOperation`, `ContentPlanJobPayload`, `submit_generate_job`, `submit_regeneration_job`, deterministic request/context hashes, idempotency keys.

- [ ] **Step 1: Write failing job-contract tests**

Assert every operation submits `job_type == "content.plan"`, payload version 1, max retries 3, no API key substring, identical immutable requests converge to the same job through existing idempotency, and different base revision/scene/instruction changes the key.

- [ ] **Step 2: Run tests and verify RED**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test content_plan_jobs_tests
```

- [ ] **Step 3: Implement typed payload and canonical hashing**

Serialize a canonical struct rather than arbitrary `serde_json::Value`. Hash stable JSON with SHA-256 already present in the project. If `PlanningRequest.source_context` and top-level `source_context` are both serialized for spec compatibility, enforce equality at construction/parsing and treat top-level `source_context` as the worker's snapshot source.

- [ ] **Step 4: Implement idempotency formats**

```text
content.plan:generate:<projectId>:<inputHash>:<contextHash>:<plannerProfileVersion>
content.plan:regenerate_plan:<planId>:<baseRevision>:<instructionHash>:<plannerProfileVersion>
content.plan:regenerate_scene:<planId>:<baseRevision>:<sceneId>:<instructionHash>:<plannerProfileVersion>
content.plan:regenerate_narration:<planId>:<baseRevision>:<sceneId>:<instructionHash>:<plannerProfileVersion>
```

- [ ] **Step 5: Run job tests**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test content_plan_jobs_tests
```

- [ ] **Step 6: Commit**

```powershell
git add src-tauri/src/content_planning src-tauri/tests/content_plan_jobs_tests.rs
git commit -m "feat(content-plan): add durable planning job contracts"
```

---

## Task 10: LocalContentPlanWorker — Checkpoints, Heartbeats, Cancellation, Fencing, Publication

**Files:**
- Create: `src-tauri/src/content_planning/worker.rs`
- Modify: `src-tauri/src/content_planning/mod.rs`
- Test: `src-tauri/tests/content_plan_worker_tests.rs`

**Interfaces:**
- Produces `LocalContentPlanWorker`, `LocalContentPlanWorkerHandle`, `start_local_content_plan_worker`.
- Consumes `JobManager::acquire_next_job_for_type("content.plan", 60)`, token-fenced checkpoints/heartbeat/completion, `ContentPlanner`, `ContentPlanService`.

- [ ] **Step 1: Write failing worker tests**

Cover: only `content.plan` leased; expected generate checkpoints; heartbeat extends 60s lease; cancel before provider call; cancel after provider response before persistence; stale lease blocks revision publication; base-revision conflict discards output; retryable provider error calls `fail_job_with_token(..., allow_retry=true)`; auth error uses `allow_retry=false`; shutdown prevents new acquisition.

- [ ] **Step 2: Run tests and verify RED**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test content_plan_worker_tests
```

- [ ] **Step 3: Implement worker core and explicit fence check**

Provide a helper that reloads the job and verifies: not cancelled, `status == Running`, `leased_by` matches this manager worker ID, and `lease_token` matches the acquired token. Use token-fenced JobManager APIs for every checkpoint/completion.

- [ ] **Step 4: Implement generate checkpoint sequence**

```text
validate_request
prepare_context
generate_outline
validate_outline
generate_content
validate_content
persist_revision
finalize
```

Persist only after a fresh cancellation/lease check. Complete the job only after the revision transaction commits.

- [ ] **Step 5: Implement regeneration checkpoint sequence and publication conflict guard**

Load immutable base revision/source snapshots specified by payload. Immediately before publication verify current revision still equals `base_revision`. On mismatch, do not write a revision; fail with sanitized `content_plan_revision_conflict` and no retry.

- [ ] **Step 6: Implement 15-second heartbeat during provider work**

Use a cooperative heartbeat loop or provider-call wrapper that keeps `heartbeat_with_token(job_id, token, 60)` alive every 15 seconds while the provider call is running. If heartbeat returns false/error, mark the call result unusable and never publish it.

- [ ] **Step 7: Implement startup loop mirroring Phase 3 runtime strategy**

Use `watch::channel(false)`, `OnceLock<tokio::runtime::Runtime>`, configurable poll interval defaulting to 500ms, and `spawn_blocking` for the synchronous provider pipeline. Return a handle with `async fn shutdown(self)`.

- [ ] **Step 8: Run worker + existing job concurrency suites**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test content_plan_worker_tests
cargo test --manifest-path src-tauri/Cargo.toml --test job_concurrency_tests
cargo test --manifest-path src-tauri/Cargo.toml --test job_type_lease_tests
```

- [ ] **Step 9: Commit**

```powershell
git add src-tauri/src/content_planning src-tauri/tests/content_plan_worker_tests.rs
git commit -m "feat(content-plan): add fenced local planning worker"
```

---

## Task 11: Typed Tauri IPC and AppState/Startup Wiring

**Files:**
- Create: `src-tauri/src/content_planning/commands.rs`
- Modify: `src-tauri/src/content_planning/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/tests/content_plan_ipc_tests.rs`

**Interfaces:**
- Produces commands named exactly:

```text
submit_content_plan_job
get_content_plan
list_project_content_plans
save_content_plan_revision
submit_content_plan_regeneration
approve_content_plan
archive_content_plan
get_content_plan_history
get_content_plan_revision
get_planner_provider_status
```

- [ ] **Step 1: Write failing IPC command tests**

Test typed argument mapping, returned `Job`/plan/summary/history/status types, revision conflicts, archived behavior, and that provider status contains no credential value.

- [ ] **Step 2: Run IPC tests and verify RED**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test content_plan_ipc_tests
```

- [ ] **Step 3: Implement pure `cmd_*` helpers plus `#[tauri::command]` wrappers**

Follow the current timeline pattern: testable helpers receive `Arc<DbManager>` / service / job manager directly; Tauri wrappers obtain dependencies from `State<'_, AppState>`.

- [ ] **Step 4: Extend AppState with planning services**

Add `content_plan_service: Arc<ContentPlanService>` and a provider resolver/registry object that contains no serialized secrets. Construct them after DB/EventBus/JobManager initialization.

- [ ] **Step 5: Start LocalContentPlanWorker exactly once in `.setup(...)`**

Use worker ID `content-plan-worker-<8 chars>`, 500ms poll interval, and provider registry containing Gemini for normal runtime. Preserve the existing render worker startup.

- [ ] **Step 6: Register all 10 commands in `tauri::generate_handler!`**

Do not remove existing Phase 0–3 handlers.

- [ ] **Step 7: Run IPC and all Rust tests**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test content_plan_ipc_tests
cargo test --manifest-path src-tauri/Cargo.toml
```

- [ ] **Step 8: Commit**

```powershell
git add src-tauri/src/lib.rs src-tauri/src/content_planning src-tauri/tests/content_plan_ipc_tests.rs
git commit -m "feat(content-plan): expose planner IPC and runtime wiring"
```

---

## Task 12: Frontend Types/API, Project Navigation, Plan List, and New Plan Form

**Files:**
- Create: `src/features/planner/types.ts`
- Create: `src/features/planner/api.ts`
- Create: `src/features/planner/components/PlannerView.tsx`
- Create: `src/features/planner/components/PlanList.tsx`
- Create: `src/features/planner/components/NewPlanForm.tsx`
- Create: `src/features/planner/__tests__/PlannerView.test.tsx`
- Modify: `src/features/projects/components/ProjectDetail.tsx`
- Modify: `src/features/projects/__tests__/ProjectDetail.test.tsx`
- Modify: `src/app/App.tsx`

**Interfaces:**
- Produces typed frontend wrappers corresponding exactly to Task 11 command names.

- [ ] **Step 1: Write failing navigation/API/UI tests**

Mock `@tauri-apps/api/core.invoke`. Test ProjectDetail shows `AI Planner`; clicking it opens Planner for the active project; plan list loads multiple plans; provider-unconfigured state is visible; Topic/Script mode switches; Generate invokes `submit_content_plan_job` and surfaces returned job ID.

- [ ] **Step 2: Run targeted frontend tests and verify RED**

```powershell
npm test -- --run src/features/planner/__tests__/PlannerView.test.tsx src/features/projects/__tests__/ProjectDetail.test.tsx
```

- [ ] **Step 3: Implement TypeScript domain mirrors**

Use exact camelCase fields matching Rust serde. Define discriminated unions for status/grounding/media intent/operation, not arbitrary strings.

- [ ] **Step 4: Implement typed API wrappers**

Example:

```ts
export async function listProjectContentPlans(projectId: string): Promise<ContentPlanSummary[]> {
  return await invoke<ContentPlanSummary[]>("list_project_content_plans", { projectId });
}
```

Mutation wrappers pass `expectedRevision` / `baseRevision` explicitly.

- [ ] **Step 5: Implement PlannerView list/create state**

`PlannerView` owns `selectedPlanId`, `isCreating`, `activeJobId`, list refresh, and child view switching. Do not add a second job store; provide an `onOpenJobs` callback to App.

- [ ] **Step 6: Implement NewPlanForm**

Inputs: Topic/Script, text, target duration, language, tone, optional source rows (`sourceId`, sourceType, title, locator, contentSnapshot), provider configured state. Build `contentHash` deterministically in backend, not in UI, if backend model accepts omitted/empty client hash; otherwise expose a backend helper within submission parsing rather than trusting browser-supplied hashes.

- [ ] **Step 7: Wire App/ProjectDetail navigation**

Extend App view state with a planner project ID, preserving Projects/Jobs/Health/Timeline behavior. `ProjectDetailProps` adds `onOpenPlanner?: () => void` next to `onOpenEditor`.

- [ ] **Step 8: Run frontend tests/typecheck**

```powershell
npm test -- --run src/features/planner/__tests__/PlannerView.test.tsx src/features/projects/__tests__/ProjectDetail.test.tsx
npm run typecheck
```

- [ ] **Step 9: Commit**

```powershell
git add src/features/planner src/features/projects src/app/App.tsx
git commit -m "feat(planner-ui): add planner navigation and generation form"
```

---

## Task 13: Structured ContentPlan Editor, Dirty State, Grounding Display, and History

**Files:**
- Create: `src/features/planner/components/ContentPlanEditor.tsx`
- Create: `src/features/planner/components/SceneEditor.tsx`
- Create: `src/features/planner/components/GroundingBadge.tsx`
- Create: `src/features/planner/components/RevisionHistory.tsx`
- Create/Test: `src/features/planner/__tests__/ContentPlanEditor.test.tsx`
- Modify: `src/features/planner/components/PlannerView.tsx`

**Interfaces:**
- Produces explicit-save structured editor using `save_content_plan_revision`, read-only historical revision view, grounding warnings, approval summary data.

- [ ] **Step 1: Write failing editor tests**

Cover scene selection, local edits do not invoke backend until Save, Save passes current revision, successful Save clears dirty state and updates revision, stale conflict shows reload message, unsaved navigation prompts Save/Discard, historical revision is read-only, full narration is displayed from plan aggregate and is not independently editable, grounding badges render all four states.

- [ ] **Step 2: Run editor tests and verify RED**

```powershell
npm test -- --run src/features/planner/__tests__/ContentPlanEditor.test.tsx
```

- [ ] **Step 3: Implement editor local draft semantics**

Clone the loaded plan into local state. Only scene fields listed by the spec are editable. Save sends the draft plus `expectedRevision = loaded.revision`; backend remains authoritative for grounding invalidation and derived-field recomputation.

- [ ] **Step 4: Implement SceneEditor and GroundingBadge**

Show narration, duration, visual description, media intent, stock queries, image/video prompt hints, claims, source refs, and grounding note. Do not expose editing of backend source IDs through arbitrary free text on existing claims; source choices must come from the revision's source context.

- [ ] **Step 5: Implement RevisionHistory**

Load summaries using `get_content_plan_history`; selecting a revision calls `get_content_plan_revision` and opens it read-only. Display revision, operationType, createdBy, createdAt, and approved indication when it matches `approvedRevision`.

- [ ] **Step 6: Run editor tests, all frontend tests, typecheck**

```powershell
npm test -- --run src/features/planner/__tests__/ContentPlanEditor.test.tsx
npm test
npm run typecheck
```

- [ ] **Step 7: Commit**

```powershell
git add src/features/planner
git commit -m "feat(planner-ui): add structured content plan editor"
```

---

## Task 14: Regeneration, Approval/Archive UX, and Jobs & Queue Navigation

**Files:**
- Modify: `src/features/planner/components/ContentPlanEditor.tsx`
- Modify: `src/features/planner/components/SceneEditor.tsx`
- Modify: `src/features/planner/components/PlannerView.tsx`
- Modify/Test: `src/features/planner/__tests__/ContentPlanEditor.test.tsx`
- Modify/Test: `src/features/planner/__tests__/PlannerView.test.tsx`
- Modify: `src/app/App.tsx`

**Interfaces:**
- Consumes Task-12/13 API wrappers.
- Produces regenerate plan/scene/narration actions, approval summary/confirmation, archive action, existing Jobs view handoff.

- [ ] **Step 1: Add failing UX tests**

Assert regeneration is blocked with unsaved local edits; plan regeneration sends `baseRevision`; scene/narration regeneration sends `sceneId` and optional instruction; approval shows claim warning counts; fatal backend validation error is displayed; archive requires current revision; job response exposes `View Job` that switches to existing Jobs & Queue.

- [ ] **Step 2: Run tests and verify RED**

```powershell
npm test -- --run src/features/planner/__tests__/PlannerView.test.tsx src/features/planner/__tests__/ContentPlanEditor.test.tsx
```

- [ ] **Step 3: Implement regeneration controls**

All three operations show copy that the previous revision is preserved and the result becomes a new draft. Do not optimistic-edit the plan content with provider output; wait for the durable job to complete and then refresh through normal plan APIs/events.

- [ ] **Step 4: Implement approval/archive UX**

Before approval show scene count, duration vs target, and counts for `model_knowledge`, `uncertain`, and `modified`. Backend remains authoritative for blocking errors. Archive returns to the project plan list after success.

- [ ] **Step 5: Wire Jobs & Queue navigation**

App receives planner `onOpenJobs(jobId?)`; switching to the existing Jobs tab is sufficient for Phase 4. If the existing JobList does not accept a selected ID, do not create a second queue; display the returned job ID in Planner and switch tabs.

- [ ] **Step 6: Run all frontend tests and build**

```powershell
npm test
npm run typecheck
npm run build
```

- [ ] **Step 7: Commit**

```powershell
git add src/features/planner src/app/App.tsx
git commit -m "feat(planner-ui): add regeneration and approval workflows"
```

---

## Task 15: Cross-Layer Acceptance, Security Regression, Desktop Runtime, and Release-Close Evidence

**Files:**
- Modify if needed for genuine defects only: Phase-4 files from Tasks 1–14
- Modify/Test: `src-tauri/tests/content_plan_jobs_tests.rs`
- Modify/Test: `src-tauri/tests/content_plan_worker_tests.rs`
- Modify/Test: `src-tauri/tests/content_plan_ipc_tests.rs`
- Modify/Test: `src-tauri/tests/content_plan_live_gemini_tests.rs`
- Modify/Test: frontend planner tests
- Do not weaken `scripts/verify.ps1`.

**Interfaces:**
- Produces release evidence only after all gates pass.

- [ ] **Step 1: Add/confirm security regression assertions**

Serialize submitted jobs, revision JSON, generation metadata, domain events, provider status, and UI error payloads; assert they do not contain a seeded fake credential such as `phase4-secret-sentinel`. Assert provider error bodies are sanitized.

- [ ] **Step 2: Run complete Rust suite**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml
```

Expected: all Phase 0–4 tests PASS.

- [ ] **Step 3: Run complete frontend suite and production build**

```powershell
npm run typecheck
npm test
npm run build
```

Expected: PASS.

- [ ] **Step 4: Run repository verification harness**

```powershell
powershell -ExecutionPolicy Bypass -File scripts/verify.ps1
```

Expected: all 6 quality gates PASS; do not modify the script merely to bypass a failure.

- [ ] **Step 5: Run live Gemini acceptance when credential is available**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test content_plan_live_gemini_tests -- --nocapture
```

Expected with `GEMINI_API_KEY`: real valid plan. Expected without it: explicit `SKIP: GEMINI_API_KEY not set` evidence.

- [ ] **Step 6: Run real Tauri desktop acceptance**

```powershell
npm run tauri dev
```

Manually execute the spec Section 28 sequence: create/open project → AI Planner → Topic and Script generation → Jobs checkpoints → edit/save revision → grounding invalidation → regenerate scene → regenerate narration → approval → edit after approval → restart → verify persistence. Confirm no blank/black WebView and no unhandled frontend console error.

- [ ] **Step 7: Verify scope and secrets by repository search**

Run:

```powershell
git diff v0.4.0...HEAD --name-only
git grep -n "GEMINI_API_KEY"
git grep -n "phase4-secret-sentinel"
git status --short
```

Expected: credential name appears only in runtime config/tests/docs; sentinel appears only in tests; no Phase-5 subsystem files; working tree clean after final committed fixes.

- [ ] **Step 8: Produce final Phase-4 implementation report before tagging**

Report exact test counts, `scripts/verify.ps1` result, live Gemini pass/explicit skip, desktop acceptance, restart persistence, migration upgrade proof, security scan, final commit SHA, and `git status --short`. Do not create `v0.5.0` until external release-close audit approves the evidence.

- [ ] **Step 9: Commit any acceptance-only test/report changes**

```powershell
git add src-tauri/tests src/features/planner
git commit -m "test(content-plan): complete phase 4 acceptance coverage"
```

Skip this commit if Step 1 required no file changes.

---

## Self-Review Record

### Spec coverage

- Domain model + canonical aggregates: Tasks 2–3.
- Migration 6 + real 5→6 upgrade + revision-scoped source snapshots: Task 1.
- Immutable revisions, listing projection, history: Task 4.
- Manual edit/approval/archive/events: Task 5.
- Provider boundary/config/prompt/mock: Task 6.
- Two-stage planner, bounded repair, regeneration isolation: Task 7.
- Gemini transport + credential-gated acceptance: Task 8.
- Uniform `content.plan` durable jobs + idempotency: Task 9.
- Worker lifecycle/heartbeat/cancellation/lease fencing/conflict publication: Task 10.
- Complete Tauri IPC + AppState/runtime wiring: Task 11.
- Project navigation/list/create UI: Task 12.
- Structured editor/dirty state/grounding/history: Task 13.
- Regeneration/approval/archive/Jobs integration: Task 14.
- Regression/security/live/desktop/restart/release gate: Task 15.

### Type consistency

- Rust serialization uses camelCase structs and snake_case enum values.
- Revision fields use `i64` consistently across repository/service/IPC/job payload.
- All planning jobs use one `job_type`: `content.plan`.
- All durable mutation APIs carry `expected_revision` or `base_revision`.
- Frontend Tauri wrappers pass camelCase argument keys matching Tauri's command mapping.

### Placeholder scan

The plan contains no unresolved implementation placeholders. Model/provider endpoint details that can change externally are handled by an explicit verification step against current official Gemini documentation before transport code is written, as required by the approved spec.

---

## Execution Handoff

Plan execution starts only after the plan itself is reviewed/approved. Before coding, use `superpowers:using-git-worktrees` if an isolated worktree is desired, then execute task-by-task with strict TDD and review gates.

Recommended execution mode: **superpowers:subagent-driven-development** with one fresh implementer per task and review between tasks. Alternative: **superpowers:executing-plans** for sequential inline execution with checkpoints.

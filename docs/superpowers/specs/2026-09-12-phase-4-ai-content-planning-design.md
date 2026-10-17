# OpenVideoAI Phase 4 — AI Content Planning Design

**Date:** 2026-09-12  
**Target version:** v0.5.0  
**Target branch:** `phase/4-ai-content-planning`  
**Repository:** `D:\OpenVideoAI`  
**Base:** `v0.4.0` / commit `0f08683`  
**Classification:** Architectural subsystem addition  
**Status:** Revised after design-spec audit; ready for final review before implementation planning

---

## 1. Goal

Phase 4 introduces the AI Content Planning layer for OpenVideoAI.

The product accepts either a topic or an existing script and produces a versioned, editable, source-aware `ContentPlan` containing scene structure, narration, duration estimates, visual planning, media intent, stock-search queries, image/video prompt hints, claims, and grounding metadata.

Phase 4 establishes a stable planning contract for later Topic → Video phases. It does **not** create/download media, generate speech, modify timelines, or render video.

Primary flow:

```text
Topic ──────┐
            ├─→ PlanningRequest
Script ─────┘
               + Optional Source Context
                        ↓
                  content.plan job
                        ↓
                 ContentPlanner
                        ↓
              Two-Stage AI Planning
              1. Planning Draft
              2. Content Expansion
                        ↓
                 Schema Validation
                        ↓
                Grounding Validation
                        ↓
               ContentPlan Revision
                        ↓
              Structured Planner UI
```

---

## 2. Design Principles

1. **Provider-agnostic core.** Planning logic must not depend on Gemini-specific transport details.
2. **Durable-by-default.** AI generation uses the existing Phase 2 durable Job Manager.
3. **Versioned content.** Every durable content change creates a new immutable revision.
4. **Grounding-aware.** Supplied-source claims are distinguishable from model knowledge and uncertain claims.
5. **No fake citations.** Providers may cite only logical source IDs supplied by the backend.
6. **Deterministic tests.** `MockProvider` is the primary automated-test provider.
7. **Secrets stay out of durable state.** API keys never enter job payloads, revision JSON, events, or logs.
8. **No direct timeline coupling.** Content planning cannot mutate timeline state.
9. **Canonical derivation.** Plan-level aggregates are derived deterministically from ordered scenes.
10. **Scope discipline.** TTS, Whisper, stock execution, web research, image/video generation, and timeline insertion remain outside Phase 4.

---

## 3. Approved Approach

### 3.1 Input modes

```text
Topic
Script
```

Both normalize into one `PlanningRequest` and produce the same canonical `ContentPlan` model.

### 3.2 Planning strategy

Use a **two-stage planner inside one durable job**.

Stage 1 produces:

```text
outline
scene structure
duration allocation
high-level narrative arc
```

Stage 2 expands the draft into:

```text
scene narration
visual descriptions
media intent
stock search queries
image prompt hints
video prompt hints
claims
source grounding
```

The architecture is intentionally not multi-agent in Phase 4.

---

## 4. Core Domain Model

### 4.1 PlanningRequest

```ts
interface PlanningRequest {
  projectId: string;
  inputType: "topic" | "script";
  inputText: string;
  targetDurationMs: number;
  language: string;
  tone: string;
  sourceContext: PlanningSource[];
  generationOptions: GenerationOptions;
}
```

Required invariants:

```text
projectId exists
inputText is non-empty
inputType is valid
targetDurationMs > 0
language is non-empty
source IDs are unique
request stays within configured planning-context limits
```

### 4.2 ContentPlan

```ts
interface ContentPlan {
  id: string;
  projectId: string;
  revision: number;
  status: "draft" | "approved" | "archived";
  title: string;
  summary: string;
  language: string;
  tone: string;
  targetDurationMs: number;
  estimatedDurationMs: number;   // derived
  fullNarration: string;         // derived/materialized
  scenes: ContentScene[];
  sourceRefs: SourceRef[];
  generationMetadata: GenerationMetadata | null;
  createdAt: string;
  updatedAt: string;
}
```

### 4.3 Canonical plan-level derivation rules

To avoid two sources of truth:

```text
estimatedDurationMs = sum(scene.durationEstimateMs ordered by scene.order)
fullNarration       = ordered scene narrations joined with "\n\n"
```

`scenes[].narration` and `scenes[].durationEstimateMs` are canonical. The backend recomputes `estimatedDurationMs` and `fullNarration` before validation/persistence for generation, regeneration, and manual-save operations.

Provider-supplied plan-level aggregate values are never trusted as independent canonical state. If present in a provider response they are ignored or checked only for diagnostics.

The Phase 4 UI may display the full narration aggregate, but direct durable editing occurs through scene narration fields so synchronization is deterministic.

### 4.4 ContentScene

```ts
interface ContentScene {
  id: string;
  order: number;
  title: string;
  narration: string;
  durationEstimateMs: number;
  visualDescription: string;
  mediaIntent: MediaIntent[];
  stockSearchQueries: string[];
  imagePrompt: string | null;
  videoPrompt: string | null;
  claims: Claim[];
  sourceRefs: SourceRef[];
}
```

### 4.5 MediaIntent

Supported planning-only values:

```text
stock_video
stock_image
ai_image
ai_video
screen_recording
text_graphic
```

### 4.6 Claim

```ts
interface Claim {
  id: string;
  text: string;
  groundingStatus: GroundingStatus;
  sourceRefs: SourceRef[];
  groundingValid: boolean;
  note: string | null;
}
```

`GroundingStatus`:

```text
supported
model_knowledge
uncertain
modified
```

Semantics:

- `supported`: must contain at least one valid supplied-source reference and `groundingValid = true`.
- `model_knowledge`: comes from model knowledge outside supplied sources; it must not masquerade as supplied-source grounding.
- `uncertain`: evidence is weak/conflicting/insufficient.
- `modified`: user-edited factual content whose prior grounding is no longer automatically valid; `groundingValid = false`.

---

## 5. Source and Grounding Model

### 5.1 PlanningSource

```ts
interface PlanningSource {
  sourceId: string;
  sourceType: string;
  title: string;
  author: string | null;
  url: string | null;
  publishedAt: string | null;
  locator: string | null;
  contentSnapshot: string;
  contentHash: string;
}
```

### 5.2 SourceRef

```ts
interface SourceRef {
  sourceId: string;
  locator: string | null;
  excerptHash: string | null;
  note: string | null;
}
```

### 5.3 Citation integrity rules

The provider may only return `sourceId` values supplied in the immutable planning context.

The provider must never invent:

```text
source IDs
URLs
authors
titles
bibliographic entries
```

Backend validation rejects unknown/dangling source IDs with `source_grounding_invalid`.

A `supported` claim without a valid source reference is invalid.

A `model_knowledge` claim must not be represented as supplied-source grounded.

If supplied sources disagree, the output must preserve uncertainty rather than silently selecting one as truth.

Scene-level `sourceRefs` are convenience summaries derived from claim-level references and are not the canonical grounding source of truth.

### 5.4 Deterministic manual-grounding invalidation

No NLP alignment model is required in Phase 4. The rules are deterministic:

1. If a user manually edits `scene.narration`, **all claims attached to that scene** become:

```text
groundingStatus = modified
groundingValid  = false
```

Their historical `sourceRefs` may remain for provenance but must be visually marked unvalidated.

2. If a user directly edits `claim.text`, only that claim becomes `modified` / `groundingValid = false`.

3. Editing non-factual planning fields such as visual description, media intent, prompts, or search queries does not invalidate claim grounding.

4. A later AI regeneration or explicit future grounding operation may produce new validated claim grounding in a new revision; Phase 4 does not silently revalidate modified claims.

---

## 6. Provider Architecture

### 6.1 Boundary

```text
ContentPlanner
     ↓
AIProvider
├─ GeminiProvider
└─ MockProvider
```

Conceptual operations:

```text
generatePlanningDraft(request, context)
generateContentPlan(draft, context)
regeneratePlan(...)
regenerateScene(...)
regenerateNarration(...)
```

Exact Rust method names may follow current repository conventions, but ownership boundaries are fixed.

### 6.2 Provider responsibilities

Providers may:

- construct provider-compatible HTTP request bodies from already-prepared prompts,
- request structured output,
- deserialize provider responses,
- expose provider usage metadata when available,
- normalize provider transport errors.

Providers must not:

- write SQLite,
- create/increment plan revisions,
- approve/archive plans,
- submit durable jobs,
- mutate timelines,
- resolve Artifact filesystem paths,
- expose/log secrets.

### 6.3 GeminiProvider

Gemini is the first real provider. The design requires structured/schema-constrained JSON output where supported by the selected Gemini API/model.

The design intentionally does **not** lock to an unverified Gemini SDK method name. The implementation plan must verify the current Gemini REST contract/model capability before coding the transport layer.

Backend schema/domain/grounding validation remains authoritative even when Gemini returns schema-valid JSON.

### 6.4 HTTP client dependency

Phase 4 may add the following Rust dependency to `src-tauri/Cargo.toml` after compatibility verification:

```toml
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }
```

The implementation plan must confirm this is compatible with the existing toolchain/Cargo.lock before changing production dependencies.

### 6.5 MockProvider

`MockProvider` is deterministic and supports configurable outcomes:

```text
success
malformed structured output
repairable output
repair exhausted
rate limited
timeout
auth failure
provider unavailable
```

Automated tests must not require network access or live AI quota.

---

## 7. Structured Output and Validation

Provider output undergoes schema validation and domain validation.

At minimum the backend validates:

```text
scene IDs unique
scene order unique and deterministic
durationEstimateMs > 0
estimated duration recomputed from scenes
estimated duration within configured tolerance of target duration
mediaIntent values valid
scene narration non-empty
plan title non-empty
supported claims have valid source refs
all source refs resolve to the revision's source snapshot
no duplicate claim IDs
no dangling source references
fullNarration recomputed from ordered scene narration
```

Recommended target-duration tolerance is approximately ±15% for AI planning output. This is planning validation, not render timing accuracy.

---

## 8. Repair and Retry Policy

Per model-generation stage:

```text
Attempt 1
↓ invalid schema/domain
Repair attempt 1
↓ invalid
Repair attempt 2
↓ still invalid
stage fails
```

Maximum three model calls per stage unless implementation constraints justify a stricter bound.

No infinite repair loop is allowed.

Error policy:

```text
provider_rate_limited
provider_timeout
provider_unavailable
→ retryable durable job

provider_auth_missing
provider_auth_failed
→ failed; do not retry indefinitely

structured_output_invalid
→ bounded repair inside the current stage

content_plan_validation_failed
source_grounding_invalid
→ fail when repair budget is exhausted
```

---

## 9. Durable Job Architecture

### 9.1 Single job type

All Phase 4 planning and regeneration jobs use one SQLite job type:

```text
jobs.job_type = "content.plan"
```

The operation is stored only inside the immutable payload:

```text
generate
regenerate_plan
regenerate_scene
regenerate_narration
```

A render/planning worker therefore performs type-safe leasing without ever acquiring another subsystem's job.

### 9.2 Immutable job payload

```json
{
  "schemaVersion": 1,
  "projectId": "...",
  "operation": "generate",
  "planningRequest": {},
  "sourceContext": [],
  "basePlanId": null,
  "baseRevision": null,
  "sceneId": null,
  "userInstruction": null,
  "provider": "gemini",
  "providerConfigRef": "gemini-default",
  "plannerProfileVersion": 1
}
```

The payload must not contain API keys.

The worker operates on this immutable snapshot and never silently rebuilds generation context from newer UI state.

### 9.3 Checkpoints

Generate:

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

Scene regeneration:

```text
validate_request
load_base_revision
prepare_scene_context
generate_scene
validate_scene
persist_revision
finalize
```

Narration regeneration follows equivalent stage-level checkpointing.

### 9.4 Idempotency

Job type remains `content.plan`; informative operation names may appear only inside idempotency keys.

Generate:

```text
content.plan:generate:<projectId>:<inputHash>:<contextHash>:<plannerProfileVersion>
```

Full-plan regeneration:

```text
content.plan:regenerate_plan:<planId>:<baseRevision>:<instructionHash>:<plannerProfileVersion>
```

Scene regeneration:

```text
content.plan:regenerate_scene:<planId>:<baseRevision>:<sceneId>:<instructionHash>:<plannerProfileVersion>
```

Narration regeneration:

```text
content.plan:regenerate_narration:<planId>:<baseRevision>:<sceneId>:<instructionHash>:<plannerProfileVersion>
```

Identical immutable requests converge on one durable job.

---

## 10. Local Content Planning Worker Lifecycle

Phase 4 introduces a dedicated local background worker analogous to the Phase 3 render worker.

Conceptual components:

```text
LocalContentPlanWorker
LocalContentPlanWorkerHandle
start_local_content_plan_worker(...)
```

### 10.1 Startup

During Tauri application setup, after the database, `JobManager`, provider resolver/registry, and content-plan repositories are ready:

```text
start_local_content_plan_worker(...)
```

is called once.

The worker reuses the existing Phase 3-compatible background Tokio runtime strategy rather than creating an unrelated runtime model.

### 10.2 Type-safe polling

The worker loop only acquires:

```text
job_manager.acquire_next_job_for_type("content.plan", 60)
```

Recommended baseline lifecycle constants:

```text
lease duration      = 60 seconds
heartbeat interval  = 15 seconds
idle poll interval  = approximately 500 ms–1 s
```

Exact constants may be centralized/configurable but must preserve enough margin between heartbeat and lease expiration.

### 10.3 Heartbeat and fencing

While processing a provider call, the worker maintains Phase 2 lease-token heartbeats.

Before every checkpoint, revision publication, and terminal completion, the worker must still own the valid lease token.

### 10.4 Shutdown

`start_local_content_plan_worker` returns `LocalContentPlanWorkerHandle` with a cooperative shutdown signal.

The application shutdown path signals the worker and awaits/coordinates clean termination where the existing Tauri lifecycle permits it. No new job may begin after shutdown is requested.

### 10.5 Publication safety

Cancellation is checked:

```text
before provider call
after provider call
before persist_revision
before final completion
```

Critical invariant:

```text
cancelled or stale worker
→ MUST NOT publish ContentPlan revision
```

If the provider HTTP call can be cancelled safely, cancellation should be propagated. If not, a late provider response is discarded and cannot be persisted after cancellation/lease loss.

---

## 11. Provider Configuration and Secrets

### 11.1 Credential source

Phase 4 uses one standard environment variable for Gemini credentials:

```text
GEMINI_API_KEY
```

`ProviderConfigResolver` resolves `providerConfigRef = "gemini-default"` to runtime provider configuration and reads `GEMINI_API_KEY` from the process environment.

If absent:

```text
provider_auth_missing
```

No Phase 4 in-app secret storage UI is required. A future settings/credential subsystem may replace the environment-only mechanism without changing the durable job contract.

### 11.2 Secret exclusion

API keys must never appear in:

```text
job payloads
job results
content_plan_revisions.content_json
content_plan_revisions.generation_metadata_json
domain events
logs
frontend persisted state
error strings exposed to the UI
```

### 11.3 Provider status for UI

The backend may expose a safe provider status command returning only non-secret information such as:

```text
provider
configured: true/false
model/profile label
```

It must never return the credential value.

---

## 12. Persistence — Migration 6

Migrations 1–5 remain immutable.

Phase 4 adds Migration 6.

### 12.1 `content_plans`

```text
id                  TEXT PRIMARY KEY
project_id          TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE
title               TEXT NOT NULL
current_revision    INTEGER NOT NULL DEFAULT 1
status              TEXT NOT NULL DEFAULT 'draft'
approved_revision   INTEGER NULL
approved_at         TEXT NULL
created_at          TEXT NOT NULL
updated_at          TEXT NOT NULL
```

`title` is a current-listing projection/cache of the latest revision's title so project plan lists do not need to deserialize every revision blob.

Whenever a new current revision changes the plan title, `content_plans.title` is updated transactionally with `current_revision`.

### 12.2 `content_plan_revisions`

```text
plan_id                  TEXT NOT NULL REFERENCES content_plans(id) ON DELETE CASCADE
revision                 INTEGER NOT NULL
parent_revision          INTEGER NULL
operation_type           TEXT NOT NULL
content_json             TEXT NOT NULL
generation_metadata_json TEXT NULL
created_by               TEXT NOT NULL
created_at                TEXT NOT NULL
PRIMARY KEY(plan_id, revision)
```

### 12.3 `content_plan_sources`

Source snapshots are revision-scoped to preserve historical reproducibility:

```text
plan_id             TEXT NOT NULL
revision            INTEGER NOT NULL
source_id           TEXT NOT NULL
source_type         TEXT NOT NULL
title               TEXT NOT NULL
author              TEXT NULL
url                 TEXT NULL
published_at        TEXT NULL
locator             TEXT NULL
content_snapshot    TEXT NOT NULL
content_hash        TEXT NOT NULL
created_at          TEXT NOT NULL
PRIMARY KEY(plan_id, revision, source_id)
FOREIGN KEY(plan_id, revision)
    REFERENCES content_plan_revisions(plan_id, revision)
    ON DELETE CASCADE
```

A new revision receives its own immutable source snapshot set. Reusing the same logical `source_id` in a later revision is allowed because `revision` is part of the key.

The implementation must never use `INSERT OR REPLACE` in a way that mutates historical source snapshots.

Recommended indexes:

```text
content_plans(project_id)
content_plan_revisions(plan_id, revision)
content_plan_sources(plan_id, revision)
content_plan_sources(plan_id, source_id)
```

Canonical revision content remains an immutable JSON snapshot in `content_plan_revisions.content_json`.

Scenes and claims are not separate SQL rows in Phase 4.

### 12.4 Migration upgrade proof

Migration tests must construct a representative released v0.4.0 / Migration-5 database containing existing Phase 0–3 rows, apply Migration 6, and prove those rows remain intact.

Running Migrations 1–6 only on a fresh empty database is not sufficient release evidence.

---

## 13. Revision Model

Every durable content change creates a new revision.

Example:

```text
rev 1  generate
rev 2  manual_edit
rev 3  regenerate_scene
rev 4  regenerate_narration
rev 5  manual_edit
```

Invariants:

```text
planId stable
revision strictly monotonic
current_revision points to latest durable revision
old revisions immutable
source snapshots immutable per revision
```

Every manual edit/regeneration uses `expectedRevision` / `baseRevision` optimistic concurrency.

A stale operation returns:

```text
content_plan_revision_conflict
```

No automatic merge is attempted in Phase 4.

---

## 14. Initial Generation and Status

A successful first generation creates transactionally:

```text
content_plans row
revision = 1
status = draft
operation_type = generate
revision-1 source snapshots
```

The plan row and revision/source snapshots must not be partially published.

---

## 15. Approval and Archive Semantics

### 15.1 Status values

```text
draft
approved
archived
```

### 15.2 Approval

Approval is explicit and does not call an AI provider.

Conceptual operation:

```text
approve_content_plan(planId, expectedRevision)
```

Approval validates structural correctness, grounding integrity, and configured duration tolerance.

Blocking issues:

```text
invalid plan structure
dangling source reference
supported claim without valid source support
fatal domain validation error
```

Warnings that do not automatically block approval:

```text
model_knowledge claims
uncertain claims
modified/unvalidated claims if product policy permits warning-only approval
```

For Phase 4, `modified` claims should be surfaced as a strong warning. They do not silently regain `supported` status.

Approval records:

```text
status = approved
approved_revision = current_revision
approved_at = now
```

No new content revision is required if content is unchanged.

### 15.3 Editing an approved plan

The approved historical revision remains immutable.

Editing or regenerating from it creates a new draft revision:

```text
rev 8 = approved
edit/regenerate
rev 9 = draft
```

`approved_revision` may continue pointing to the last approved historical revision while `status` for the current active plan becomes `draft`.

### 15.4 Archive

Archived plans are read-only in normal workflow and excluded from default active-plan selection.

Archiving is a plan-level workflow state change and does not rewrite historical revision content.

---

## 16. Manual Editing

The frontend keeps transient editor state locally.

```text
typing
→ local dirty state
→ Save Revision
→ one durable new revision
```

No durable revision is created per keystroke.

Save requires:

```text
planId
expectedRevision
edited ContentPlan draft
```

Before persistence the backend:

1. applies deterministic grounding invalidation rules,
2. recomputes plan-level derived fields,
3. validates the resulting document,
4. checks optimistic concurrency,
5. inserts the new revision and revision-scoped source snapshots transactionally,
6. updates current plan projection fields (`title`, `current_revision`, `status`, `updated_at`).

A new manual edit after an approved revision creates a new draft revision.

---

## 17. Regeneration Semantics

### 17.1 Full-plan regeneration

```text
base revision N
→ regenerate_plan
→ revision N+1 draft
```

The provider may replace the full content-plan body, subject to schema/domain/grounding validation and source constraints.

### 17.2 Scene regeneration

Only the target scene may be replaced by provider output.

The worker supplies:

```text
overall plan context
previous scene
target scene
next scene
relevant source snapshot
scene duration budget
optional user instruction
```

All non-target scenes must remain domain-equivalent to the base revision.

After replacing the target scene, the backend **always** recomputes:

```text
estimatedDurationMs
fullNarration
plan-level sourceRefs summary
```

before validating/persisting the new revision.

### 17.3 Narration-only regeneration

The provider may modify only:

```text
target scene narration
target scene claims
target scene sourceRefs/grounding
```

It must not change:

```text
scene durationEstimateMs
scene title
visual description
media intent
stock search queries
image prompt
video prompt
scene order
other scenes
```

After narration replacement the backend recomputes `fullNarration`. `estimatedDurationMs` remains derived from unchanged scene durations and is recomputed as a consistency check.

### 17.4 Base-revision conflict

Before publication:

```text
current_revision == payload.baseRevision
```

must hold for regeneration jobs.

If not:

```text
content_plan_revision_conflict
```

The stale generated result is discarded and no new revision is published.

---

## 18. Source Snapshot Reproducibility

Every durable revision owns a revision-scoped immutable source snapshot set in `content_plan_sources`.

For generation, the source snapshot comes from the immutable job payload.

For regeneration:

- if the request provides an updated source context, that exact context becomes the new revision's snapshot;
- otherwise the worker copies the base revision's source snapshots into the new revision.

For manual edits, the current revision's source snapshots are copied forward unchanged unless the operation explicitly supplies a new validated source set.

Historical revision source rows are never updated or replaced.

This ensures a revision can always be audited against the exact context used when it was created.

---

## 19. Generation Metadata and Usage

```ts
interface GenerationMetadata {
  provider: string;
  model: string;
  plannerProfileVersion: number;
  promptTemplateVersion: number;
  generatedAt: string;
  usage: {
    inputTokens: number | null;
    outputTokens: number | null;
    totalTokens: number | null;
    providerReportedCost: number | null;
    durationMs: number | null;
  };
}
```

Do not store raw secret-bearing prompts or unrestricted provider responses in this metadata.

---

## 20. Prompt Construction

`PromptBuilder` prepares structured context for the provider.

Supplied sources use explicit backend-owned logical IDs:

```text
SOURCE src_001
Title: ...
Author: ...
Locator: ...
Content:
...
```

Provider instructions must include equivalent semantics to:

```text
You may cite only source IDs supplied in ALLOWED_SOURCES.
Never invent source IDs, titles, authors, URLs, or locators.
If a factual statement is not supported by supplied sources,
mark it model_knowledge or uncertain.
```

Prompt templates are versioned (`promptTemplateVersion`).

---

## 21. Context and Request Limits

The backend enforces bounded planning inputs.

At minimum configuration covers:

```text
maximum input text length
maximum source count
maximum source snapshot size
maximum combined planning-context size
maximum scene count
maximum target duration
```

If the validated context exceeds the Phase 4 budget:

```text
planning_context_too_large
```

Exact limits should be centralized and selected during implementation using the verified model/context capability rather than scattered magic numbers.

---

## 22. Error Model

Public error categories include:

```text
content_plan_not_found
content_plan_revision_not_found
content_plan_revision_conflict
content_plan_archived
invalid_planning_request
planning_context_too_large
content_plan_validation_failed
source_grounding_invalid
provider_auth_missing
provider_auth_failed
provider_rate_limited
provider_timeout
provider_unavailable
structured_output_invalid
planning_cancelled
stale_lease
content_plan_persist_failed
```

UI-facing errors must not contain raw Rust backtraces, SQL errors, secret values, unrestricted raw provider responses, or full internal prompts.

---

## 23. Tauri IPC Contract

Phase 4 uses typed Tauri commands consistent with existing Phase 3 command registration patterns.

Minimum IPC surface:

### 23.1 Submit initial generation

```text
submit_content_plan_job(
  project_id,
  planning_request,
  provider_config_ref?
) -> Job
```

Submits a `content.plan` job with operation `generate`.

### 23.2 Get current plan

```text
get_content_plan(plan_id) -> ContentPlan
```

Returns the current revision plus safe plan metadata.

### 23.3 List project plans

```text
list_project_content_plans(project_id) -> ContentPlanSummary[]
```

Summary uses relational projection fields such as `id`, `title`, `status`, `current_revision`, and timestamps; it must not require deserializing every revision JSON row.

### 23.4 Save manual revision

```text
save_content_plan_revision(
  plan_id,
  expected_revision,
  content
) -> ContentPlan
```

Applies grounding invalidation, derived-field recomputation, validation, and optimistic concurrency before creating a new draft revision.

### 23.5 Submit regeneration

```text
submit_content_plan_regeneration(
  plan_id,
  base_revision,
  operation,
  scene_id?,
  user_instruction?,
  source_context?
) -> Job
```

`operation` is one of:

```text
regenerate_plan
regenerate_scene
regenerate_narration
```

### 23.6 Approve

```text
approve_content_plan(
  plan_id,
  expected_revision
) -> ContentPlanSummary
```

### 23.7 Archive

```text
archive_content_plan(
  plan_id,
  expected_revision
) -> ContentPlanSummary
```

Archive uses optimistic concurrency so a stale UI cannot archive a newer plan accidentally.

### 23.8 Revision history

```text
get_content_plan_history(plan_id) -> ContentPlanRevisionSummary[]
get_content_plan_revision(plan_id, revision) -> ContentPlan
```

Historical revisions are read-only.

### 23.9 Safe provider status

```text
get_planner_provider_status() -> PlannerProviderStatus
```

Returns only safe configuration state such as provider/model label and `configured: bool`.

Rust parameter naming follows repository conventions; frontend wrappers may map to Tauri's expected camelCase argument keys.

---

## 24. Frontend Planner Workflow

### 24.1 Navigation

Within an opened project, `ProjectDetail` adds an **AI Planner** action alongside the existing timeline/editor workflow.

Main application navigation gains an `AI Planner` view for the active project without removing existing Projects, Editor, Jobs & Queue, or System Health flows.

### 24.2 Plan list

AI Planner starts with the project's content-plan list:

```text
Title
Status
Current revision
Last updated
Open
```

Multiple plans per project are supported.

Archived plans are hidden by default or visually separated.

### 24.3 New-plan form

```text
Topic / Script mode
Input text
Target duration
Language
Tone/style
Optional sources
Provider status: Gemini configured/not configured
Generate
```

Generation returns a durable job ID. The user may navigate away while it runs.

### 24.4 Structured editor

Recommended layout:

```text
┌──────────────────────────────────────────────────────────┐
│ Plan title     Rev 4     Draft       [Approve] [History] │
├───────────────────┬──────────────────────────────────────┤
│ SCENES            │ SCENE EDITOR                         │
│ 1. Intro          │ Narration                            │
│ 2. Background     │ Duration                             │
│ 3. Event          │ Visual description                   │
│ 4. Conclusion     │ Media intent                         │
│                   │ Search queries                       │
│                   │ Image / video prompts                │
│                   │ Claims + citations                   │
└───────────────────┴──────────────────────────────────────┘
```

The full narration aggregate is viewable/exportable and synchronized from scene narration; it is not an independent durable editable source in Phase 4.

### 24.5 Dirty-state semantics

```text
typing
→ local dirty state
→ Save Revision
→ durable revision N+1
```

Navigating away with unsaved changes requires Save/Discard confirmation.

Regeneration cannot start while local unsaved edits are unresolved.

### 24.6 Regeneration UX

Plan level:

```text
Regenerate Entire Plan
```

Scene level:

```text
Regenerate Scene
Regenerate Narration
```

Regeneration may accept a small optional user instruction.

The UI clearly states that regeneration creates a new draft revision and preserves the previous revision.

### 24.7 Revision history

History shows at least:

```text
revision
operation type
created_by
created_at
approval indication where applicable
```

Historical revisions open read-only.

A lightweight field/scene change summary is allowed; a full rich diff engine is not required.

### 24.8 Approval summary

Before approval the UI shows validation information such as:

```text
scene count
estimated duration vs target
source-reference validity
model_knowledge claim count
uncertain claim count
modified/unvalidated claim count
```

Fatal structural/grounding errors block approval. Warnings remain visible.

### 24.9 Citation display

Supported claim:

```text
Supported
Source A — p. 42
```

Model knowledge:

```text
AI knowledge — not grounded in supplied sources
```

Uncertain:

```text
Needs review
```

Modified:

```text
Modified after grounding — citation requires revalidation
```

### 24.10 Jobs integration

Planner UI does not create a second job system.

It links to the existing Jobs & Queue view for the `content.plan` job and its checkpoints.

---

## 25. Domain Events

Phase 4 may persist/broadcast versioned domain events through the existing transactional-event + post-commit broadcast architecture.

Recommended events include:

```text
content_plan.created
content_plan.revision_created
content_plan.approved
content_plan.archived
```

Events contain logical identifiers and safe metadata only. They must not contain API keys, raw prompts, full source snapshots, or unrestricted provider responses.

Revision creation event persistence should occur in the same transaction as the new revision where existing event infrastructure permits it, followed by post-commit broadcast.

---

## 26. Security and Logging

Never log:

```text
GEMINI_API_KEY
full secret-bearing prompt text
full source snapshots by default
unrestricted raw provider responses
canonical private filesystem paths
```

Safe structured logging may include:

```text
jobId
planId
revision
provider
model
operation
stage
attempt
latencyMs
inputTokenCount?
outputTokenCount?
validationResult
errorCode
```

All frontend/provider error messages are sanitized.

---

## 27. Testing Strategy

### 27.1 Migration tests

- fresh Migration 6 schema,
- migration runner idempotency,
- real Migration 5 → 6 upgrade with representative Phase 0–3 data preserved,
- revision-scoped source snapshots permit same logical source ID in different revisions,
- historical snapshots are immutable.

### 27.2 Domain tests

- valid ContentPlan,
- duplicate scene ID/order rejected,
- invalid durations rejected,
- media intent validation,
- `estimatedDurationMs` derived correctly,
- `fullNarration` derived correctly,
- title/narration requirements.

### 27.3 Versioning tests

- first generation creates revision 1 draft,
- manual edit creates revision N+1,
- revision strictly monotonic,
- stale `expectedRevision` rejected,
- approved revision preserved,
- edit/regenerate after approval creates draft revision,
- archive optimistic concurrency and read-only semantics.

### 27.4 Grounding tests

- unknown/fake source ID rejected,
- supported claim without source rejected,
- model knowledge valid without supplied-source reference,
- conflicting source output remains uncertain,
- scene narration manual edit invalidates all scene claims,
- direct claim edit invalidates only that claim,
- non-factual field edit does not invalidate claim grounding.

### 27.5 Provider/repair tests

Using `MockProvider`:

- successful draft/content generation,
- malformed output,
- repair succeeds,
- repair budget exhausted,
- rate limit maps retryable,
- timeout maps retryable,
- auth failure maps non-retryable,
- provider unavailable maps retryable.

### 27.6 Job/worker tests

- type-filtered worker only leases `content.plan`,
- generate queued → running → completed,
- expected checkpoint sequence,
- idempotent double submit,
- heartbeat lease extension,
- cancellation before provider call,
- cancellation after provider call but before persistence,
- stale lease cannot publish a revision,
- revision conflict at publication discards generated result,
- clean worker shutdown.

### 27.7 Regeneration tests

- full plan regeneration creates new draft revision,
- scene regeneration changes only target scene plus derived aggregates,
- narration regeneration changes only allowed target fields plus derived aggregates,
- all unaffected scenes remain domain-equivalent,
- base-revision conflict prevents publication.

### 27.8 IPC tests

Cover every command in Section 23, including argument mapping, optimistic revision behavior, safe provider status, and returned types.

### 27.9 Frontend tests

At minimum:

- AI Planner navigation from active project,
- project plan list,
- Topic/Script mode,
- optional sources,
- Generate job submission,
- provider-not-configured state,
- structured editor load,
- local dirty state,
- Save Revision,
- revision conflict handling,
- regenerate plan/scene/narration,
- history read-only view,
- approval warnings/errors,
- archive flow,
- Jobs & Queue navigation.

### 27.10 Live Gemini acceptance

A live Gemini acceptance test runs only when `GEMINI_API_KEY` is available.

Flow:

```text
simple topic
→ submit content.plan
→ Gemini structured output
→ schema/domain/grounding validation
→ persist revision 1
→ completed
```

Validate:

```text
plan valid
scene count > 0
scene narration non-empty
no invented source IDs
revision persisted
provider/model metadata recorded
```

If the environment variable is absent, the test reports an explicit environment/credential skip. It must never fake a live success.

---

## 28. Runtime Acceptance

Using the actual Tauri desktop application:

```text
1. Open/create project.
2. Open AI Planner.
3. Confirm Gemini configuration status is displayed safely.
4. Enter topic or script.
5. Configure duration/language/tone.
6. Add optional source context.
7. Generate.
8. Confirm content.plan appears in Jobs & Queue.
9. Confirm job progresses through checkpoints.
10. Open generated draft plan.
11. Edit scene narration and save a new revision.
12. Confirm grounding invalidation behavior.
13. Regenerate one scene.
14. Confirm only target scene + derived aggregates changed.
15. Regenerate narration.
16. Confirm visual/media planning fields stayed unchanged.
17. Review claims/citations.
18. Approve plan.
19. Edit/regenerate approved plan and confirm new revision becomes draft while approved historical revision remains.
20. Restart OpenVideoAI.
21. Confirm plans, revisions, sources, approval status, and Jobs data persist.
```

No blank/black WebView window or unhandled frontend console error is acceptable.

---

## 29. Performance and Robustness Guardrails

```text
source snapshots bounded by validated context budget
no AI call on UI thread
no synchronous long-running Tauri command waiting for Gemini generation
JSON snapshots written transactionally
no per-keystroke durable revisions
provider retries bounded
worker heartbeat continues during long provider calls
historical revisions/source snapshots never rewritten
```

---

## 30. Scope Gate

### Included

```text
Topic input
Script input
Optional source context
ContentPlan domain
Full narration aggregate
Scene planning
Media intent
Stock search-query hints
Image/video prompt hints
Claim grounding/citation integrity
Gemini provider
Mock provider
Durable content.plan worker
Migration 6
Versioned revisions
Manual structured editing
Regenerate plan
Regenerate scene
Regenerate narration
Approval/archive
Revision history
AI Planner UI
Jobs & Queue integration
Usage metadata
```

### Explicitly excluded

```text
TTS
Whisper
speech synthesis
automatic captions
stock-media search/download execution
web research engine
autonomous crawling/scraping
AI image generation
AI video generation
ComfyUI
Kaggle
remote workers
automatic timeline insertion
timeline mutation by AI
rendering changes
server mode
multi-user collaboration
rich document editor
billing system
```

Stop rather than silently expanding Phase 4 into excluded functionality.

---

## 31. Release and Migration Discipline

Expected progression:

```text
v0.1.0  Foundation
v0.2.0  Project & Artifact Core
v0.3.0  Durable Jobs & Events
v0.4.0  Media & Timeline Core
v0.5.0  AI Content Planning
```

Before implementation:

1. branch `phase/4-ai-content-planning` must be based on `v0.4.0`,
2. working tree must be clean,
3. Phase 3 quality gates must remain green,
4. Migrations 1–5 must remain immutable,
5. the design spec must be approved after this revision review.

Do not tag `v0.5.0` until external release-close audit authorizes it.

---

## 32. Phase 4 Acceptance Gate

Phase 4 is PASS only if all applicable items are evidenced:

- [ ] Phase 3 base preserved
- [ ] Migration 6 upgrades real Migration-5 DB without data loss
- [ ] `content_plan_sources` snapshots are immutable per revision
- [ ] Plan titles list without parsing all revision JSON
- [ ] ContentPlan generation works from Topic
- [ ] ContentPlan generation works from Script
- [ ] Optional source context works
- [ ] Fake/dangling citations are rejected
- [ ] Supported/model_knowledge/uncertain/modified semantics enforced
- [ ] Manual narration edit invalidates scene claims deterministically
- [ ] Plan-level derived fields remain synchronized
- [ ] Manual save creates a new revision
- [ ] Revision conflicts enforced
- [ ] Approval semantics work
- [ ] Approved historical revision preserved after edit/regeneration
- [ ] Archive semantics work
- [ ] Regenerate full plan works
- [ ] Regenerate scene is isolated
- [ ] Regenerate narration is isolated
- [ ] MockProvider deterministic
- [ ] Bounded structured-output repair works
- [ ] `jobs.job_type` strictly `content.plan` for all planning operations
- [ ] Type-filtered AI worker lifecycle works
- [ ] Cancellation prevents revision publication
- [ ] Stale lease prevents revision publication
- [ ] API key absent from jobs/revisions/events/logs/frontend state
- [ ] `GEMINI_API_KEY` missing state handled safely
- [ ] AI Planner project navigation works
- [ ] Tauri IPC commands covered by tests
- [ ] Phase 0–3 regression tests remain green
- [ ] TypeScript check PASS
- [ ] Frontend tests PASS
- [ ] Frontend production build PASS
- [ ] Rust fmt PASS
- [ ] Rust clippy `-D warnings` PASS
- [ ] Rust tests PASS
- [ ] `scripts/verify.ps1` PASS
- [ ] Live Gemini acceptance PASS or explicit credential/environment skip
- [ ] Real Tauri desktop acceptance PASS
- [ ] Restart persistence PASS
- [ ] No Phase 5+ functionality slipped in
- [ ] Final working tree clean

---

## 33. Design Approval and Audit Record

The user approved the architectural design sections covering:

1. Topic/Script → canonical ContentPlan input model,
2. complete narration + scene planning without media execution,
3. provider abstraction with Gemini first and deterministic MockProvider,
4. durable `content.plan` jobs,
5. versioned revisions and draft/approved/archived states,
6. granular plan/scene/narration regeneration,
7. structured Planner UI,
8. optional source context,
9. claim-level grounding and citation integrity,
10. media intent/search/prompt planning,
11. security, cancellation, lease fencing, testing, and release gates.

A repository-aware design-spec audit then required corrections before implementation planning. This revision resolves those findings by explicitly defining:

- revision-scoped `content_plan_sources` primary key and foreign key,
- `content_plans.title` plus safe defaults,
- deterministic plan-level derived fields,
- deterministic manual grounding invalidation,
- one strict `content.plan` job type,
- `LocalContentPlanWorker` startup/polling/heartbeat/shutdown lifecycle,
- concrete Tauri IPC surface,
- project-level AI Planner navigation and multi-plan listing,
- `GEMINI_API_KEY` credential source,
- planned `reqwest` transport dependency without inventing Gemini SDK methods.

This document is the authoritative Phase 4 design pending final review approval. Implementation planning must not begin until that approval is given.

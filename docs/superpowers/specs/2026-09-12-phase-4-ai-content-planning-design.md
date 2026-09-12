# OpenVideoAI Phase 4 — AI Content Planning Design

**Date:** 2026-09-12  
**Target version:** v0.5.0  
**Target branch:** `phase/4-ai-content-planning`  
**Repository:** `D:\OpenVideoAI`  
**Base:** `v0.4.0` / commit `0f08683`  
**Classification:** Architectural subsystem addition

---

## 1. Goal

Phase 4 introduces the AI Content Planning layer for OpenVideoAI.

The product must accept either a topic or an existing script and produce a versioned, editable, source-aware `ContentPlan` containing full narration, scene structure, duration estimates, media intent, search queries, image/video prompt hints, claims, and grounding metadata.

Phase 4 does **not** create or download media, generate speech, modify timelines, or render video. It establishes a stable planning contract for later Topic → Video phases.

Primary user flow:

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

Phase 4 follows these principles:

1. **Provider-agnostic core.** Planning logic must not depend directly on Gemini-specific transport or response formats.
2. **Durable-by-default.** AI generation runs through the Phase 2 durable Job Manager rather than blocking frontend IPC requests.
3. **Versioned content.** Every durable edit or regeneration creates a new immutable content revision.
4. **Grounding-aware.** Supplied sources and AI-added model knowledge are distinguishable.
5. **No fake citations.** Providers may only cite logical source IDs supplied by the backend.
6. **Deterministic tests.** `MockProvider` is the primary provider for automated tests.
7. **Secrets remain outside durable payloads.** API keys never enter job payloads, content revisions, events, or logs.
8. **No direct timeline coupling.** Content planning cannot directly mutate timeline state.
9. **Scope discipline.** TTS, Whisper, stock execution, web research, image/video generation, and timeline insertion remain outside Phase 4.

---

## 3. Approved Approach

### 3.1 Input Modes

Phase 4 supports two user inputs:

```text
Topic
Script
```

Both are normalized into one `PlanningRequest` and produce the same canonical `ContentPlan` domain model.

### 3.2 Planning Strategy

Use a **two-stage planner inside one durable job**.

Stage 1 produces a structured planning draft:

```text
outline
scene structure
duration allocation
high-level narrative arc
```

Stage 2 expands that draft into the complete `ContentPlan`:

```text
full narration
scene narration
visual descriptions
media intent
stock search queries
image prompt hints
video prompt hints
claims
source grounding
```

This is intentionally not a multi-agent architecture. Phase 4 should remain understandable, testable, and operationally simple.

---

## 4. Core Domain Model

### 4.1 PlanningRequest

Conceptual contract:

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
request remains within configured planning-context limits
```

### 4.2 ContentPlan

Canonical conceptual contract:

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
  estimatedDurationMs: number;
  fullNarration: string;
  scenes: ContentScene[];
  sourceRefs: SourceRef[];
  generationMetadata: GenerationMetadata | null;
  createdAt: string;
  updatedAt: string;
}
```

### 4.3 Scene

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

### 4.4 MediaIntent

Supported Phase 4 planning values:

```text
stock_video
stock_image
ai_image
ai_video
screen_recording
text_graphic
```

These values express planning intent only. Phase 4 does not execute any media acquisition or generation.

### 4.5 Claim

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

Grounding status:

```text
supported
model_knowledge
uncertain
modified
```

Semantics:

- `supported`: the claim is supported by one or more supplied sources and must contain at least one valid `sourceRef`.
- `model_knowledge`: the claim came from model knowledge outside supplied sources; source references may be empty.
- `uncertain`: evidence is weak, conflicting, or otherwise insufficient for a confident statement.
- `modified`: the user changed previously grounded content; historical provenance may remain, but `groundingValid = false` until revalidated.

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

### 5.3 Citation Integrity Rules

The provider may only return `sourceId` values supplied in `PlanningContext`.

The provider must never invent:

```text
source IDs
URLs
authors
titles
bibliographic entries
```

Backend validation rejects references to unknown source IDs with `source_grounding_invalid`.

A `supported` claim without a valid source reference is invalid.

A `model_knowledge` claim must not masquerade as source-grounded.

If supplied sources disagree, the plan must preserve that uncertainty rather than silently picking one source as truth.

Scene-level `sourceRefs` are convenience summaries derived from claim-level grounding, not the canonical grounding source of truth.

---

## 6. Provider Architecture

### 6.1 Provider Boundary

```text
ContentPlanner
     ↓
AIProvider
├─ GeminiProvider
└─ MockProvider
```

Conceptual provider interface:

```text
generatePlanningDraft(request, context)
generateContentPlan(draft, context)
regeneratePlan(...)
regenerateScene(...)
regenerateNarration(...)
```

The exact Rust interface may follow existing repository conventions, but public semantics must remain equivalent.

### 6.2 Provider Responsibilities

Provider responsibilities:

- construct provider-compatible request bodies from prepared prompts,
- request structured output,
- deserialize provider responses,
- return provider usage metadata when available,
- normalize provider transport errors into internal error categories.

Provider non-responsibilities:

- writing SQLite,
- incrementing revisions,
- approving plans,
- submitting jobs,
- mutating timelines,
- resolving Artifact filesystem paths,
- logging API keys.

### 6.3 GeminiProvider

Gemini is the first real provider in Phase 4.

The Gemini implementation should use structured/schema-constrained output where supported by the selected Gemini API/model.

Backend validation remains authoritative even when Gemini returns schema-valid JSON.

### 6.4 MockProvider

`MockProvider` must be deterministic and support configurable outcomes for tests:

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

Automated tests must not require network access or consume live AI quota.

---

## 7. Structured Output and Validation

Structured output undergoes two layers of validation.

### 7.1 Schema Validation

Validate required types and structural fields.

### 7.2 Domain Validation

At minimum:

```text
scene IDs unique
scene order unique and deterministic
durationEstimateMs > 0
estimatedDurationMs > 0
estimated duration within configured tolerance of target duration
mediaIntent values valid
scene narration non-empty
plan title non-empty
supported claims have valid source refs
all source refs resolve to supplied sources
no duplicate claim IDs
no dangling source references
```

Recommended target-duration planning tolerance is approximately ±15% for AI planning output. This is planning validation, not render timing accuracy.

---

## 8. Repair and Retry Policy

Provider output repair is bounded.

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

Maximum: three model calls per stage unless implementation constraints justify a stricter limit.

No infinite structured-output repair loop is allowed.

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
→ bounded repair inside stage

content_plan_validation_failed
source_grounding_invalid
→ fail once repair budget is exhausted
```

---

## 9. Durable Job Architecture

### 9.1 Job Types / Operations

Phase 4 uses the existing durable Job Manager.

Supported planning operations:

```text
generate
regenerate_plan
regenerate_scene
regenerate_narration
```

The implementation may use one job type (`content.plan`) with an operation field, or dedicated compatible subtypes, provided Jobs & Queue can represent them consistently.

### 9.2 Immutable Job Payload

Conceptual payload:

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
  "provider": "gemini",
  "providerConfigRef": "gemini-default",
  "plannerProfileVersion": 1
}
```

The durable payload must not contain API keys.

The worker operates on the immutable payload snapshot and must not silently rebuild generation context from newer mutable UI state.

### 9.3 Checkpoints

Generate flow:

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

Regenerate scene flow:

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

Conceptual generate identity:

```text
content.plan:<projectId>:<inputHash>:<contextHash>:<plannerProfileVersion>
```

Conceptual scene regeneration identity:

```text
content.regenerate.scene:<planId>:<baseRevision>:<sceneId>:<instructionHash>:<plannerProfileVersion>
```

Double-clicking Generate or Regenerate must converge on one durable job for an identical immutable request.

---

## 10. Cancellation, Lease Fencing, and Publication Safety

AI planning workers must preserve Phase 2 lease-token fencing.

Cancellation is checked:

```text
before provider call
after provider call
before persist_revision
before final completion
```

If the underlying provider call supports cancellation, the worker should propagate cancellation where practical.

Critical invariant:

```text
cancelled or stale worker
→ MUST NOT publish ContentPlan revision
```

Before persisting a generated revision, the worker must still own a valid lease token and the job must not be cancelled.

Before final completion, the worker must still satisfy Phase 2 fenced terminal-state semantics.

---

## 11. Persistence — Migration 6

Migrations 1–5 remain immutable.

Phase 4 introduces Migration 6.

### 11.1 `content_plans`

Conceptual schema:

```text
id                  TEXT PRIMARY KEY
project_id          TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE
current_revision    INTEGER NOT NULL
status              TEXT NOT NULL
approved_revision   INTEGER NULL
approved_at         TEXT NULL
created_at          TEXT NOT NULL
updated_at          TEXT NOT NULL
```

### 11.2 `content_plan_revisions`

```text
plan_id                  TEXT NOT NULL REFERENCES content_plans(id) ON DELETE CASCADE
revision                 INTEGER NOT NULL
parent_revision          INTEGER NULL
operation_type           TEXT NOT NULL
content_json             TEXT NOT NULL
generation_metadata_json TEXT NULL
created_by               TEXT NOT NULL
created_at               TEXT NOT NULL
PRIMARY KEY(plan_id, revision)
```

### 11.3 `content_plan_sources`

```text
plan_id             TEXT NOT NULL REFERENCES content_plans(id) ON DELETE CASCADE
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
PRIMARY KEY(plan_id, source_id)
```

Recommended indexes:

```text
content_plans(project_id)
content_plan_revisions(plan_id, revision)
content_plan_sources(plan_id)
```

Canonical plan revision content remains an immutable JSON snapshot in `content_plan_revisions.content_json`.

Scenes and claims are **not** separate SQL rows in Phase 4.

---

## 12. Revision Model

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
planId remains stable
revision is strictly monotonic
currentRevision points to latest durable revision
old revisions are immutable
```

Every edit/regeneration operation includes `expectedRevision` or equivalent `baseRevision` optimistic concurrency protection.

A stale mutation returns:

```text
content_plan_revision_conflict
```

No automatic merge is attempted in Phase 4.

---

## 13. Initial Generation and Status

A successfully generated plan creates:

```text
revision = 1
status = draft
operation_type = generate
```

`draft` is the default active state.

---

## 14. Approval and Archive Semantics

### 14.1 Status Values

```text
draft
approved
archived
```

### 14.2 Approval

Approval is explicit and does not call the AI provider.

Conceptual operation:

```text
approve_content_plan(planId, expectedRevision)
```

Approval validates structural correctness, grounding integrity, and configured duration tolerance.

Blocking approval issues:

```text
invalid plan structure
dangling source reference
supported claim without source support
fatal domain validation error
```

Warnings that do not automatically block approval:

```text
model_knowledge claims
uncertain claims
```

Approval marks the current revision as approved and records approval metadata without creating a new content revision when no content changes.

### 14.3 Editing an Approved Plan

An approved revision remains immutable historical evidence.

Editing or regenerating from an approved revision creates a new `draft` revision.

Example:

```text
rev 8 = approved
edit/regenerate
rev 9 = draft
```

### 14.4 Archive

Archived plans are read-only in normal workflow and are not selected as active plans.

---

## 15. Manual Editing

The structured editor supports manual durable editing of fields such as:

```text
title
summary
tone
fullNarration
scene.title
scene.narration
scene.durationEstimateMs
scene.visualDescription
scene.mediaIntent
scene.stockSearchQueries
scene.imagePrompt
scene.videoPrompt
```

Frontend typing stays local until explicit Save Revision.

```text
typing
→ local dirty state
→ Save Revision
→ durable revision N+1
```

Do not create a revision per keystroke.

If a user manually changes previously grounded factual text, its grounding must not remain silently valid.

Recommended behavior:

```text
groundingStatus = modified
groundingValid = false
historical sourceRefs may be retained for provenance
```

---

## 16. Regeneration Semantics

### 16.1 Full Plan Regeneration

Full regeneration may replace:

```text
title
summary
full narration
scene structure
scene narration
visual planning
claims
grounding
```

It creates a new draft revision and preserves the previous revision.

### 16.2 Scene Regeneration

Scene regeneration receives:

```text
base plan summary
previous scene
target scene
next scene
relevant source context
scene duration budget
optional user instruction
```

Only the target scene may change.

All non-target scenes must remain domain-equivalent to the base revision.

### 16.3 Narration-Only Regeneration

Narration regeneration may modify only:

```text
scene.narration
scene.claims
scene.sourceRefs / grounding metadata associated with narration
```

It must not silently modify:

```text
scene ordering
durationEstimateMs
visualDescription
mediaIntent
stockSearchQueries
imagePrompt
videoPrompt
```

---

## 17. Revision Conflict at Job Completion

A regeneration job is tied to its `baseRevision`.

Before publication:

```text
currentRevision == baseRevision
```

must still hold for regeneration operations.

If a newer revision appeared while the AI job was running, publication fails with:

```text
content_plan_revision_conflict
```

The stale AI result is not persisted as a newer revision over the user's work.

---

## 18. Source Snapshot Reproducibility

Generation jobs use immutable source-context snapshots.

The plan revision records references to the source snapshot used for generation.

If an external URL changes later, the historical revision must remain attributable to the context snapshot used at generation time.

Phase 4 does not attempt to become a document-management or web-crawling system.

---

## 19. Prompt Construction

Prompt construction is a dedicated responsibility, separate from provider transport.

Conceptual components:

```text
ContentPlanner
├─ PromptBuilder
├─ AIProvider
├─ ContentPlanValidator
├─ GroundingValidator
└─ ContentPlanRepository
```

Structured source context should be rendered with explicit logical IDs, for example:

```text
SOURCE src_001
Title: ...
Author: ...
Locator: ...
Content:
...

SOURCE src_002
...
```

Provider instructions must state that only supplied source IDs are valid citations.

---

## 20. Generation Metadata and Usage

Revision generation metadata should capture reproducibility and usage information where available:

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

Provider cost is optional and nullable.

Phase 4 does not implement a billing subsystem.

---

## 21. Provider Configuration and Secrets

Durable payloads store a provider reference, not credentials.

```text
provider = gemini
providerConfigRef = gemini-default
model = configured/default planning model
```

At runtime:

```text
AI worker
→ ProviderConfigResolver
→ secure local configuration / environment
→ Gemini credential
```

API keys must never be persisted in:

```text
job payloads
job results
domain events
content plan revisions
frontend persisted state
normal logs
```

Missing credentials return a typed safe error such as:

```text
provider_auth_missing
```

---

## 22. Logging and Observability

Safe log fields include:

```text
jobId
planId
revision
provider
model
stage
attempt
latencyMs
inputTokenCount
outputTokenCount
validation result
error code
```

Normal logs must not include:

```text
API keys
raw secrets
unrestricted full source context
unsanitized provider responses
canonical filesystem paths
```

---

## 23. Planning Context Limits

Phase 4 must enforce configurable planning-context limits.

Limits cover:

```text
inputText length
source count
total source context size
scene count
target duration
```

The exact numerical limits should be chosen during implementation based on actual provider/model constraints and product usability, not guessed in this design document.

Requests exceeding configured limits return:

```text
planning_context_too_large
```

A reasonable default maximum scene count may be approximately 50, but implementation should make this configurable.

---

## 24. Error Model

Public/internal typed error categories include:

```text
provider_auth_missing
provider_auth_failed
provider_rate_limited
provider_timeout
provider_unavailable
structured_output_invalid
content_plan_validation_failed
source_grounding_invalid
planning_context_too_large
content_plan_revision_conflict
content_plan_not_found
content_plan_archived
stale_lease
job_cancelled
```

The frontend must not receive raw Rust stack traces, unrestricted provider bodies, API keys, or internal prompts.

---

## 25. Planner UI

Phase 4 adds an **AI Planner** application area.

Primary flow:

```text
Project
→ AI Planner
→ Topic / Script
→ target duration
→ language
→ tone
→ optional sources
→ Generate
→ durable job
→ Content Plan Editor
```

### 25.1 Planner Input

Required controls:

```text
Topic / Script mode
input text
target duration
language
tone/style
optional source context
provider: Gemini
Generate
```

Phase 4 does not need a model marketplace or large provider selector UI.

### 25.2 Content Plan Editor

Conceptual layout:

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

Also expose plan-level:

```text
summary
full narration
target duration
estimated duration
language
tone
```

### 25.3 Dirty State

Unsaved manual edits stay local.

When navigating away from dirty state, UI must prompt Save or Discard.

Regeneration must not silently discard unsaved local edits.

### 25.4 Revision History

Display operation lineage such as:

```text
Rev 1  AI Generate
Rev 2  Manual Edit
Rev 3  Regenerate Scene 4
Rev 4  Approved
Rev 5  Regenerate Narration
```

Historical revisions open read-only.

A simple field/scene-level comparison is sufficient for Phase 4. A rich diff engine is not required.

### 25.5 Regeneration UX

Controls:

```text
Regenerate Entire Plan
Regenerate Scene
Regenerate Narration
```

Scene/narration regeneration may accept an optional natural-language instruction.

The UI must explain that regeneration creates a new draft revision and preserves the previous revision.

### 25.6 Approval UX

Before approval, show validation summary such as:

```text
scene count
target vs estimated duration
citation integrity
model-knowledge claim count
uncertain claim count
```

Warnings may remain approvable; structural/grounding errors block approval.

### 25.7 Citation UX

Example claim display:

```text
Claim: "..."
Status: Supported
Sources:
- Source A — p. 42
```

`model_knowledge` should clearly state that the claim is not grounded in supplied sources.

`uncertain` should display a review warning.

`modified` should display that previous grounding requires revalidation.

---

## 26. Jobs & Queue Integration

Do not create a second job UI.

AI planning jobs appear in the existing Jobs & Queue experience.

The Planner UI may provide a `View Job` shortcut.

User-facing stage labels may map to internal checkpoints, for example:

```text
Queued
Preparing context
Planning outline
Expanding content
Validating
Saving revision
Completed
```

---

## 27. Testing Strategy

### 27.1 Migration Tests

Required:

- Migration 6 applies cleanly.
- Migration 6 runner remains idempotent.
- A real Migration-5 / v0.4.0 database upgrades to Migration 6 without losing Phase 0–3 data.
- Projects, artifacts, media metadata, timeline data, history, jobs, and events remain valid after upgrade.

### 27.2 Domain Tests

At minimum:

- valid `ContentPlan`,
- invalid/duplicate scene order,
- invalid scene duration,
- media-intent validation,
- target-duration tolerance,
- unique scene IDs,
- unique claim IDs.

### 27.3 Grounding Tests

At minimum:

- unknown source ID rejected,
- `supported` claim without source rejected,
- `model_knowledge` with no source accepted,
- modified claim invalidates grounding,
- dangling references rejected,
- conflicting source representation can produce `uncertain`.

### 27.4 Repository and Revision Tests

At minimum:

- create revision 1,
- manual edit creates revision 2,
- old revision immutable,
- stale `expectedRevision` rejected,
- approved revision preserved,
- editing approved revision creates draft revision,
- archive semantics,
- revision lineage persisted.

### 27.5 Regeneration Tests

At minimum:

- full plan regeneration creates a new revision,
- scene regeneration changes only target scene,
- narration regeneration changes only narration/claims/grounding fields,
- unrelated scenes remain equivalent,
- regeneration against stale base revision fails safely.

### 27.6 Provider Tests

Using `MockProvider`:

```text
success
malformed output
repair success
repair exhausted
rate limit
timeout
auth failure
provider unavailable
```

### 27.7 Job Integration Tests

At minimum:

- queued → running → completed,
- checkpoint order,
- retryable transient provider failure,
- idempotent submission,
- cancellation before provider call,
- cancellation after provider response but before persistence,
- stale lease cannot persist revision,
- stale lease cannot complete job,
- concurrent newer revision blocks stale regeneration publication.

### 27.8 Frontend Tests

At minimum:

- Topic/Script input modes,
- generate submission,
- job navigation,
- plan load,
- scene selection,
- manual dirty state,
- Save Revision,
- revision conflict handling,
- regenerate plan,
- regenerate scene,
- regenerate narration,
- approval warnings/errors,
- revision history view,
- citation status rendering.

### 27.9 Live Gemini Acceptance

A live Gemini acceptance test runs only when valid credentials are available.

Flow:

```text
simple topic
→ submit content.plan
→ Gemini structured output
→ validation
→ grounding validation
→ persist revision
→ completed
```

Validate:

```text
plan valid
scene count > 0
narration non-empty
no invented source IDs
revision persisted
generation metadata recorded
```

If credentials are unavailable, use an explicit environment skip. Do not fake a live-provider pass.

---

## 28. Desktop Runtime Acceptance

Phase 4 closure requires real Tauri runtime smoke:

```text
1. Launch OpenVideoAI.
2. Open/create project.
3. Open AI Planner.
4. Generate from a topic using MockProvider or configured Gemini.
5. Observe durable planning job in Jobs & Queue.
6. Open resulting ContentPlan.
7. Edit a scene and Save Revision.
8. Regenerate one scene.
9. Regenerate narration for one scene.
10. Inspect citation/grounding states.
11. Approve a valid revision.
12. Edit/regenerate from approved revision and confirm new draft revision.
13. Close application.
14. Relaunch application.
15. Confirm plans, revisions, sources, approval state, and history persist.
```

No blank/black window or unhandled WebView errors are acceptable.

---

## 29. Security and Robustness Rules

1. API keys never enter durable payloads or logs.
2. Providers may cite only source IDs supplied by backend context.
3. Unsupported provider-generated source IDs are rejected.
4. Provider transport is separated from domain validation.
5. Cancellation/stale lease cannot publish a revision.
6. Optimistic revision control prevents stale AI results from overwriting newer user work.
7. Source context is bounded by configured limits.
8. Raw provider response bodies are not exposed directly to normal frontend errors.
9. Historical revisions remain immutable.
10. Phase 0–3 persistence contracts remain preserved.

---

## 30. Scope Gate

### Included

```text
AI Content Planner
Topic input
Script input
Optional source context
Two-stage planning
Gemini provider
Deterministic MockProvider
Structured output validation
ContentPlan v1 domain
Full narration
Scene planning
Media intent planning
Stock search query hints
Image/video prompt hints
Claim/source grounding
Fake-citation prevention
Durable content.plan jobs
Versioned ContentPlans
Manual editing
Regenerate whole plan
Regenerate scene
Regenerate narration
Approval/archive semantics
Revision history
Planner UI
Jobs & Queue integration
Provider usage metadata
```

### Explicitly excluded

```text
TTS
Whisper / transcription
automatic captions
stock media search/download execution
web research engine
web crawling
AI image generation
AI video generation
Veo execution
ComfyUI
Kaggle
remote workers
timeline insertion
automatic timeline generation
rendering changes
Activepieces
server mode
collaboration
multi-user comments
rich text document editor
full academic citation formatter
```

Stop rather than silently expanding into excluded functionality.

---

## 31. Migration and Release Discipline

Expected release progression:

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
4. Migrations 1–5 must remain immutable.

Do not tag `v0.5.0` until external release-close audit authorizes it.

---

## 32. Phase 4 Acceptance Gate

Phase 4 is PASS only if all applicable items are evidenced:

- [ ] Branch is based on `v0.4.0`
- [ ] Phase 0–3 regressions remain green
- [ ] Migration 6 upgrades real Migration-5 DB without data loss
- [ ] Migrations 1–5 remain unchanged
- [ ] Topic input works
- [ ] Script input works
- [ ] Optional source context works
- [ ] Two-stage planning works
- [ ] Structured output validation works
- [ ] Bounded repair policy works
- [ ] `MockProvider` is deterministic
- [ ] Gemini provider integration works
- [ ] No API key persists in jobs/revisions/events/logs
- [ ] `ContentPlan` revision 1 persists
- [ ] Manual edit creates new revision
- [ ] Revisions are immutable and monotonic
- [ ] Stale revision write rejected
- [ ] Approved revision preserved
- [ ] Editing approved revision creates draft revision
- [ ] Archive semantics work
- [ ] Full-plan regeneration works
- [ ] Scene regeneration changes only target scene
- [ ] Narration regeneration changes only allowed fields
- [ ] Fake source IDs rejected
- [ ] Supported claims require valid source refs
- [ ] Model-knowledge claims are distinguishable
- [ ] Modified claims invalidate grounding
- [ ] Source conflict can be represented as uncertain
- [ ] Durable planning jobs use idempotency
- [ ] Planning jobs expose checkpoints
- [ ] Cancellation prevents revision publication
- [ ] Stale lease prevents revision publication
- [ ] Stale base revision blocks regeneration publication
- [ ] Planner UI supports structured editing
- [ ] Planner UI supports history
- [ ] Planner UI supports regeneration controls
- [ ] Planner UI supports approval/archive
- [ ] Citation/grounding states are visible
- [ ] Frontend TypeScript check PASS
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

## 33. Design Approval Record

The user approved the following design decisions during the Phase 4 brainstorming process:

1. both Topic and Script are supported inputs,
2. both normalize into one canonical `ContentPlan`,
3. Phase 4 produces full narration and complete scene planning but no media execution,
4. provider abstraction is introduced with Gemini as the first real provider and deterministic MockProvider for tests,
5. AI planning uses durable jobs rather than synchronous frontend blocking,
6. ContentPlans are versioned and support draft/approved/archived states,
7. regeneration is supported at whole-plan, scene, and narration scope,
8. the Planner UI is a structured editable editor rather than read-only JSON,
9. optional source context is supported without a web-research engine,
10. claims support source-level grounding and citation provenance,
11. scenes carry structured media intent, search queries, and image/video prompt hints,
12. the planner uses a two-stage generation architecture,
13. revisions are immutable JSON snapshots with relational plan/source metadata,
14. fake citations are rejected through backend source-ID validation,
15. approval preserves historical approved revisions,
16. AI workers use cancellation, lease fencing, and optimistic revision publication checks,
17. Phase 4 uses Migration 6 and leaves Migrations 1–5 immutable,
18. Phase 4 does not include TTS, Whisper, stock execution, image/video generation, timeline insertion, web research, ComfyUI, Kaggle, or server mode.

This document is the formal written design for OpenVideoAI Phase 4 — AI Content Planning.
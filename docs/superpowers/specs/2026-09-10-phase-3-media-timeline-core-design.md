# OpenVideoAI Phase 3 — Media & Timeline Core Design

**Date:** 2026-09-10  
**Target version:** v0.4.0  
**Target branch:** `phase/3-media-timeline-core`  
**Repository:** `D:\OpenVideoAI`  
**Expected base:** verified Phase 2 release state at/after `f8438a4`; if `v0.3.0` has not yet been created, verify the Phase 2 release-close smoke and clean working tree before tagging or branching.

---

## 1. Classification

Phase 3 is an **architectural subsystem addition**.

It introduces durable timeline state, typed editing mutations, optimistic revision control, bounded persistent undo/redo, secure preview delivery, immutable RenderSpec snapshots, an FFmpeg compiler/runner boundary, and a local render worker integrated with the existing Phase 2 durable Job Manager.

The design must preserve the boundaries created in earlier phases:

```text
Phase 1 Artifact Core
        │
        ▼
Phase 3 Timeline / RenderSpec
        │
        ▼
Phase 2 Durable Job Manager
        │
        ▼
Local FFmpeg Renderer
        │
        ▼
Phase 1 Artifact Core
```

---

## 2. Product Goal

Phase 3 turns OpenVideoAI from infrastructure into the first usable non-destructive video editor vertical slice.

A user must be able to:

```text
Open Project
→ Create Timeline
→ Add imported video
→ Trim
→ Split
→ Move
→ Adjust volume / mute
→ Undo / Redo
→ Render
→ Observe render job
→ Receive output ArtifactRef
→ Restart application
→ Continue with the same timeline
```

The internal model is multi-track from the beginning, while the Phase 3 UI intentionally remains minimal.

---

## 3. Approved Approach

### Multi-track internal model + minimal Phase 3 editor UI

The internal timeline supports multiple video and audio tracks. This avoids a destructive schema redesign when overlays, captions, AI edits, or richer compositing arrive later.

The UI does **not** attempt to become a full CapCut/Premiere clone in Phase 3.

```text
Timeline
├── Video Track 0
│   ├── Clip A
│   └── Clip B
├── Video Track 1
└── Audio Track 0
    └── Music Clip
```

### Same-track overlap rule

Phase 3 forbids overlapping clips on the same track.

```text
V1 [ Clip A ][ Clip B ]            VALID

V1 [ Clip A------ ]
        [ Clip B------ ]            INVALID

V2        [ Clip B------ ]
V1 [ Clip A------ ]                 VALID
```

Cross-track overlap is valid.

This keeps FFmpeg compilation deterministic while preserving the multi-track architecture.

---

## 4. Core Architectural Boundaries

```text
Frontend Editor
      │
      │ typed operations
      ▼
TimelineService
      │
      ├── validation
      ├── revision check
      ├── transactional mutation
      └── history maintenance
      │
      ▼
SQLite Timeline State
      │
      ▼
TimelineDocument revision N
      │
      ▼
RenderSpecBuilder
      │
      ▼
Immutable RenderSpec v1
      │
      ▼
FFmpegCompiler
      │
      ▼
FfmpegExecutionPlan
      │
      ▼
Phase 2 Job Manager
      │
      ▼
Local Render Worker
      │
      ▼
FFmpeg Process Runner
      │
      ▼
Temporary Render Output
      │
      ▼
Artifact Import
      │
      ▼
ArtifactRef
```

The frontend never constructs raw FFmpeg commands. The timeline never stores physical media paths. The renderer resolves physical storage only inside the trusted backend through the existing ArtifactResolver.

---

## 5. Timeline Domain Model

### TimelineDocument

```ts
interface TimelineDocument {
  id: string;
  projectId: string;
  name: string;
  revision: number;
  canvas: {
    width: number;
    height: number;
    frameRate: number;
  };
  durationMs: number; // derived, not canonical persisted state
  tracks: TimelineTrack[];
  createdAt: string;
  updatedAt: string;
}
```

`durationMs` is derived from the maximum clip end time and must not become an independently mutable source of truth.

### TimelineTrack

```ts
type TimelineTrackKind = "video" | "audio";

interface TimelineTrack {
  id: string;
  timelineId: string;
  kind: TimelineTrackKind;
  order: number;
  name: string;
  muted: boolean;
  clips: TimelineClip[];
}
```

Track order semantics:

```text
order 0 = bottom
order 1 = above order 0
order 2 = above order 1
```

For audio, order establishes deterministic mixing order but does not imply exclusive playback.

### TimelineClip

```ts
interface TimelineClip {
  id: string;
  trackId: string;
  artifactId: string;
  sourceInMs: number;
  sourceOutMs: number;
  timelineStartMs: number;
  volume: number;
  muted: boolean;
  transform: {
    x: number;
    y: number;
    scale: number;
    rotationDeg: number;
    opacity: number;
  };
}
```

Phase 3 stores transform fields to establish a stable model boundary. The UI does not need to expose every transform control yet.

### Supported source kinds in Phase 3

- A `video` track accepts video artifacts.
- An `audio` track accepts audio artifacts.
- A video clip may contribute its embedded audio during final render unless muted.
- Image timeline clips are deferred to a later phase.
- Subtitle and JSON artifacts are not editable timeline clips in Phase 3.
- The asset bin may display all project artifacts even when a type is not yet insertable into the timeline.

---

## 6. Time Representation and Clip Invariants

Canonical editing time is integer milliseconds.

```text
sourceInMs
sourceOutMs
timelineStartMs
```

Required invariants:

```text
sourceInMs >= 0
sourceOutMs > sourceInMs
timelineStartMs >= 0
sourceOutMs <= source media duration
volume ∈ [0.0, 2.0]
opacity ∈ [0.0, 1.0]
scale > 0
```

When source duration is required but unavailable, the mutation fails with `media_duration_unknown`. Do not silently guess media duration.

```text
clipDurationMs = sourceOutMs - sourceInMs
timelineEndMs = timelineStartMs + clipDurationMs
```

Split example:

```text
sourceIn      = 2s
sourceOut     = 14s
timelineStart = 5s
split at timeline 10s

relative offset = 5s
source split    = 7s

Clip A: source 2s–7s,  timeline 5s–10s
Clip B: source 7s–14s, timeline 10s–17s
```

Splitting never creates or modifies media files.

---

## 7. Persistence — Migration 5

Migration 5 introduces the Phase 3 timeline schema. Previously released migrations remain immutable. If later corrections require schema additions after Migration 5 has been applied, create Migration 6 or higher.

### `timelines`

```text
id                  TEXT PRIMARY KEY
project_id          TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE
name                TEXT NOT NULL
revision            INTEGER NOT NULL
width               INTEGER NOT NULL
height              INTEGER NOT NULL
frame_rate          REAL NOT NULL
history_cursor      INTEGER NULL
created_at          TEXT NOT NULL
updated_at          TEXT NOT NULL
```

`revision` starts at 1 and is monotonically increasing. `history_cursor` identifies the currently selected history snapshot and is independent from the monotonic revision.

### `timeline_tracks`

```text
id                  TEXT PRIMARY KEY
timeline_id         TEXT NOT NULL REFERENCES timelines(id) ON DELETE CASCADE
kind                TEXT NOT NULL
order_index         INTEGER NOT NULL
name                TEXT NOT NULL
muted               INTEGER NOT NULL DEFAULT 0
UNIQUE(timeline_id, order_index)
```

### `timeline_clips`

```text
id                  TEXT PRIMARY KEY
track_id            TEXT NOT NULL REFERENCES timeline_tracks(id) ON DELETE CASCADE
artifact_id         TEXT NOT NULL REFERENCES artifacts(id)
source_in_ms        INTEGER NOT NULL
source_out_ms       INTEGER NOT NULL
timeline_start_ms   INTEGER NOT NULL
volume              REAL NOT NULL DEFAULT 1.0
muted               INTEGER NOT NULL DEFAULT 0
position_x          REAL NOT NULL DEFAULT 0
position_y          REAL NOT NULL DEFAULT 0
scale               REAL NOT NULL DEFAULT 1.0
rotation_deg        REAL NOT NULL DEFAULT 0
opacity             REAL NOT NULL DEFAULT 1.0
```

### `timeline_history`

```text
timeline_id         TEXT NOT NULL REFERENCES timelines(id) ON DELETE CASCADE
history_index       INTEGER NOT NULL
snapshot_json       TEXT NOT NULL
operation_type      TEXT NOT NULL
created_at          TEXT NOT NULL
PRIMARY KEY(timeline_id, history_index)
```

Each durable snapshot is versioned:

```json
{
  "schemaVersion": 1,
  "timeline": {}
}
```

Required indexes:

```text
timelines(project_id)
timeline_tracks(timeline_id, order_index)
timeline_clips(track_id, timeline_start_ms)
timeline_clips(artifact_id)
timeline_history(timeline_id, history_index)
```

Foreign keys remain enabled on every connection.

---

## 8. Timeline Mutation Architecture

The frontend does not directly manipulate database rows. All committed edits flow through `TimelineService`.

```text
UI
 ↓
typed mutation
 ↓
TimelineService
 ├── load timeline
 ├── expectedRevision validation
 ├── operation validation
 ├── overlap validation
 ├── transactional write
 ├── revision +1
 ├── history update
 └── return fresh TimelineDocument
```

Conceptual operation contract:

```ts
type TimelineMutation =
  | { type: "add_track"; kind: "video" | "audio"; name: string }
  | { type: "reorder_track"; trackId: string; order: number }
  | { type: "add_clip"; trackId: string; artifactId: string; timelineStartMs: number }
  | { type: "remove_clip"; clipId: string }
  | { type: "move_clip"; clipId: string; timelineStartMs: number }
  | { type: "trim_clip"; clipId: string; sourceInMs: number; sourceOutMs: number }
  | { type: "split_clip"; clipId: string; timelinePositionMs: number }
  | { type: "set_clip_audio"; clipId: string; volume: number; muted: boolean };
```

Exact Rust representation may follow existing conventions, but public semantics remain equivalent.

---

## 9. Optimistic Revision Control

Every committed mutation carries `expectedRevision`.

```ts
{
  timelineId: "...",
  expectedRevision: 18,
  mutation: { ... }
}
```

Backend requirement:

```text
database revision == expectedRevision
```

Otherwise return `revision_conflict`. No automatic merge is attempted in Phase 3.

This also protects future AI Editing Agent operations from overwriting newer user edits.

---

## 10. Undo / Redo

Phase 3 uses bounded persistent state snapshots rather than full event sourcing.

A timeline has:

- a monotonic `revision`,
- a history snapshot cursor independent from revision,
- at most 50 retained snapshots.

Normal edit:

```text
if redo snapshots exist → delete them
apply edit
revision +1
store resulting snapshot
move history cursor to new snapshot
```

Undo:

```text
find previous history snapshot
restore it transactionally
revision +1
move history cursor backward
```

Redo:

```text
find next history snapshot
restore it transactionally
revision +1
move history cursor forward
```

Undo/redo never decrement revision.

Example:

```text
revision 18 state A
revision 19 state B
revision 20 undo → state equivalent to A
revision 21 redo → state equivalent to B
```

A new normal edit after undo deletes the redo branch. Pruning retains at most 50 snapshots and must never remove the current history cursor.

---

## 11. Frontend Interaction Semantics

Transient pointer movement is not persisted continuously.

```text
pointerdown
→ pointermove (frontend preview state)
→ pointermove (frontend preview state)
→ pointerup
→ one committed timeline mutation
```

Committed state remains backend-authoritative.

---

## 12. Media Preview Gateway

The frontend must not receive canonical artifact filesystem paths.

Required boundary:

```text
ArtifactRef
    ↓
MediaPreviewGateway
    ↓
artifact ID validation
    ↓
ArtifactResolver
    ↓
read-only seekable media response
    ↓
HTML video/audio element
```

Preferred conceptual URI:

```text
openvideoai-media://artifact/<artifact-id>
```

Implementation may use a Tauri 2 custom URI protocol or equivalent safe mechanism that:

1. accepts an artifact identifier rather than an arbitrary path,
2. resolves only through the Artifact Registry,
3. is read-only,
4. supports byte-range/seek behavior,
5. rejects malformed/unknown IDs and traversal attempts.

Do not weaken the ArtifactRef boundary to make preview easier.

---

## 13. Preview Scope

```text
Preview = responsive editing aid
Final Render = authoritative output
```

Required preview correctness:

- clip source range,
- trim boundaries,
- split positions,
- timeline placement,
- playhead mapping,
- selected clip playback.

Phase 3 does not require a browser-side pixel-perfect multi-layer compositor.

---

## 14. RenderSpec v1

The renderer consumes an immutable snapshot, not live timeline rows.

```ts
interface RenderSpecV1 {
  version: 1;
  projectId: string;
  timelineId: string;
  timelineRevision: number;
  output: {
    width: number;
    height: number;
    frameRate: number;
    container: "mp4";
    videoCodec: "h264";
    audioCodec: "aac";
    pixelFormat: "yuv420p";
  };
  tracks: RenderTrack[];
}
```

Render clips contain logical artifact identifiers and editing parameters, never physical paths.

Supported canvas presets:

```text
720p landscape     1280x720
1080p landscape    1920x1080
720p portrait      720x1280
1080p portrait     1080x1920
```

Default frame rate is 30 fps unless explicitly selected from validated timeline settings. Changing canvas dimensions/frame rate is a timeline mutation and increments revision.

---

## 15. Immutable Render Snapshot

When the user presses Render:

```text
Timeline revision N
↓
validate
↓
build RenderSpec v1
↓
persist complete RenderSpec inside durable job payload
↓
submit render.timeline job
```

The worker renders that persisted spec and does not silently re-read a later timeline revision.

---

## 16. Render Job Idempotency

Use the Phase 2 database-enforced idempotency guarantee.

Conceptual identity:

```text
render.timeline:<timeline-id>:<timeline-revision>:<render-spec-version>:<renderer-profile-version>
```

Renderer profile version starts at 1.

Two render clicks against the same revision/profile converge on one durable job. A later timeline revision creates a new job.

---

## 17. Render Job Payload

```json
{
  "schemaVersion": 1,
  "renderSpec": {
    "version": 1
  },
  "rendererProfileVersion": 1
}
```

The complete RenderSpec is persisted in the job payload. Do not store only a timeline ID and rebuild against live state when the worker starts.

---

## 18. Render Job Checkpoints

Required stage-level checkpoints:

```text
validate_spec
resolve_artifacts
compile_ffmpeg
render_media
validate_output
import_output_artifact
finalize
```

Phase 3 does not claim encoder-state/frame-level resume.

---

## 19. FFmpeg Compiler Boundary

```text
RenderSpec
↓
validation
↓
FfmpegExecutionPlan
```

Conceptual result:

```ts
interface FfmpegExecutionPlan {
  inputs: ResolvedInput[];
  filterGraph: string;
  outputArgs: string[];
}
```

The compiler must not mutate timelines, write SQLite state, submit jobs, execute FFmpeg, or import artifacts.

---

## 20. FFmpeg Compilation Semantics

Video clip concept:

```text
source
↓
trim
↓
reset timestamps
↓
transform
↓
offset onto timeline
↓
compose by track order
```

Audio concept:

```text
source audio
↓
atrim
↓
reset timestamps
↓
volume/mute
↓
timeline placement
↓
mix
```

Correctness takes priority over premature optimization.

FFmpeg execution uses executable + argument arrays, never shell-string concatenation.

---

## 21. Local Render Runner

The process runner is responsible for:

- resolving detected FFmpeg executable,
- spawning FFmpeg outside the UI thread,
- passing individual arguments,
- capturing exit code,
- capturing bounded diagnostics,
- cooperating with Phase 2 lease/heartbeat semantics,
- cleanup/cancellation behavior where supported.

Windows paths with spaces and Unicode must work.

---

## 22. Render Temporary Output

```text
render staging
↓
FFmpeg output.tmp.mp4
↓
process exit == 0
↓
verify output exists and is non-empty
↓
FFprobe validation when available
↓
Artifact Core import
↓
canonical SHA-256 artifact
↓
project association
↓
ArtifactRef result
```

A failed render never creates a valid canonical output artifact.

---

## 23. Output Encoding Scope

Phase 3 output is fixed to:

```text
Container:     MP4
Video codec:   H.264
Pixel format:  yuv420p
Audio codec:   AAC
```

Deferred:

```text
HEVC/H.265
AV1
WebM/VP9
ProRes
hardware/GPU encoding
custom FFmpeg flags
```

---

## 24. Output Artifact Contract

Successful result is logical, not path-based:

```json
{
  "artifactId": "...",
  "uri": "artifact://...",
  "kind": "video",
  "timelineRevision": 18,
  "renderSpecVersion": 1,
  "rendererProfileVersion": 1
}
```

---

## 25. Error Model

| Error | Meaning |
|---|---|
| `timeline_not_found` | Timeline does not exist |
| `revision_conflict` | Expected revision differs from current revision |
| `invalid_timeline_operation` | Mutation is invalid |
| `clip_overlap` | Same-track overlap was attempted |
| `invalid_clip_range` | Source/timeline range is invalid |
| `artifact_not_found` | Referenced artifact does not exist |
| `media_duration_unknown` | Required source duration is unavailable |
| `preview_unavailable` | Preview could not be served |
| `render_spec_invalid` | Timeline cannot produce a valid RenderSpec |
| `ffmpeg_unavailable` | Local renderer is unavailable |
| `render_failed` | FFmpeg exited unsuccessfully |
| `render_output_import_failed` | Rendered file could not be imported into Artifact Core |

Do not send raw Rust backtraces, unrestricted FFmpeg stderr, SQL errors, or internal canonical paths to normal frontend error messages.

---

## 26. Security and Robustness Rules

1. Validate all project/timeline/track/clip/artifact IDs.
2. Media preview resolves only registered artifacts.
3. Preview is read-only.
4. RenderSpec contains no physical media paths.
5. FFmpeg arguments are not passed through a shell string.
6. Job output is imported through managed Artifact Core.
7. Timeline mutations are transactional.
8. History restoration is transactional.
9. Render workers honor Phase 2 lease-token fencing.
10. No secrets or unrestricted private paths are logged.

---

## 27. Performance Guardrails

```text
time representation = integer milliseconds
no DOM element per media frame
no entire-video load into RAM
artifact hashing remains streaming
FFmpeg execution does not block UI thread
drag/trim pointer motion stays frontend-local until commit
timeline commits are transactional
```

Timeline display uses a presentation-only zoom mapping:

```text
pixelPosition = timeMs * pixelsPerMs
```

---

## 28. Minimal Phase 3 Editor UI

```text
┌─────────────────────────────────────────────────────────────┐
│ OpenVideoAI   Projects | Editor | Jobs & Queue | Health    │
├─────────────────┬───────────────────────────────────────────┤
│ ASSET BIN       │                 PREVIEW                   │
│ video.mp4       │                    ▶                      │
│ audio.wav       │                                           │
│ image.png       │                                           │
├─────────────────┴───────────────────────────────────────────┤
│ 00:00         00:05         00:10         00:15            │
│ ─────────────────────│───────────────────────────────────── │
│ V2                                                           │
│ V1   [ clip A ][ clip B              ]                      │
│ A1   [ music                               ]                │
└─────────────────────────────────────────────────────────────┘
```

Required controls:

- create/open timeline,
- add supported artifact,
- select clip,
- move clip,
- trim start/end,
- split at playhead,
- delete clip,
- volume,
- mute/unmute,
- add/reorder basic tracks,
- undo/redo,
- choose supported canvas preset,
- render,
- inspect corresponding job.

Advanced transform controls are optional even though transform data exists in the model.

---

## 29. Backend IPC Surface

Minimum behavior:

```text
create_timeline
get_timeline
list_project_timelines
apply_timeline_mutation
undo_timeline
redo_timeline
build_render_spec
submit_timeline_render
```

Frontend convenience wrappers may expose operation-specific functions, but all mutations converge on the same backend validation/service rules.

---

## 30. Testing Strategy

### Timeline domain tests

At minimum:

- create timeline,
- add/reorder tracks,
- add clip,
- artifact type validation,
- source range validation,
- trim math,
- split math,
- move,
- same-track overlap rejected,
- cross-track overlap accepted,
- volume/mute validation,
- revision increments,
- stale expected revision rejected.

### Persistence/history tests

- Migration 5 upgrades Phase 2 DB without data loss,
- migration runner remains idempotent,
- timeline/tracks/clips survive DB reopen,
- foreign keys enforced,
- history snapshot schema version persisted,
- undo/redo restore state and increment revision,
- new edit after undo invalidates redo,
- history bounded to 50,
- pruning never deletes current cursor.

### RenderSpec/compiler tests

- RenderSpec version 1,
- artifact IDs only, no physical paths,
- exact timeline revision captured,
- trim mapping correct,
- timeline offsets correct,
- track ordering correct,
- audio mute/volume represented,
- H.264/AAC/MP4/yuv420p execution plan,
- Windows paths with spaces handled as individual args,
- compiler independent from live timeline state.

### Job integration tests

- same revision/profile submits one idempotent render job,
- new timeline revision creates distinct job,
- complete immutable RenderSpec persisted in job payload,
- old job remains bound to old revision after later edits,
- checkpoints emitted in expected stage order,
- render worker honors lease fencing,
- cancelled/stale attempt cannot publish successful output.

### Real FFmpeg acceptance

Generate a synthetic fixture via host FFmpeg:

```text
320x240
5–10 seconds
H.264 + AAC
```

Then prove:

```text
create project/timeline
→ import fixture
→ add clip
→ trim
→ split
→ move
→ adjust audio
→ build RenderSpec
→ submit durable render job
→ execute real FFmpeg
→ completed
→ import output
→ receive ArtifactRef
→ FFprobe output validation succeeds
```

If host FFmpeg is unavailable, use explicit environment-specific skip/degraded semantics; never fake a successful real render.

### Frontend tests

At minimum:

- editor loads timeline,
- asset bin renders,
- supported artifact add action,
- unsupported artifact handling,
- drag/trim commit only on interaction completion,
- split uses playhead,
- revision conflict safe handling,
- undo/redo controls,
- render submits expected revision,
- render-job navigation,
- preview unavailable state,
- reload/persistence flow.

---

## 31. Runtime Acceptance

```text
1. Launch OpenVideoAI.
2. Open/create project.
3. Create timeline.
4. Import/select real video artifact.
5. Add it to timeline.
6. Trim.
7. Move.
8. Split at playhead.
9. Undo.
10. Redo.
11. Mute or adjust volume.
12. Select canvas preset.
13. Render.
14. Confirm job appears in Jobs & Queue.
15. Confirm job completes.
16. Confirm output becomes project video ArtifactRef.
17. Play/inspect rendered MP4.
18. Restart OpenVideoAI.
19. Confirm timeline/tracks/clips/history persist.
20. Confirm Projects, Jobs & Queue, and System Health remain functional.
```

No blank/black WebView window or unhandled frontend console error is acceptable.

---

## 32. Scope Gate

### Included

```text
Timeline persistence
Internal multi-track model
Typed timeline mutations
Optimistic revision control
Bounded persistent undo/redo
Minimal timeline editor UI
Secure media preview
RenderSpec v1
FFmpeg compiler
FFmpeg local process runner
Phase 2 render-job integration
Stage-level render checkpoints
ArtifactRef output
```

### Explicitly excluded

```text
Gemini / LLM integrations
AI Editing Agent
Whisper
TTS
automatic captions
AI image generation
AI video generation
Kaggle
ComfyUI
Activepieces
remote rendering
object storage
server mode
transitions
keyframes
effects library
masking
chroma key
speed ramp
color grading
advanced audio mixer
image timeline clips
hardware/GPU encoder optimization
custom FFmpeg flag UI
nested timelines
multicam
```

Stop rather than silently expanding into excluded functionality.

---

## 33. Migration / Release Discipline

Expected progression:

```text
v0.1.0  Foundation
v0.2.0  Project & Artifact Core
v0.3.0  Durable Jobs & Events
v0.4.0  Media & Timeline Core
```

Before implementation:

1. verify Phase 2 working tree is clean,
2. verify Phase 2 final quality gates,
3. verify Phase 2 runtime release-close smoke,
4. verify/create `v0.3.0` only against the validated final Phase 2 state,
5. branch `phase/3-media-timeline-core` from that release state.

Do not rewrite Phase 0–2 release history.

---

## 34. Phase 3 Acceptance Gate

Phase 3 is PASS only if all applicable items are evidenced:

- [ ] Phase 2 base verified and preserved
- [ ] Phase 3 branch correct
- [ ] Migration 5 upgrades prior DB without data loss
- [ ] Timeline persists across restart
- [ ] Multi-track model works
- [ ] Same-track overlap rejected
- [ ] Cross-track overlap accepted
- [ ] Trim math correct
- [ ] Split math correct
- [ ] Revision conflict enforced
- [ ] Revision strictly monotonic
- [ ] Undo works
- [ ] Redo works
- [ ] New edit after undo invalidates redo
- [ ] History safely bounded
- [ ] ArtifactRef boundary preserved
- [ ] Preview does not expose raw canonical paths
- [ ] Preview supports seek/range behavior
- [ ] RenderSpec versioned
- [ ] RenderSpec immutable per submitted revision
- [ ] RenderSpec contains no physical paths
- [ ] Compiler separated from process runner
- [ ] FFmpeg uses argument arrays, not shell-string execution
- [ ] Render job uses Phase 2 idempotency
- [ ] Render worker uses Phase 2 lease fencing
- [ ] Render staging cannot create partial canonical artifacts
- [ ] Successful render returns ArtifactRef
- [ ] Failed render leaves no valid output artifact
- [ ] Phase 0/1/2 regression tests remain green
- [ ] TypeScript check PASS
- [ ] frontend tests PASS
- [ ] frontend production build PASS
- [ ] Rust fmt PASS
- [ ] Rust clippy `-D warnings` PASS
- [ ] Rust tests PASS
- [ ] `scripts/verify.ps1` PASS
- [ ] real FFmpeg acceptance PASS or documented environment skip
- [ ] runtime desktop acceptance PASS
- [ ] no Phase 4+ functionality slipped in
- [ ] final working tree clean

---

## 35. Design Approval Record

The user approved:

1. multi-track internal / minimal UI approach,
2. Timeline → immutable RenderSpec → FFmpeg Compiler → durable render job architecture,
3. relational timeline persistence,
4. non-destructive editing,
5. optimistic revision control,
6. bounded persistent undo/redo,
7. secure ArtifactRef-based media preview,
8. Phase 3 error/testing/migration/scope design.

This document is the written formalization of that approved design.

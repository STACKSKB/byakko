# Byakko rewrite plan

Started 2026-09-26 on `codex/application-rewrite`, from `3db624f`.

## Objective and authority

Replace the accumulated application with a human-readable, modular program.
A feature should implement clear, established contracts for data, legal edits,
effects, accepted outcomes and presentation. Favor simple functional composition and explicit
ownership over guards, wrappers and synchronized controller state. The user's
Elm/Xmonad examples express this clarity; Rust and Iced remain.

Remove the old application rather than keeping a legacy frontend beside the new
one. Git preserves it. Selectively carry forward reviewed portable domain values,
firmware codecs, native transactions, useful research tools and evidence. These
are protocol/domain assets, not a reason to preserve old controller ownership.
Temporary construction gaps do not redefine the full configurator objective.

Read [AGENTS.md](AGENTS.md) and [the constraint inventory](docs/rewrite-constraints.md).
Historical hardware acceptance describes the old executable and particular unit;
it is not evidence that new frontend paths work.

## Architectural decisions

1. Four product crates: core, devices, desktop, CLI. Root research tools depend
   inward. Do not add packages merely to impose another privacy layer.
2. Core `contract` owns serializable commands/completions, IDs and outcomes,
   separate from `session`. Native queues are adapters, not the contract.
3. Connection, feature state and operation identity are separate. Keymaps have
   an editor; keymap readiness is not connection state.
4. The shared editor lifecycle handles accepted observations; feature rules
   supply validation and write planning. Session owns
   exclusivity/correlation and genuine cross-feature effects. Named workflows
   compose features. No giant controller hidden in multiple `impl` files.
5. Core acceptance returns typed facts. Frontends must not reconstruct success
   from every editor's status. Preserve Readback versus TransportAccepted.
6. Desktop owns widget text, selection, gestures, navigation and window lifecycle.
   Baselines/drafts remain in core. Pass specific inputs instead of global Desktop.
7. Auto-save distinguishes latest intent from submitted target. Outer timers
   deliver deadlines; feature owners coalesce. Settings save one scalar at a time.
8. One immutable selected-device worker executes effects. Foreground work takes
   priority between passive macro slot reads; host frames are bounded. Keep known
   firmware settling time, not incidental delays from extra identity reads.
9. Native modules own conversion/planning/transactions by feature. Collapse pure
   forwarding layers. Replace error downcasting with typed transaction failures.
10. Use higher-order functions for demonstrated shared transformations/steps.
    Prefer explicit enums/functions when clearer. No speculative frameworks,
    service hierarchies, blanket cloning or metric-driven mechanical splitting.
11. Capabilities drive UI and constraints; preserve backend revisions and opaque
    bytes. Shared UI/core do not assume Nia87 slots, layers or report widths.
12. Preserve interaction behavior. Temporary feature gaps are recorded below;
    they do not authorize a reduced final product or deletion of firmware evidence.
13. User clarification: constrain growth through a unified editor model with
    explicit implementations under `editor/`. Grouping by technical role is
    appropriate when it exposes that contract. Common load/edit/revert/save and
    failure transitions must not be reimplemented for every feature. Features
    supply their domain rules; genuine workflows such as recording and
    save-and-assign compose editors. Avoid an extensible hierarchy or policy flags.
14. Organize all application roles this way, not just editors: `model/`,
    `validation/`, `library/`, `recorder/`, `projection/` and `workflow/` contain
    feature-named files. Desktop separates `form/`, `view/` and `widget/`.
    No parallel feature hierarchy or legacy module aliases. Native board and
    protocol-family organization retains the hardware boundary.

## Intended ownership

| Area | Responsibility |
| --- | --- |
| core/contract | Commands, completions, IDs, evidence and failures |
| core/session | Connection, exclusivity, correlation and routing |
| core/model | Owned feature values and serializable snapshots |
| core/editor | Shared lifecycle and feature implementations |
| core/validation | Feature constraints and snapshot validation |
| core/library | Occupancy knowledge, discovery and retained diagnostic captures |
| core/recorder | Explicitly timed local recording |
| core/projection | Pure capability-to-control projections |
| core/workflow | Genuine multi-feature sequences such as save-and-assign |
| devices/executor | Serialized delivery and scheduling |
| devices/hid, rongyuan, nia87 | OS collections, family codecs, board mapping, transactions |
| devices/storage and sampling | Durable files and OS samplers |
| desktop/app | Iced lifecycle, routing and effect delivery |
| desktop/controller | Connection/discovery, local recording, file jobs and debounce deadlines |
| desktop/input | Window-local input translated into portable actions |
| desktop/form | Unsubmitted input, parsing and user intents |
| desktop/view | Feature rendering over forms and core state |
| desktop/widget | Reusable physical keyboard and semantic controls |
| CLI | Typed parsing, file intent and output over shared core |

Use subdirectories only where navigation benefits; do not manufacture files just
to match this table. Public APIs expose concepts, not every internal helper.

## Implementation sequence

- [x] Inspect checkout and create rewrite branch.
- [x] Replace engineering rules and write this plan.
- [x] Review detailed constraint inventory against previous rules and current
  macro encoding/pacing, including the 250-byte upload versus 256-byte snapshot.
- [x] Replace old session, desktop/CLI controllers and worker with working keymap
  read/edit/save/revert/reconnect slice. Include retained drafts, close behavior,
  typed completion and deliberately different memory device. No live writes.
- [x] Rebuild macro library/editor and pure save-and-assign workflow.
  Preserve unknown slots, candidate reads, count rules and partial outcomes.
- [x] Restore exclusive local macro recording through the shared macro draft.
- [ ] Complete macro interaction review, including user-facing event choices
  and rendered layout.
- [x] Before expanding features, consolidate the shared editor lifecycle and
  feature implementations under `editor/`. Put the other application roles in
  top-level folders as requested. Remove the macro lifecycle duplication and
  constrain the session to routing/exclusivity and explicit workflows.
- [x] Rebuild coalesced lighting, picture and scalar settings with owned intent
  and submission state, real selector dependencies and one-pass read policy.
- [x] Restore files, local labels and diagnostic capture through the new
  architecture. Developer archive restore remains outside public UI.
- [x] Restore automatic discovery and reconnect behavior.
- [x] Restore assignment catalog navigation/search, physical-key capture and
  capability-described custom shortcuts.
- [ ] Restore host lighting lifecycle, OS samplers and verified restoration.
- [ ] Remove remaining obsolete native forwarding/error-erasure paths, retaining
  fixtures and useful research commands. Narrow APIs around demonstrated callers.
- [ ] Review ownership/docs, run workspace checks, compare protocol sequences
  and record outstanding physical gates.

## Validation

Each integrated slice must build both clients and exercise real public intents
using the memory backend. Verify rejected/stale/wrong-direction completions,
unchanged drafts on failure, newer queued intent during writes, reconnect
retention, partial workflow outcomes and closing/restoration. Preserve tests for
exact encoding, cached before-image backup, report order, pacing and read counts.

Run `cargo fmt --all -- --check`, workspace tests and strict Clippy after coherent
integration. Do not invent frontend handlers only for tests. Physical output,
rendered layout and Linux/hardware acceptance require their own coordinated work.
Do not claim a rewrite fixes the recorded recovery failures.

## Current status

The user explicitly confirmed source replacement on 2026-09-26, superseding the
audit-only restriction. The old core session workflows, desktop controllers, CLI
controllers and host worker scheduler are removed. The replacement keymap path
has a separate connection state, keymap editor and typed outcomes; both clients
use the new session and finite-command executor. The shared simulator has three
keys, three named layers and a read-only key. Native wire behavior is retained.

Integration review is complete. Workspace library/binary/integration tests,
strict all-target/all-feature Clippy, formatting, and locked native development
builds pass. The built CLI completed demo read -> plan-keymap -> apply-keymap
against memory. Review caught and fixed file apply accidentally including an
unrelated existing draft; it now rejects that situation without changing edits.
The executor test counts one initial read and one apply, with no extra read.

Native transaction diff review found only type-namespace/import changes; report
bytes, HID selection, pacing, backup and recovery logic are unchanged. No live
device writes were performed. New UI rendering, physical output and Linux runtime
behavior have not been accepted.

Commands run at this checkpoint:

```text
cargo test --workspace --all-features --offline --lib --bins --tests
cargo test -p byakko-cli --offline
cargo clippy --workspace --all-targets --all-features --offline -- -D warnings
cargo fmt --all -- --check
cargo build --locked --offline -p byakko-desktop -p byakko-cli
target/debug/byakko-cli --demo read
target/debug/byakko-cli --demo plan-keymap target/rewrite-smoke/keymap.json
target/debug/byakko-cli --demo apply-keymap target/rewrite-smoke/keymap.json
```

The macro checkpoint adds an occupancy library, selected editor,
passive worker discovery and shared save-and-assign workflow. Desktop preserves
the keyboard workspace while editing macros. CLI supports macro snapshots and
assignment. Core reports partial assignment failure explicitly. Review fixed a
queued-scan/foreground race and stale occupancy after uncertain writes; unrelated
observations remain intact. Client tests cover actual messages and file workflows.
All workspace tests, strict Clippy and both locked development builds pass.
No hardware writes or rendered-layout acceptance were performed.

The role-based architecture checkpoint is complete. One `Editor<F>` owns the
baseline, draft, submitted target, status and accepted outcomes for keymap,
macros, lighting, picture and settings. Feature rules supply projection,
validation, edits and write planning. The old Draft and feature editor wrappers
are deleted. Models, validators, libraries, recording, projections, workflows
and tests use top-level role folders; desktop uses form/view/widget folders.
Session retains correlation and workflows without a duplicate descriptor or
hidden macro aggregate. Native adapters and both clients use the new paths;
there are no legacy module facades.

All 56 core tests, 208 device tests, the external device integration test and
desktop/CLI tests pass. New lifecycle tests cover newer intent during a submitted
write, retained typed recovery, invalid results, evidence distinctions, scalar
settings and rejected operation allocation. Audit also corrected raw-zero macro
staging: reads/revert preserve it, but changed programs require an editable count.
Strict workspace Clippy, formatting and locked development builds pass. The
built CLI passed memory-only macro read/plan/apply, assignment and discovery.
Hardware behavior and rendered layout remain separate acceptance work.

The recording checkpoint adds window-local input, measured/fixed timing and
explicit release of held inputs on stop, focus loss or close. Core owns exclusive
recording against the sole macro draft; desktop's controller owns only options,
the local clock and a pending request while library cancellation finishes.
Recording disables device polling. Tests cover normal input, rejected input,
held releases, close/discard, scan cancellation and unsubmitted form retention.
Workspace tests (60 core, 21 desktop), strict Clippy, formatting and development
builds pass. Recording playback and rendered UI still require user acceptance.

The lighting/picture/settings checkpoint restores both clients through the shared
editor. Desktop holds only debounce deadlines, forms and picker gestures; core
retains submitted values and newer intent. Close flushes queued edits and failures
retain drafts without automatic retry. Settings retains the submitted scalar's
identity until completion, preventing a second field from entering that write.

Lighting snapshots now expose backend-owned picture context, including opaque
lighting observations. Actual context changes invalidate dependent pictures;
ordinary brightness/color updates retain them. The picture preparation workflow
loads lighting once when needed, activates the advertised display effect, and
reads its picture once. It preserves partial failures and does not get lighting
again after the accepted setter. Independent picture backends need no lighting.
Painting reuses the accepted brush color across selected keys.

CLI lighting/picture/settings snapshot commands validate revisions and contexts,
read only their associated baseline once and reject unrelated staged edits.
Workspace tests (73 core, 32 desktop, 213 devices plus external integration,
six CLI tests), strict all-target/all-feature Clippy, formatting and locked native
development builds pass. The built CLI passed memory-only read/plan/apply for all
three features. Native report sequencing, pacing, backup and recovery code did
not change. No hardware writes were performed; rendered layout and physical
output remain separate acceptance gates.

Readback matching ignores only snapshot provenance for lighting/picture; matching
bytes and context after a transport-accepted save do not falsely conflict with a
newer draft. The application renderer lives under `view/application.rs`; the app
owns event/effect delivery and lends render inputs without another mutable model.

The file and capture checkpoint restores portable macro import/export, local
macro names and explicit diagnostic capture/export. Core stages imported
programs through the selected shared editor and retains captures separately from
editable caches. File storage lives in `devices/storage/`; the desktop file
controller owns one background job and returns correlated completions to the app.
Closing waits for those completions, and failures reopen the window. Successful
exports remain successful if the device disconnects; imports still require the
same connection and selected slot. Existing files are never overwritten.

Archive capture performs one sweep without reloading feature editors or
restarting discovery. CLI comparison stays offline. Existing file formats are
preserved: desktop writes native archive bytes, CLI writes its JSON envelope.
Public desktop restore remains absent. Macro names and document binding metadata
are local presentation data; imports never select or assign a source slot.

Workspace tests (78 core, 40 desktop, 215 devices plus external integration,
seven CLI tests), strict all-target/all-feature Clippy, formatting and locked
development builds pass. CLI demo capture, offline self-comparison and existing
destination rejection also pass. No hardware writes or rendered acceptance.

The discovery checkpoint restores background enumeration and automatic
attachment through `controller/discovery.rs` and `controller/connection.rs`.
Enumeration has one outstanding correlated request. Commands, file work,
recording and close invalidate earlier results; those results are drained without
changing the active connection. Idle discovery runs at two seconds after the
initial 100 ms discovery cadence. Native enumeration remains read-only.

Every attached executor stays bound to its exact collection. Manual Read retires
the old worker and deliberately selects the current unique collection, even if
the cable transition occurred between scans. Automatic attachment binds the
observed identity. Core derives reconnect cautions from editor states, retaining
feature conflicts and failed-write diagnostics until deliberate reads. Ordinary
attachment/connection-read failures can retry after a later inventory result.

Connection refresh reads keymap, lighting, settings and picture once, followed
by the previously loaded macro slot when applicable, then passive macro discovery.
It never activates a display effect. The next read is derived from editor
readiness; there is no duplicate refresh queue or baseline. Navigation and
successful feature writes do not refresh unrelated caches.

Workspace tests (82 core, 50 desktop, 215 devices plus external integration,
seven CLI tests), strict all-target/all-feature Clippy, formatting and locked
development builds pass. Tests exercise manual worker replacement, failed
attachment, changed identity, ordinary read retry, held failed-write diagnostics,
bounded/stale discovery and read-only refresh on a backend without macros.
No hardware writes or physical reconnect acceptance were performed.

The assignment checkpoint restores the complete category catalog, measured
section navigation and highlighted section tracking. Categories navigate without
hiding other groups; word search covers advertised labels, numpad aliases and
category names. Enter selects a unique exact or unique remaining match. Physical
key capture is local and one-shot, accepts only an advertised action, and ends on
Escape, focus loss, navigation or device work.

Custom shortcut controls use advertised modifier/key choices and bounds.
Unsubmitted choices and local errors belong to `form/shortcut.rs`; the existing
core capability validator composes and validates staged actions. Changing a
target/layer or accepting an assignment reloads the form from the shared draft.
Presentation lives under `view/`; measured Iced operations live under `widget/`.
No form owns a second editable keymap. The memory demo now advertises shortcuts,
numpad and system choices so these controls can be exercised without hardware.

Workspace tests (82 core, 60 desktop, 216 devices plus external integration,
seven CLI tests), strict Clippy, formatting and locked development builds pass.
Message tests cover search/capture, one/two-modifier staging and saving, target
resets, local validation errors and capture cancellation during saves. No native
wire changes or hardware writes; rendered layout remains unaccepted.

The macro interaction audit found specific remaining regressions to fix next:
valid repeat input must stage immediately and invalid text must block Save/Assign;
event controls need advertised labels and choices instead of raw identifiers;
unchanged reads must preserve unsubmitted event fields and replacement target;
successful sequence edits must reset/collapse the composer while rejected edits
retain input. Restore the established limit/help text and New/Inspect flow.

Temporary gaps: macro interaction corrections/review and host streaming.
The native APIs, codecs, OS samplers, direct transaction tests and research tools
remain. Passive catalog priority is restored; host scheduling returns with its
feature milestone.
The full rewrite remains incomplete; this checkpoint is not configurator parity.

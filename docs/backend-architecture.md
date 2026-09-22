# Reusable native frontend: backend boundary

## Approved pre-alpha migration

The user approved `pre-alpha-proposal.md`, selected Iced and required the core to
support a future browser frontend or service adapter. `AGENTS.md` defines the
functional-first rules for all work. The sections below describing egui refer
to the retained research application, not the new desktop architecture.

The first physical boundary is `crates/byakko-core`: device-neutral keymap
values, validation and deterministic session transitions. Its normal dependency
is Serde; it has no GUI, HID, filesystem, thread or clock dependencies. The old
`backend` module re-exports these types so existing codec/adapter tests exercise
the same definitions rather than a second model.

Commands and completions are owned serializable values with connection
generation and operation IDs. Serialization is an internal contract at this
stage, not a stable public API or a network protocol. A browser adapter can
transport these values or run the core as WebAssembly without depending on
Iced messages, Rust channels or native HID handles. Device-specific validation
and expected-state checks remain mandatory in the executor.

Browser delivery does not imply direct WebHID works on this keyboard or every
browser. Direct browser transport and a native service are separate future
adapters. Any service will need its own session identity, origin/authentication
policy and bounded request validation; none is exposed by this change. No
JavaScript or browser engine is introduced into the native application.

The session model now drives the first Iced keymap screen in
`crates/byakko-desktop`. Its library receives a configured core session and executor; only
the binary composition root selects the Nia87 backend. Views own no
firmware representation, file paths, baseline copies or device workers. The
desktop now depends only on core, devices and Iced. The legacy root package
depends on devices for compatibility; it is not a desktop dependency.

Startup reads automatically; selection, search, staging, change review, apply,
verification, revert and explicit read/reconnect use the same core owner.
Generation publication precedes each read. The UI polls the bounded completion
queue every 25 ms only while a command is pending; no timer runs while idle.
Closing waits for in-flight I/O. Failed/uncertain completion keeps the window
and draft open; closing a dirty idle draft requires explicit discard. A dirty
reconnect conflict blocks editing/apply and retains the draft: revert, then
read to accept device values. There is no automatic hotplug monitor yet.

`--demo` composes the same screen/executor with `MemoryDevice`: three keys,
three named layers, one read-only opaque binding, two editable macro slots and
one opaque macro slot, with no hardware/file effects.
The memory backend validates expected state and advances opaque revisions.
Tests cover stale writes, fixed-key rejection, retained opaque values, UI layer
selection and close/failure transitions. Those tests do not establish physical
key output or graphical interaction correctness.

Iced 0.14 uses tiny-skia and system fonts here; WGPU, egui and the bundled Fira
font are absent from the desktop's normal dependency graph. The legacy root
GUI still uses egui when built separately or as part of all-feature workspace
checks. `tools/check_dependency_licenses.py --package byakko-desktop` audits the
Windows/Linux normal/build metadata separately; distribution asset notices and
source-header review remain required. Core compiles independently for WASM;
the native desktop is not a browser frontend.

`crates/byakko-devices` now provides the serialized native executor and a small
`KeymapDevice` effect contract. Nia87 codecs, mapping, transactions and archives
live under `byakko-devices::nia87`; native HID, storage and feature-gated research
instrumentation live in devices too. Its dependencies are core, Serde/JSON and
target-specific OS bindings. Root compatibility modules re-export the same
implementation, so research/legacy commands do not keep parallel copies. Old
file paths are retained in accordance with the user's no-deletion instruction.
The executor owns the adapter on one thread. Existing Nia87
transactions still open/close their established handles under the process lock;
this is not yet a persistent physical device session or multi-device discovery.

Migration verification compared 22 implementation modules against the preceding
source after namespace/rustfmt normalization. Existing tests moved with their
implementations: the workspace still has 183 passing library tests. Native
read-only acceptance through `examples/read_keymap.rs` issued the same owned
core command used by Iced, received its correlated completion and reached Ready.
Both complete 128-slot keymaps and identity matched the previously captured
full-configuration baseline exactly. This does not cover writes, physical
playback, the unresolved recovery fault or graphical interaction.

Both command and completion queues are bounded. The owner publishes the core's
connection generation before submitting commands and publishes zero on
disconnect. The worker rejects old-generation/duplicate commands before I/O,
independently of core's stale-completion checks. A generation change cannot
cancel a transaction that has already started. The UI must wait for completion
before closing; dropping a worker is not a rollback policy. Queue rejection
returns a correlated completion to feed into core, rather than leaving a pending
operation stranded. These rules apply equally to a future service adapter.

Nia87 keymap apply now exposes a typed recovery result while retaining its old
diagnostic/API for the research app. Preflight failure reports NotAttempted;
post-write failures carry the actual Verified/Failed rollback result. Executor
panics report Unverified. No recovery decision parses a display string. Existing
write order, delays, backup and full readback remain unchanged; hardware fault
recovery has not been reaccepted on the strength of these structural changes.

## Macro migration foundation

`byakko-core::macros` defines owned, serializable programs, events, snapshots,
capabilities and atomic draft edits. Key usages, pointer button usages, signed
movement and wait-after values are wider than Nia87 storage fields; available
slots and ranges come from a backend. Editing returns a validated candidate and
does not change the original on rejection. The backend must still validate its
encoded capacity before a candidate is accepted for storage.

`byakko-devices::nia87::macro_adapter` translates these values to the reviewed
simple-macro codec. Five pointer buttons have semantic usage IDs; wheel events
retain backend-scoped action IDs and edge flags until portable semantics are
established. Repeat zero is retained without claiming infinite playback. The
key's playback mode remains separate from the slot's shared repeat count; the
existing explicit count-1 policy for toggle/hold must survive UI migration.

Macro snapshots preserve the entire raw revision. Valid extended encodings of
short waits remain editable with the original before-image intact. Unknown or
malformed stores are exposed as opaque, preserved for inspection/export and
rejected for writes. Draft validation rejects a decoded baseline that differs
from its raw revision, wrong backend IDs, unsupported actions and byte overflow.
The native macro transaction now returns typed recovery status through a shared
error adapter, retaining its previous wire sequence, delays and diagnostics.

Macro commands now use the same executor as keymaps. `session::Session` owns one
generation, monotonically increasing operation sequence and `Activity` enum for
both surfaces; the compatibility name `KeymapSession` refers to that same owner.
Keymap trust and macro trust are independent from pending activity. Every
request, edit, slot switch and revert checks the shared activity guard. Replies
must match kind, generation, operation and macro slot before any state changes;
the macro editor separately checks the returned snapshot's backend and slot.

Each surface retains its own baseline/draft for its distinct data. A failed
macro operation retains the draft and requires rereading before another save.
Reconnect preserves dirty drafts; a changed observed revision enters conflict.
Keymap reads/writes invalidate macro trust; macro writes invalidate keymap trust
before dispatch, because earlier hardware recovery showed unexplained changes
outside the intended fields. Reverting a draft does not restore trust. A
generation change rejects queued old work but cannot interrupt started I/O;
desktop close continues to wait on the shared activity before dropping the
executor. Process termination is not a rollback guarantee.

The deterministic memory backend has optional macro capabilities/storage and
exact expected-state checks. Tests now exercise the complete session → worker →
memory apply → completion → session path and verify rereads and retained opaque
slots. Unsupported operations, stale commands, panics, conflicts and malformed
readback remain explicit failures. No live macro writes were performed for this
step. The Iced macro screen now uses this same session and executor. The view
projects slot/action capabilities and the sole core draft. Widget-local text
buffers hold only unsubmitted event/count inputs; explicit Stage actions parse
and submit edits to core. Failed edits preserve input and program. Slot changes
reject dirty drafts, sequence changes clear replacement targets, and failed
reads retain form input. Successful saves are verified against the macro result
when closing, even though they invalidate the keymap baseline's trust.

The memory-backed desktop tests exercise event replacement, signed movement,
zero wait/count preservation, readback, opaque-slot rejection, failure retention
and close behavior with a separately dirty keymap. Persistent local labels still
need migration; the working legacy macro panel is
not being ported wholesale. See `iced-macro-acceptance.md` for the current limits.

Macro capabilities advertise each slot's binding choices as typed keymap
actions, with an optional required saved repeat count. The desktop does not
construct numeric slot/mode codes. Nia87 supplies counted/toggle/hold choices;
the memory backend supplies named actions for nonnumeric slot IDs. Core checks
macro trust, clean saved contents, count policy and keymap readiness/writability
before staging a binding. Mode selection never rewrites the macro count. The
same staged-change review and serialized keymap apply perform the later write.
This policy check occurs at staging; keymap expected-state checks cover keymaps,
and do not make macro content and bindings one atomic transaction.

The core recorder appends through the macro editor's sole draft. It receives
integer millisecond timestamps, suppresses duplicate edges, preserves the old
prefix, and reserves reverse-order releases before accepting another edge.
`Activity::Recording` excludes device operations and all other edits. Iced owns
the clock and physical-input mapping; it shows a dedicated capture view,
excludes widget-consumed clicks, and ends capture on focus loss or close.
No worker polling runs during recording. Oversized timing intervals stop the
recording; the final held interval falls back to zero with an explicit notice.

Optional `ByteBudget` capability data describes additive encoding sizes, so
core can enforce storage limits without importing Nia87 packets. Supported
action costs must be positive and arithmetic is checked. Nia87's model is
tested independently against its encoder at delay and capacity boundaries;
native encoding remains the final authority. Other backends may omit this
model when their encoding does not fit it. No encoding service or closure is
injected into the browser-portable core.

Macro file operations use a correlated `FileTicket` and `Activity::MacroFile`.
The core ticket contains identity and operation kind, with no paths or OS file
handles. It excludes recording, device I/O, other edits and slot switches.
An import replaces the draft only after complete capability validation; stale
results cannot alter its data or file metadata. Export can preserve a retained
unverified draft without restoring its device trust. Iced performs file work
off the UI thread, waits before closing, and does not poll the device worker
while the local file operation is active.

`devices::macro_files` bounds JSON to 64 KiB, imports the reviewed native v1
format and emits core `Document` v2. Exports serialize before exclusive file
creation and never overwrite. The legacy implementation moved unchanged apart
from namespace into `devices::nia87::macro_file`; root re-exports it. File names
and binding preferences are per-slot desktop metadata; unsupported source
binding preferences are dropped with a notice. Import never stages a keymap
binding or changes the selected slot. Names currently persist through exported
documents, not automatically across app restarts. See `macro-documents.md`.

Byakko currently has a native egui frontend for the Nia87. The shared Keys editor now uses an injectable backend interface; the rest of the application is **not yet backend-neutral**. The long-term goal is to reuse the frontend and its interaction patterns for other keyboard backends, including potential QMK/VIA adapters, without making those backends emulate Nia87 packets or its fixed feature set. This is an internal architecture direction, not a public SDK commitment. The current Nia87 safety and recovery work remains independent of this migration.

## Implemented first slice

`src/backend.rs` defines dynamic physical keys, layers, typed actions, opaque observed revisions and a keymap backend interface. `src/backend/nia87.rs` owns conversion to the existing stock-firmware protocol and delegates guarded writes to the established transaction implementation. `src/keymap_ui.rs` renders and edits those backend-provided models without importing Nia87 codecs or geometry. Unknown bindings remain opaque and lossless.

Device-free tests render a separate 12-key, three-layer backend and exercise unsupported actions, read-only keys, stale state and incorrect readback. This is evidence for the keymap boundary, not a completed QMK/VIA implementation. Startup discovery, native profiles, macros, lighting, settings and archive orchestration still need migration. The Nia87 application bridge preserves existing native file formats.

The shared keymap editor is now the sole owner of the loaded keymap baseline,
staged actions and keymap apply worker. `Workbench` no longer retains separate
raw base/Fn drafts or another loaded `Snapshot`. Its Nia87 inspector and macro
binding controls project individual actions through the adapter and stage edits
through the shared owner. Raw snapshots are derived at import/export, reconnect
and archive boundaries. This removes the previous frame-dependent synchronization
between two editable copies; it does not make the remaining Nia87 panels generic.

## Implementation discipline

Keep cyclomatic complexity low by simplifying the state model and separating
decisions from effects. File splitting alone is not a complexity reduction.

- Give each editable state one owner; derive projections instead of synchronizing
  redundant mutable representations.
- Express validation, protocol encoding, change planning and recorder transitions
  as deterministic functions with explicit inputs and results. Pass time into
  transitions rather than reading a clock inside the decision logic.
- Use enums to represent mutually exclusive workflow states and typed outcomes
  where callers must distinguish failure or recovery conditions. Avoid independent
  booleans that admit impossible combinations and parsing error strings for control flow.
- Keep transport, files, clocks and worker dispatch at narrow imperative boundaries.
  Keep device write order, settling delays and recovery policy explicit and auditable.
- Prefer exhaustive pattern matching and small composable transformations. Use
  iterators where they clarify data flow; use straightforward loops for ordered I/O
  and algorithms where an iterator chain would conceal the logic.
- Introduce abstractions for actual shared behavior, not speculative generality.
  Avoid blanket cloning, allocation and indirection merely to imitate immutability.
- Test transition invariants, rejected changes, wire representations and failure
  boundaries. Review branch count, nesting and state combinations as well as size;
  do not move branches into tiny helpers just to improve a metric.

The macro recorder now owns deterministic timing, held-input and capacity rules
without egui or I/O. The archive controller owns its operation state and worker
channel; the workbench coordinates it with the other editors. Remaining structural
work includes macro file/device orchestration and the remaining standalone device
transactions. Preserve wire behavior during these
changes. Feature expansion remains secondary to this cleanup.

## Nia87 device internals

`device/transport.rs` owns discovery, the process lock, HID framing and research
setter instrumentation. Its concrete session owns a handle and lock together;
the handle is dropped before the lock. Single-handle reads and complete archive
transactions use this ownership boundary. Host lighting retains the session
through restoration. Existing standalone transactions that deliberately reopen
handles retain that behavior; this extraction does not establish a universal
single-handle policy or resolve the recorded transport recovery failure.

`device/configuration.rs` contains whole-archive capture, apply and recovery;
`device.rs` retains feature reads and standalone feature transactions. The moved
capture, recovery and section-writing bodies and setter instrumentation were
compared with the preceding source and are unchanged apart from whitespace.
Session adoption changes ownership, not report contents or settling intervals.

`Settings::plan_change` prepares the raw-preserving target and forward/recovery
reports without I/O. The device operation performs expected-state checks, saves
the backup, executes the plan and verifies readback. Unencodable recovery values
are now rejected during preflight, before device reads. Valid transactions retain
their report order and settling delays.

Archive setting preflight reuses this planner. It additionally checks exact raw
archive compatibility in both directions, including the stricter backlight
constraint: a boolean toggle must reproduce the entire options reply. Unknown
unchanged fields remain archival data. Editing an auto-OS byte other than 0 or 1
is rejected before writes because a boolean setter cannot restore it exactly;
editing unrelated settings continues to preserve that byte.

## Current frontend coupling

| UI module | Device-specific assumptions in the frontend today |
| --- | --- |
| `app.rs` | Calls `device::snapshot` from its read worker and coordinates the archive controller. Keymap baseline, draft and apply work belong to the shared editor. The Nia87 inspector/macro-binding view still uses `board::slot_for_usage`, `layout::nia87_keys()` and Base/Function layers, with adapter projections for raw actions. Profile/archive formats and archive change rendering remain Nia87-specific. |
| `archive_workflow.rs` | Owns capture/review/apply states and workers calling Nia87 `capture_configuration` and `apply_configuration`. Review planning, archive serialization and section summaries remain Nia87-specific. State separation does not make this a generic backend capability. |
| `macro_ui.rs` / `macro_state.rs` | UI dispatches `device::read_macro`/`apply_macro`; the state model owns the slot, draft, paired raw/decoded baseline and read/apply lifecycle. The recorder is deterministic and separate. The 50 slots, `u8` identifiers, 256-byte before-image, Nia87 codec/capacity, play modes and file format still reflect this backend. |
| `lighting_ui.rs` | Calls `device::read_lighting`/`apply_lighting`, `lighting::write_report` and the Nia87 effect catalog. Holds decoded settings alongside raw lighting reports; displays raw bytes. Screen/audio streaming invokes device-specific stream modules. |
| `picture_ui.rs` | Uses `layout::nia87_keys()` and `board::slot_for_usage`, edits the Nia87 matrix color array (including nonphysical slots), and calls `device::read_picture`/`apply_picture`. The fixed slot bounds and initial Esc selection are layout assumptions. |
| `settings_ui.rs` | Calls `device::read_settings`/`apply_setting`, validates via Nia87 `settings::write_report`, and renders a fixed debounce/auto-OS/sleep/backlight set with four raw replies and opcode labels. |

Worker threads, staged drafts, expected-state checks, backups, and readback verification are useful patterns, but the current workers do not themselves establish a backend boundary. Protocol encoding and device operations need to move behind a backend session; widgets should receive typed, discoverable capabilities and state.

## Target boundary

Introduce a device-neutral application model and a `Backend`/`BackendSession` interface. Discovery returns a stable backend identity, device identity, display name, dynamic physical layout, layers, and capabilities. Keys have stable backend-provided logical IDs and geometry; bindings refer to an action model with an explicit `Unsupported`/backend-specific variant where translation is not possible. Avoid using HID usage as a universal physical-key ID or assuming that every binding occupies four bytes. Layers are a list of identified, named layers, not a two-value enum.

Capabilities describe available editing surfaces and their constraints: keymaps; macro slots, event types, limits and play modes; lighting controls/effects and optional per-key color geometry; typed settings with ranges/enums and validation; optional host streaming. Tabs and actions appear only when supported. A backend can expose read-only or partially understood values. The UI edits a typed draft and asks the backend to validate it, preserving unrecognized values until the user explicitly changes them. Nia87-specific options and raw diagnostics live in an explicitly named backend details panel, not in shared widgets.

The session owns transport, protocol codecs, device-specific mapping, capability discovery, serialization, and write policy. A practical interface has operations such as `describe`, `read_state(scope)`, `validate(change)`, `plan(change, expected_revision)`, `apply(plan)`, and `export/import`; exact Rust signatures can evolve. The returned state and changes carry stable key/layer/slot identifiers and a revision or equivalent expected-state token. Before applying, the backend checks observed state against the expected state, creates a recoverable backup, writes, reads back, and reports verification. A capability may refuse writes when safe verification is unavailable. The UI orchestrates worker progress and conflicts without constructing reports or calling firmware codecs.

Portable profiles should contain only actions and capabilities with defined cross-backend meaning and report unsupported mappings during import. A separate lossless native archive is namespaced by backend ID, schema version, and device compatibility identity; its opaque backend payload retains raw/unrecognized data and is round-tripped only by the owning backend. Never silently translate or discard opaque data in a portable export. The existing Nia87 archive remains a Nia87 format until an explicit migration with round-trip tests is implemented.

## Incremental implementation

First slice: extract a small model for `DeviceDescriptor`, dynamic `PhysicalKey`/`LayerId`, key bindings, keymap capability, observed state, and changes; define an injectable keymap session. Adapt the existing Nia87 `device` functions behind it, keeping their expected-state, backup, and readback behavior. Move `app.rs` keymap reading/applying and the key grid/layer selector to the descriptor and typed binding model. Keep Nia87 raw bytes behind the adapter, with a backend details affordance for expert inspection; preserve the existing profile/archive behavior during this slice. Do not move all tabs at once.

Verify with a fake in-memory backend describing a non-87-key layout and at least three layers. Test layout rendering/selection, staged edits, unsupported actions, a stale expected-state conflict, and a failed readback; confirm that none invokes Nia87 mapping or packet code. Run existing Nia87 unit tests and device-free UI tests. Subsequently migrate macros, lighting/picture, settings, and archives one capability at a time, using fake-backend cases for absent or partially supported features. Future QMK/VIA integrations can implement the session independently; this plan does not require firmware flashing, JavaScript in the UI, or copying GPL code.

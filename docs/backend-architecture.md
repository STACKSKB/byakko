# Reusable native frontend: backend boundary

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

The next structural work is to isolate the macro recorder from egui, extract
archive workflow decisions, and separate device sessions, feature operations and
transaction recovery. Preserve wire behavior during these changes. Feature
expansion remains secondary to this cleanup.

## Current frontend coupling

| UI module | Device-specific assumptions in the frontend today |
| --- | --- |
| `app.rs` | Calls `device::snapshot`, `capture_configuration`, and `apply_configuration` from workers and holds Nia87 archive review state. Keymap baseline, draft and apply work belong to the shared editor. The Nia87 inspector/macro-binding view still uses `board::slot_for_usage`, `layout::nia87_keys()` and Base/Function layers, with adapter projections for raw actions. Profile/archive formats and the 50-slot archive summary remain Nia87-specific. |
| `macro_ui.rs` | Calls `device::read_macro`/`apply_macro`; initializes 50 slots and `u8` slot IDs; validates drafts through Nia87 `macros::encode`, and stores the observed 256-byte representation. Macro event encoding, capacity, play modes, and import/export reflect that backend. |
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

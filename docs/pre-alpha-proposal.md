# Pre-alpha architecture — approved direction

Status: approved by the user with Iced selected. Implementation may proceed.
The 2026-09-25 GPL-3.0-or-later project-license decision supersedes the historical
no-copyleft selection constraint below; it does not change the chosen toolkit.
The later [protocol-family amendment](protocol-family-boundary.md) supersedes
the assumption below that all native wire behavior belongs directly to Nia87:
the Rongyuan driver should be reusable across verified OEM boards, and QMK/VIA
is the next independent backend after Nia87.
The user additionally requires future browser delivery: core models and
command/result contracts must remain independent of Iced, OS transport and
in-process channels. A future frontend may run the core in WebAssembly or
expose the native executor through a separately secured service adapter.
No server, web UI or browser HID support is being implemented in this slice.
This replaces incremental feature expansion as the immediate priority; it does
not reduce the full configurator parity objective.

## What went wrong

The prototype grew around screens and experiments instead of an application
model. Panels own device workers, backup paths, validation, mutable drafts and
presentation. Workbench still reads Nia87 snapshots directly while the shared
keymap editor separately owns apply. Macros and other panels still depend on
Nia87 storage details. Error strings sometimes carry recovery semantics.
Extracting files improved local structure but did not establish a clean whole.

Immediate mode is not required for this product. Replacing egui alone would
carry these problems into a different toolkit. The protocol findings and capture
corpus are useful; the current application structure should not be the template.

## Toolkit recommendation

Use **Iced**, subject to review and a small implementation spike after approval.
It supplies a Rust message/update/view model, Windows/Linux support and an MIT
license. It needs no JavaScript, browser, QML or vendor helper. Its software
renderer is worth measuring for this mostly static application; it is not a
promise of lower memory or CPU use. Iced draws its own widgets and its maintainers
still describe it as experimental. Text editing, scrolling, keyboard navigation
and packaging must be checked before committing to a full UI migration.

**Qt Widgets** is the alternative if mature desktop controls take priority over
an all-Rust frontend. Use C++ Widgets, not QML, with a small Rust boundary. Its
commercial/open-source licensing needs an explicit decision under the user's
no-copyleft constraint; LGPL is not assumed acceptable. Adding FFI and a second
build system is a real cost. **Slint** provides another declarative native option,
but its custom royalty-free/commercial licensing and additional UI language are
less attractive here than Iced's Rust/MIT path.

Sources reviewed: [Iced project](https://github.com/iced-rs/iced),
[Iced architecture](https://book.iced.rs/architecture.html),
[software renderer](https://docs.iced.rs/iced_tiny_skia/index.html),
[Qt licensing](https://doc.qt.io/qt-6/licensing.html),
[Slint licensing](https://slint.dev/agreements/slint-royalty-free-license.pdf).
These are selection tradeoffs, not a release dependency-license clearance.

## Three packages, explicit dependencies

1. **core**: device-neutral values and pure application transitions. Physical
   key IDs, layer IDs, actions, capabilities, observed snapshots, drafts, edits,
   validation results, operation IDs and typed outcomes. No GUI types, HID,
   filesystem, threads or clocks. Feature transitions are separate small
   functions; a top-level dispatcher delegates rather than becoming a giant
   match containing all behavior.
2. **devices**: backend implementations and the effect executor. Shared
   Rongyuan report framing and family-specific codecs sit below Nia87's board
   profile, while selected HID access, pacing, identity checks, backup formats,
   expected-state checks and recovery remain explicit. `yc500` and `gen2`
   commands must not share a write dispatcher because some opcodes collide.
   Platform HID code lives here. One worker
   owns each connected session and serializes its commands through a bounded
   queue. File effects and discovery also live outside core. Research commands
   remain separate entry points, excluded from the normal desktop interface.
3. **desktop**: composition root and thin Iced views. Renders model projections,
   emits user messages, submits effects and feeds completion events into core.
   Owns widget-local focus/scroll state. It neither constructs firmware reports
   nor starts independent device workers in each panel.

Dependency direction: desktop -> core; desktop -> devices; devices -> core.
Core never imports either outer package. Start with these three packages and
ordinary modules, not a plugin framework, service container or generic event bus.

## State and effects

One owner holds the configuration baseline and draft for each connected device.
Dirty state is derived from their difference. Panels receive projections and
emit edits, not independent editable copies of shared configuration. Macro
recording is a pure draft transition with supplied timestamps.

Use explicit operation states such as Disconnected, Loading, Ready, Applying,
Conflict and Unverified. An apply request owns an immutable expected snapshot
and proposed edit. Connection generation and operation IDs accompany completions
so replies from an old connection cannot change a new session. Long-lived model
data is not cloned on every view pass; immutable requests clone only what must
cross the worker boundary. Rust ownership is compatible with a functional core.

The execution sequence is: validate/plan -> verify expected device state ->
durable backup -> ordered writes -> readback -> typed completion. Failed writes
report whether restoration was verified, failed or not attempted. No parsing
display strings to decide success. Cancellation/disconnection cannot relabel an
uncertain write as saved. The unresolved hardware recovery failure remains an
acceptance gate, not something an architectural rewrite proves fixed.

Backends expose capabilities with their actual constraints. The UI does not
assume two layers, 50 macro slots, four-byte actions or Nia87 lighting IDs.
Do not force future QMK/VIA adapters to emulate those details. Preserve opaque
backend values losslessly, scoped to their backend. Portable profiles and exact
native backups are distinct formats. Only define abstractions exercised by the
Nia87 and a deliberately different in-memory backend; do not implement QMK/VIA
prematurely or promise portable macros without a defined semantic mapping.

## Migration and pre-alpha review gate

Preserve the current source and captures as the research baseline. Reuse reviewed
codecs, transport knowledge and invariant tests selectively. Do not port screen
controllers wholesale or silently discard unknown device bytes. Keep the deferred
accessibility patch aside.

After human approval:

1. Establish core state/command/result types and the memory backend. Prove draft
   ownership, stale completion rejection, conflicts and failure outcomes without
   a GUI or attached keyboard.
2. Route one full keymap workflow through the Nia87 backend and one session owner.
   Compare protocol output with existing fixtures; preserve established pacing.
3. Build one Iced screen: detect, load, select key, stage, review, apply, verify,
   revert and reconnect. Test the toolkit's actual editing/navigation behavior
   and measure resources before expanding the UI.
4. Exercise macro editing through the same boundary early enough to expose a
   bad model; then migrate lighting/settings and archives. Maintain the parity
   ledger and outstanding physical/Linux acceptance requirements throughout.

The architecture review should approve the toolkit direction, dependency graph,
state ownership and command/result contract before step 1. The first vertical
slice is a checkpoint toward the full product, not a new definition of completion.

## Backend readiness checkpoint

The current Iced feature views consume core descriptors, editor projections and
capabilities. They contain no Nia87 report opcodes or board-specific feature
branches. Size, spacing and palette defaults live in `desktop::panels::UiStyle`;
views compose panels using those tokens. The executable composition root still
constructs only Nia87 and demo sessions. Adding QMK/VIA will require a
discovery/adapter factory there, plus QMK/VIA descriptors and capability
catalogs, but should not require a separate keymap view.

The first QMK/VIA backend should implement the existing device contract for
keymaps before new feature abstractions are introduced. The current settings
model supports toggles and numbers; a QMK/VIA setting with a finite choice
catalog will need a new generic choice kind and view projection. Add that when
the first concrete setting requires it, preserving unknown backend values.

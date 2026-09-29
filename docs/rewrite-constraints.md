# Application rewrite constraints

This document collects the product and engineering constraints for the fresh
Byakko application rewrite. It is a constraint inventory, not a rewrite plan or
a claim that the rewrite already satisfies anything. The goal remains full
Nia87 USB configurator functional parity; temporary gaps during replacement do
not reduce that goal. Historical implementation and acceptance evidence is
summarized in [engineering status](engineering-status.md) and the linked
feature notes. Dated evidence applies only to the code, device, and scope it
records. It is not verification of rewritten code.

## Product and delivery

- Build an original native Windows and Linux desktop application in Rust with
  Iced. Do not put JavaScript, Electron, a webview, QML, a vendor helper, a web
  server, or a Node.js runtime in the desktop product or its build. USB Nia87
  stock firmware over USB is first; do not flash firmware. QMK/VIA and 2.4 GHz
  are later independent capabilities.
- Preserve full configurator parity as the destination, including keymap,
  macros, lighting, per-key colors, settings, discovery, and the workflows
  users need to work with those features. Keep the user's accepted persistent
  keyboard workspace, coalesced lighting choices, native color picker, modal
  discard prompt, passive macro discovery, and foreground read of an unbound
  macro candidate. Unknown macro slots are unknown, never presumed empty.
- Keep the public native archive workflow diagnostic: capture and export only.
  Do not expose archive import, review, or restore/apply in the public UI.
  Developer archive APIs and research examples may remain outside it. Keep
  automatic per-feature before-image backups for ordinary feature writes.
- Use Byakko-owned code, documentation, and assets under GPL-3.0-or-later.
  Preserve third-party notices and check dependency and asset compatibility.
  Do not copy vendor or Sharkfin source or UX.
- Functional completeness comes before visual polish. The visual direction is
  a white-tiger identity, no gradients, consistent spacing and alignment,
  clear attention hierarchy, concise text, and useful SVG or Unicode symbols.
  Keep the interface legible to first-time users and efficient for experts;
  ask the user about material UX, behavior, or design choices.
- Plan for an i18n catalog. Initial languages are English, Hindi, Bengali,
  Kannada, Telugu, Tamil, Marathi, and Japanese. Do not make protocol or state
  logic depend on UI text. Translation polish follows functional work.
- This is a personal pre-alpha. There is no CI/CD requirement. Keep local build
  and verification commands reproducible. Compare idle and active CPU and
  memory with Sharkfin and the official app under comparable workloads, and
  state the method and limits; toolkit choice or a mismatched process snapshot
  is not evidence of efficiency.

## Architecture and code shape

- Maintain three clear ownership boundaries. `core` owns portable domain data,
  validation, and deterministic transitions. `devices` owns HID, operating
  system effects, firmware codecs, ordered I/O, backup, pacing, and recovery.
  `desktop` renders projections and turns user input into typed messages.
  Core imports neither outer layer and contains no GUI types, filesystem paths,
  HID handles, threads, clocks, networking, environment reads, or platform
  conditionals. Pass values such as time in as inputs.
- Keep the serializable core/session commands and completions as the frontend
  contract. Iced and the CLI are independent clients. A future static browser
  client may use WebAssembly or a separate service adapter, but this does not
  authorize a web stack now. Browser HID permission and interface limits must
  be represented honestly.
  The user's 2026-09-29 request authorizes the optional WebHID/browser client;
  it does not add a web stack to the native product. Shared pure protocol codecs
  now live in `byakko-protocol` as needed by both transports. The first browser
  checkpoint is read-only; shared core editors and workflow semantics remain
  required for the eventual configurator. See [browser integration](webhid-browser.md).
- Use one owner for each device baseline and draft. Derive dirty state and
  rendered projections instead of synchronizing duplicate mutable copies.
  Model operation states and outcomes with enums and exhaustive matches;
  correlate results with connection generations and operation IDs so stale
  results cannot change current state. Do not encode a workflow in unrelated
  booleans or infer typed outcomes by parsing error strings.
- Favor small pure decisions composed into explicit transitions and effects.
  Keep side effects at clear boundaries; ordered device operations belong in
  a small imperative shell, where a straightforward sequence is easier to
  inspect than layered wrappers. Avoid giant reducers, generic event buses,
  speculative traits, helper proliferation, mechanical file splitting,
  gratuitous cloning, and premature micro-optimization. Prefer readable,
  modular names and files organized around domain ownership and real change
  boundaries.
- Capabilities and pure projections determine which controls exist. Views
  render those controls and emit edits; they do not choose protocol IDs,
  assume Nia87 counts or report widths, or own feature policy. Keep shared
  spacing/type tokens and reusable panes, panel layouts, and semantic controls.
  Keep Iced layout details out of core.
- Preserve opaque firmware values losslessly. Keep portable profiles distinct
  from backend-native backups. Capabilities describe observed backend support,
  not hopeful support. Avoid creating blanket abstractions where a small,
  typed family- or board-specific implementation is clearer.

## Device and protocol rules

- One serialized executor owns an attached device session. Keep collection
  enumeration and exact-target selection backend-neutral. A matching inventory
  entry alone does not authorize feature reports: the Nia87 open path verifies
  its board configuration collection and report shape. Keep tablet and other
  input-report semantics separate from keyboard configuration.
- Pin each desktop executor, including recovery opens, to the immutable
  selected Nia87 HID target. Never fall back to a different unique match after
  a target check fails. A reconnect replaces the old executor, rejects stale
  completions, and retains edits. Preserve feature-specific conflict and
  failed-write diagnostics across disconnect; after an uncertain write or
  conflict, require a deliberate manual read before automatic refresh resumes.
- The Nia87 uses the observed `yc500`-shaped path. Keep common observed 64-byte
  framing/checksum mechanics in a Rongyuan report module and keep `yc500` and
  `gen2` commands separate: opcodes collide and can have different meanings
  and payloads. Put Nia87 identity, layout, reserved slots, and proven
  capabilities in board data. Do not infer setter support from VID/PID or a
  shared GET. Treat within-family compatibility as unverified until another
  PCB supplies evidence. Do not build a general report interpreter or duplicate
  codecs. QMK/VIA is a separate backend using the portable contract, not
  Rongyuan emulation.
- Discovery and classification are read-only. Unknown board, firmware, or
  setter variants may be shown as unconfigured or read-only and must not inherit
  Nia87 write permissions. USB cable and 2.4 GHz receiver are transport
  variants, not protocol families; USB remains the supported Nia87 configuration
  transport until receiver relay behavior is observed and validated.
- Treat the connected configurator session as the owner of device state. Normal
  runtime loads each feature once, saves its cached before-image, and reads the
  affected feature once after a write when that feature requires readback.
  Do not add repeated identity snapshots, per-page identity barriers, or a
  second executor preflight. CLI file workflows may read once to establish the
  file's baseline; the executor consumes that baseline without repeating the
  preflight. Diagnostic investigations may repeat read-only captures; do not
  turn those repetitions into runtime checks.
- Keep exact collection selection, report/schema validation, known setter
  settling time, correlated completions, and verified recovery. Use explicit
  pacing rather than incidental waits caused by extra reads. Keymap, settings,
  and macro writes require one post-write feature readback. Ordinary global
  lighting and picture setters use their known pacing; transport acceptance is
  their successful completion and does not trigger a getter or automatic
  rollback. A real lighting selector change invalidates selector-dependent
  picture data and requires a subsequent picture read. A successful feature
  write must not invalidate unrelated caches or restart library scans.
- The user's 2026-09-27 event-driven follow-up supersedes the unsuccessful
  two-second polling implementation. Match the notification input collection to
  the selected keyboard's physical USB ancestor. Validate the native four-byte
  report (ID 5); the official RPC wrapper is not the native report format.
  Coalesce events and issue only affected loaded-feature reads. Include lock,
  system/power settings and selector changes. Reset/profile events refresh loaded
  configuration and invalidate macro occupancy summaries. Ordinary lighting
  events must not reread unchanged picture data, macros or settings. Preserve
  queued events during other activities and reject stale generations. Background
  reads retain editable drafts/navigation while saves and other operations remain
  serialized. Failure/conflict retention and setter completion evidence are
  unchanged. A healthy native listener replaces connected inventory polling too;
  discovery remains available for absent devices and listener failure.
- Keep backup, expected-state checks, pacing, verification, and recovery
  outcomes explicit in typed results. Failed or unknown recovery is never
  reported as a successful save. Retry only a concrete failure when firmware
  evidence supports the retry. Archive capture and verification use one
  complete sweep, not duplicate sweeps or section-by-section rereads.

## Feature-specific invariants

- **Keymap:** preserve full raw snapshots, including both 128-record maps,
  padding, empty entries, and the special Fn slot. Forward Nia87 writes target
  only ordinary slots populated in the observed default matrix, including the
  two unlabeled ISO positions. Recovery can repair any slot affected by a
  failed write. Portable edits use advertised typed actions; new opaque
  four-byte bindings are not programmable, while existing opaque values remain
  lossless. Shortcut choices and physical-key mapping are backend capabilities
  and board data, not UI assumptions.
- **Macros:** preserve the complete raw slot snapshot and revision. The observed
  conservative Nia87 range is slots 0–49; do not use the vendor allocator's
  apparent slot-50 off-by-one without device evidence. Keep the conservative
  encoder limit of 248 logical bytes (including the repeat-count header) unless
  new testing establishes otherwise. The observed wire upload is distinct:
  four full pages and a 26-byte final page write 250 bytes; the read snapshot
  remains 256 bytes. Do not restore the old 32-byte final-page write, which
  crossed into picture storage for slot 49. The current native transaction waits
  30 ms between macro pages and 2 s after the upload before its one readback.
  Stored repeat count zero is readable and preserved, but the observed counted
  editor admits 1–65,535; do not stage, save, or bind zero without new physical
  playback evidence. The native snapshot-file workflow is not a portable macro
  document. A changed snapshot is planned against one slot read/revision and
  staged through the correlated session path. Retain known macro pacing and
  one complete readback; do not add mismatch-triggered duplicate reads.
  Explicit developer native-backup restoration is separate from editor policy:
  validate the exact prior bytes, pin the selected collection, back up the current
  slot and verify the complete restored snapshot, including a stored zero count.
  Recording is exclusive local activity: feed explicit timestamps to core,
  reserve releases for held inputs, finish before close/focus loss, do not poll
  the device worker while only recording, and do not capture global input.
- **Lighting and per-key picture:** keep onboard effects separate from host
  screen-average and playback-music modes. Nia87 global lighting intentionally
  mirrors the official semantic `FFFFFF` → wire `FAFFFA` → semantic `FFFFFF`
  convention. This is backend behavior, not a bug or release gate; other RGB
  tuples remain unchanged. The [Windows boundary evidence](../Research/rgb-white-boundary-20260927.md)
  found literal native `FFFFFF` becomes `B4B4B4`, rejecting a general channel cap.
  Host modes continue across app focus changes, and use bounded frames sent
  only to the selected device and separate OS samplers. Start only from a
  verified editable baseline; Stop and close require verified restoration.
  Live music parameter changes remain within the active session, coalesce to
  the latest setting and retain the original restoration baseline. A
  recognized host mode left after a crash can be replaced only by explicit
  onboard-effect selection through the guarded lighting transaction. Preserve
  unknown replies as opaque and send no startup reset. Per-key color files need
  the complete advertised map and matching cached picture revision/context;
  stage changed colors through one command. Do not claim a fresh selector
  comparison before every write. A full native archive contains the picture
  response under its captured selector only. Developer restore APIs reject a
  transaction changing selector and picture colors together until a verified
  multi-selector backup/recovery representation exists.
- **Settings:** one scalar field per apply. Preserve all raw reply bytes,
  including unknown/reserved values; noncanonical values may remain opaque.
  Use capability-advertised types, bounds, steps, and units. Refresh reads
  sections once; navigation does not read again.
- **Established pacing:** keymap setters wait 1 s; ordinary global lighting and
  scalar settings wait 500 ms; picture upload pages wait 20 ms. Developer archive
  per-key color setters use 100 ms. These are evidence-based native transaction
  delays, separate from UI coalescing deadlines. Change them only with new evidence.
- **Archive:** public capture/export is diagnostic only. Preserve opaque native
  archive APIs for developer use as applicable. Nia87's full archive represents
  both keymaps, 50 raw macro slots, global lighting, 128 picture triples, and
  four settings replies. Capture is one sweep. The archive picture payload is
  under the captured selector; it is not a multi-selector backup. Exact native
  restoration must still report nonidentical bytes, including literal white
  transformed by the device; semantic white equivalence is not exact restoration.
- **CLI and files:** keep parsing closed and typed. Load only files associated
  with the chosen command, use the shared bounded JSON reader for snapshots,
  and preserve offline archive comparison before device discovery. CLI flags
  are presentation and cannot become a second protocol implementation or
  bypass session/executor rules. Read-only commands should support unattended
  Linux smoke runs.

## Acceptance and evidence boundaries

Preserve useful protocol fixtures, captures, and research history. Re-evaluate
each behavior on the rewrite with an appropriate local check. Unit or memory
backend evidence verifies modeled behavior only; it does not establish physical
output, playback, write/recovery behavior, Linux hardware behavior, or rendered
layout. A transport-accepted lighting/picture setter establishes acceptance by
the transport after pacing, not a device readback. Physical interaction and
Windows official-app capture work must be coordinated with the user. Do not
perform hardware writes or fault experiments without authorization.

The current evidence includes unresolved limits that remain facts until
addressed with new evidence: earlier picture recovery failed; a fault-injected
archive apply caused unexplained Windows collateral changes and recovery
failure; Linux GUI/runtime
acceptance is open; host lighting streaming/restoration and several macro
playback cases lack physical acceptance; physical Iced settings write/restore
remains open. The dated Linux literal-white restore reported nonidentical bytes;
the later Windows white comparison establishes the intentional official semantic
convention, not an exact native restoration or new Linux hardware proof.
Current and dated macro pacing evidence is specific to its
investigated device and transaction. None of these observations is a verified
rewrite regression or a rewrite fix. See the dated [engineering status](engineering-status.md),
[protocol boundary](protocol-family-boundary.md), [frontend contract](frontend-contract.md),
and feature acceptance notes for their exact scope.

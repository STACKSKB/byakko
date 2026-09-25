# Byakko engineering rules

This is a functional-first house. Read this file before changing code. The
approved direction is `docs/pre-alpha-proposal.md`, amended below for browser
delivery. Iced is the selected desktop toolkit; the egui application is a
research baseline, not the structure to port.
For a Linux checkout, start with `docs/linux-handoff.md` and retain the
read-only-first device acceptance sequence there.

## Read policy amendment (user-directed, 2026-09-24)

Treat the connected configurator session as the owner of device state. Do not
assume hostile users, competing configurators or hot-swapping between commands.
This supersedes older requirements below for repeated matching snapshots,
per-page identity barriers and pre-write fresh-state comparisons. Load each
feature once, back up the cached before-image, and read the affected feature
once after a write. Retry only a concrete failure where firmware evidence calls
for it. Keep exact collection selection, report/schema validation, known setter
settling delays, correlated completions, and verified recovery. Keymap,
settings, and macro writes use one post-write feature readback. Ordinary global-
lighting and picture setters are exceptions: after known pacing, success means
transport acceptance and does not trigger a post-write getter. Successful
feature writes must not invalidate unrelated caches or restart library scans.
A real selector change invalidates selector-dependent picture data, which
requires a new picture read.
CLI file workflows may read once to establish the file's current baseline;
the executor must not repeat that preflight. Archive capture/verification uses
one complete sweep, not duplicated sweeps and section-by-section rereads.

## Product constraints

- Native Windows/Linux desktop, Rust + Iced. No JavaScript, Electron, webview,
  QML or vendor helper in the desktop product or its build.
- Treat the core/session command and completion types as a frontend contract.
  Iced and `byakko-cli` are independent clients; a future
  static browser SPA may reuse the portable model via WebAssembly. Keep any web
  bootstrap/transport code out of the native executable and do not require a
  local Node.js server. Browser HID access still follows browser permissions;
  do not promise native plug-and-play behavior from a web page.
- Keep core logic usable from a future browser frontend or a service exposing
  the native backend. Browser support is an architectural requirement now, not
  authorization to add a web stack or server now. Never assume browsers can
  access every HID interface or bypass browser permission prompts.
- USB Nia87 stock firmware first. Do not flash firmware. Preserve the full
  configurator parity objective; QMK/VIA and 2.4 GHz are later capabilities.
- Original implementation and UX. Do not copy vendor or Sharkfin source.
  Byakko-owned code, documentation and assets are GPL-3.0-or-later by the
  user's 2026-09-25 decision, superseding the earlier no-copyleft rule. Preserve
  third-party licenses/notices and review dependency and asset compatibility.
- This is a personal pre-alpha: no CI/CD requirement. Use local reproducible
  build/test commands. Ask the user about UX/behavior/design decisions and
  coordinate physical or Windows official-app capture work with them.
- Finish functional parity or reach a genuine user-input blocker before visual
  polish. The later visual direction is a white-tiger identity with no gradients,
  consistent spacing and alignment, a clear attention hierarchy, concise text,
  and useful SVG/Unicode symbols. Keep it legible to first-time GUI users while
  retaining fast expert workflows; do not imitate either reference app.
- Plan an i18n catalog instead of treating English literals as permanent UI
  data. Required initial languages include English, Hindi, Bengali, Kannada,
  Telugu, Tamil, Marathi and Japanese. Do not begin translation polish ahead
  of functional work, but avoid new protocol or state logic keyed to UI text.
- Profile idle and active CPU and memory under comparable workloads against
  Sharkfin and the official app. State the measurement method and limits; do
  not infer efficiency from toolkit choice or an unmatched process snapshot.

## Architecture and functional discipline

- Three boundaries: `core` owns domain values and deterministic transitions;
  `devices` executes effects and owns firmware/OS details; `desktop` renders
  state and emits messages. Core imports neither outer layer.
- Within `devices`, put shared observed HID framing in a Rongyuan report
  module, but separate `yc500` and `gen2` command families: some write opcodes
  collide and have different effects. The Nia87 uses a `yc500`-shaped path;
  its identity/layout/capabilities are board data. Never infer write support
  from VID/PID or a common GET alone. Treat compatibility within one family as
  unverified until another PCB supplies evidence. QMK/VIA is the next
  independent backend and should reuse the portable contract, not emulate
  Rongyuan reports. Follow `docs/protocol-family-boundary.md`; do not build a
  general report interpreter.
- Implement in `crates/byakko-{core,devices,desktop}`. The legacy root package
  may depend on these packages; the new desktop must not depend on the legacy
  package. Board-specific Nia87 behavior and data belong under
  `byakko-devices::nia87`; proven shared codecs and effects belong in their
  exact Rongyuan protocol-family module. Root re-exports are compatibility
  only. Do not maintain duplicate codecs.
- Core has no GUI types, filesystem paths, HID handles, threads, clocks,
  networking, environment reads or platform conditionals. Supply inputs such
  as timestamps explicitly. Transport-facing commands/results use owned,
  serializable values; in-process channels are an adapter, not the contract.
- One owner per device baseline and draft. Derive dirty state and projections;
  do not synchronize multiple mutable representations of the same data.
- Use algebraic data types and exhaustive matches for operation states and
  outcomes. Reject stale completions using connection generation/operation IDs.
  Do not parse error strings or encode workflow state in independent booleans.
- Separate decisions from effects. Prefer small pure functions and explicit
  effect values. Ordered device I/O belongs in a small imperative shell.
  Straightforward loops are appropriate for protocol sequencing.
- Keep cyclomatic complexity low through better models, not helper proliferation
  or mechanical file splitting. Avoid giant reducers, generic event buses,
  speculative traits, gratuitous cloning and premature micro-optimizations.
- Compose desktop pages from reusable panes, panel layouts, semantic controls
  and a small set of central spacing/type tokens. Let backend capabilities and
  pure projections determine which controls exist; views render those controls
  and emit edits. Do not put protocol IDs, one-off fixed pane dimensions or
  color literals in a feature view. Keep Iced layout concerns out of core.
- Capabilities describe actual backend constraints. Shared UI must not assume
  Nia87 layer counts, matrix slots, report widths or effect IDs. Preserve opaque
  values losslessly and distinguish portable profiles from native backups.
- One serialized executor owns each connected device session. Keep backups,
  pacing, expected-state checks, verification and recovery outcomes explicit.
  Failed or unknown recovery is never reported as a successful save.
- HID collection enumeration and exact-target selection are backend-neutral.
  A matching inventory entry does not authorize feature reports: Nia87 opens
  must still verify the board collection and report shape. Keep tablet/other
  input-report semantics separate from the keyboard configuration protocol.
- Recording is an exclusive local session activity. Feed explicit timestamps
  into core, reserve held-input releases, and finish before close or focus loss.
  Do not poll the device worker while only recording, or capture global input.
- Macro storage limits and editor policy can differ. Nia87 can decode a stored
  repeat count of zero, but the observed counted-mode editor admits 1–65,535.
  Preserve raw zero snapshots; do not stage, save, or bind zero through the
  editor without new physical playback evidence. An explicit native-backup
  restore is a separate recovery operation: validate the exact prior bytes,
  bind the selected HID collection, back up the current slot, and verify the
  complete restored readback.
- Nia87 keymap forward writes target only the ordinary slots populated in its
  observed default matrix, including two unlabeled ISO positions. Preserve the
  special Fn slot and empty matrix entries verbatim in snapshots and archives;
  leave recovery able to repair any slot affected by a failed write.
- Portable Nia87 keymap edits must use advertised typed actions. New opaque
  four-byte bindings are not programmable through the frontend contract;
  existing opaque values remain lossless in reads and native archives.
- CLI file workflows may read once to establish the file's current baseline;
  the serialized executor must not repeat that preflight. Keymap and settings
  writes use the shared session path, a cached before-image backup, then one
  complete post-write feature readback. Settings permits one scalar field per
  apply. Ordinary global-lighting and picture setters report transport
  acceptance after their known pacing; they do not imply device readback.
  The macro snapshot-file workflow plans against one fresh slot revision and
  stages through the correlated session import and guarded executor. Do not
  confuse that backend snapshot with a portable macro document; changed
  programs with stored repeat count zero remain unwritable pending playback
  evidence.
  The per-key color file workflow requires a complete advertised map and a
  matching cached picture revision/context; it stages changed colors through
  one session command. A real lighting selector change invalidates
  selector-dependent picture data and requires a new picture read, which may
  be requested as part of the selector activation flow. Do not claim a per-write fresh selector comparison.
  A full Nia87 archive contains only the picture response under its captured
  selector. Its preflight must reject a restore that changes both the lighting
  selector and picture colors in one transaction; there is no verified
  multi-selector backup or recovery representation yet.
  Recent physical evidence covers rebuilt Iced bulk picture submissions and
  visible steady-lighting choices; retain the earlier failed picture recovery
  as an unresolved concern. Do not broaden that evidence into archive restore,
  power-cycle persistence, or general recovery acceptance.
  Keep CLI command parsing closed and typed; load only the file associated with
  that command. Preserve offline archive comparison before device discovery
  and use the shared bounded JSON reader for snapshot inputs.
  Read-only CLI commands remain appropriate for unattended Linux smoke tests.

## Work sequence and evidence

- The pure model, memory backend, Nia87 adapter, and Iced keymap, macro,
  built-in global lighting, per-key picture, scalar settings and native archive
  capture/review/apply workflows now exercise the approved boundary. Iced
  picture submissions have bounded physical acceptance evidence: repeated
  full-image uploads and visible color changes succeeded. Successful ordinary
  writes mean transport accepted, with cached backup and explicit later read
  available. The earlier failed picture recovery discrepancy remains
  unresolved. RGB storage is separate from selecting the global picture
  effect. Settings stage one field per native transaction; Iced physical
  write/restore acceptance remains open.
  Native archive apply now has typed recovery outcomes and a reviewed Iced
  action, but has no live write acceptance in this slice; the earlier failed
  automatic recovery remains open. Iced now has a read-only, bounded USB
  discovery worker and an idle reconnect flow. Each desktop executor holds an
  immutable Nia87 HID target, including recovery opens; never fall back to an
  arbitrary unique match after a target check fails. A user-assisted physical
  unplug/replug and fresh read were observed on 2026-09-23; Linux runtime
  acceptance remains open. Before an automatic reconnect,
  preserve feature-specific conflict or failed-write diagnostics and require
  a deliberate manual read; keymap status alone does not cover other editors.
  Iced screen-average lighting now
  has a pure session lifecycle, a bounded frame path through the selected
  device executor, and a separate OS sampler. It advertises Start only from a
  verified editable lighting baseline and requires verified restoration on
  Stop/close. This is headless-verified only: physical Iced streaming, focus
  loss, disconnect behavior and Linux capture/runtime acceptance remain open.
  Iced playback music now uses the same host lifecycle with 32-band OS audio
  sampling. Its temporary brightness, option and color controls come from a
  portable host-mode schema; the Nia87 adapter alone translates these into
  firmware reports. This is headless verified, with physical and Linux runtime
  acceptance still open. A recognized host mode left stored after a crash can
  be replaced only by an explicit onboard-effect choice through the normal
  guarded lighting transaction; unknown responses remain opaque, and startup
  sends no automatic reset. Physical acceptance of this exit path is open.
  2.4 GHz remains a later capability. Do not pursue
  speculative UI polish or accessibility work before architecture review
  checkpoints. The physical-coordinate key selector is now shared by keymap
  and per-key color views; keep future selectors device-neutral and styled by
  `UiStyle` tokens. On 2026-09-23 the user confirmed the rebuilt Iced Keys page
  showed a TKL board and key selection worked after a physical USB replug.
  General one- and two-modifier shortcuts now use an optional portable
  descriptor capability and a staged Iced editor; memory-backend tests cover
  selection, apply and readback, while Nia87 codec tests cover encoding.
  Physical shortcut output remains unverified.
  The user explicitly prioritized a persistent keyboard workspace and responsive
  interaction on 2026-09-23. Keep the board visible while key, macro, lighting,
  per-key color and settings controls change around it. Scalar settings are
  presented together as toggles/sliders. Page navigation issues no reads.
  Connection refresh loads scalar sections once; macro discovery is passive,
  with a separate correlated ticket and foreground priority between slot reads
  on the same serialized executor. Recording cancels the scan after the active
  slot and remains local. The library shows configured/bound slots; Add chooses
  the first unbound free slot. Selecting a macro reads its editable snapshot.
  Explicit reads remain for failed or invalidated feature snapshots. The Windows
  release containing this UX change has been opened for user validation. The
  2026-09-25 camera capture shows the retained color-brush path painting F2
  and F3 green in the reviewed release; this does not establish subsecond
  batching. The official settings capture and current Iced session provide
  reference/implementation evidence, not physical Iced settings write
  acceptance.
- Preserve existing protocol fixtures and research evidence. Reuse reviewed
  codecs selectively; screen sampling is an OS effect separate from HID and
  the root sampler module is a compatibility re-export.
- Test invariants and failure boundaries: stale results, conflicts, rejected
  edits, unchanged drafts on failure, exact encoding and verified readback.
  Headless tests do not prove physical playback or Linux hardware behavior.
- Use bounded GPT-6 Sol/Luna subagents for independent tasks with explicit ownership.
  Review their changes. Avoid parallel edits to the same files.
- Commit after major completed steps. No PRs, issues, messages to others,
  Firefox automation, or file deletion. Use Edge/Codex browser for research.
- Coordinate physical interaction with the user when available. Keep hardware
  writes backed up and bounded; retain the recorded recovery failure as an open
  acceptance gate. Do not revive deferred work without authorization.

## UX review boundary (2026-09-23)

The user has requested that work stop after the specifically flagged UX fixes.
Do not resume parity work or further UX redesign without their next instruction.
Lighting now sends user choices automatically through coalesced intent queues;
normal use does not require Read/Apply. The native color picker replaces RGB
sliders. Verified writes retain unrelated cached baselines. The discard prompt
is modal.
Macro creation can foreground-read an unbound candidate while discovery remains
passive; unknown slots must never be assumed empty. Physical LED response and
final rendered layout remain for the user's review when they return.

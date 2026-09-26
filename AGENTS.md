# Byakko engineering rules

This is a functional-first house. Read this file before changing code. The
approved direction is `docs/pre-alpha-proposal.md`, amended here for browser
delivery. Iced is the selected desktop toolkit; the egui application was retired
in `3471866` and remains only a research baseline. Dated implementation evidence
and open acceptance gates are in `docs/engineering-status.md`.
For a Linux checkout, start with `docs/linux-handoff.md` and retain the
read-only-first device acceptance sequence there.

## Read policy amendment (user-directed, 2026-09-24)

Keep checks appropriate to a keyboard configurator (reaffirmed 2026-09-26).
Treat the connected configurator session as the owner of device state. Do not
assume hostile users, competing configurators or hot-swapping between commands.
This supersedes older requirements below for repeated matching snapshots,
per-page identity barriers and pre-write fresh-state comparisons. Load each
feature once, back up the cached before-image, and read the affected feature
once after a write. Retry only a concrete failure where firmware evidence calls
for it. Keep exact collection selection, report/schema validation, known setter
settling delays, correlated completions, and verified recovery. Use explicit
settling time instead of incidental delays from extra identity reads. Keymap,
settings, and macro writes use one post-write feature readback. Ordinary global-
lighting and picture setters are exceptions: after known pacing, success means
transport acceptance and does not trigger a post-write getter. Successful
feature writes must not invalidate unrelated caches or restart library scans.
A real selector change invalidates selector-dependent picture data, which
requires a new picture read.
CLI file workflows may read once to establish the file's current baseline;
the executor must not repeat that preflight. Archive capture/verification uses
one complete sweep, not duplicated sweeps and section-by-section rereads.

Clarification (user-directed, 2026-09-25): the one-pass rule governs normal
behavior of the emitted Byakko executable. The investigating agent may repeat
read-only captures as often as needed for diagnosis and acceptance evidence.
Do not turn diagnostic repetition into redundant automatic runtime preflights.

## Native archive pre-alpha scope (user-directed, 2026-09-25)

The public pre-alpha Iced UI exposes native archive capture/export as a
diagnostic tool only. Do not expose archive import, review or restore/apply in
that UI. Retain the core/device archive APIs, tests and research examples for
developer use, along with automatic per-feature before-image backups for normal
feature writes. Do not add an exact-restore white guard or semantic conversion
as part of this scope change. This defers the archive restore workflow and
closes its public release exposure gate; it does not fix the recorded raw-white
restore mismatch or the older Windows collateral changes. Research restore
remains available to developers outside the public workflow.

## Product constraints

- Native Windows/Linux desktop, Rust + Iced. No JavaScript, Electron, webview,
  QML or vendor helper in the desktop product or its build.
- Treat the owned, serializable core/session command and completion types as a
  frontend contract. Iced and `byakko-cli` are independent clients. Keep core
  usable by a future static browser SPA via WebAssembly or a service adapter;
  this does not authorize a web stack or server now. Keep web bootstrap and
  transport out of the native executable; require no local Node.js server.
  Browser HID still needs permission and may not expose every interface.
- USB Nia87 stock firmware first. Do not flash firmware. Preserve the full
  configurator parity objective; QMK/VIA and 2.4 GHz are later capabilities.
- Original implementation and UX. Do not copy vendor or Sharkfin source.
  Byakko-owned code, documentation and assets are GPL-3.0-or-later by the
  user's 2026-09-25 decision, superseding the earlier no-copyleft rule. Preserve
  third-party licenses/notices and review dependency and asset compatibility.
- This is a personal pre-alpha: no CI/CD requirement. Use local reproducible
  build/test commands. Ask the user about UX/behavior/design decisions and
  coordinate physical or Windows official-app capture work with them.
- Pursue functional parity when authorized, before visual polish. The later
  visual direction is a white-tiger identity with no gradients,
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
- Implement the product in `crates/byakko-{core,devices,desktop}`. The retired
  egui root application must not be revived; the root package retains research
  tools and may depend on the product crates. Desktop
  must not depend on root. Board-specific Nia87 behavior belongs under
  `byakko-devices::nia87`; proven shared codecs and effects belong in their
  exact Rongyuan protocol-family module. Do not maintain duplicate codecs.
- Core has no GUI types, filesystem paths, HID handles, threads, clocks,
  networking, environment reads or platform conditionals. Supply inputs such
  as timestamps explicitly; in-process channels are an adapter, not the contract.
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
- Each desktop executor binds an immutable Nia87 HID target, including recovery
  opens; never fall back to an arbitrary unique match after a target check
  fails. Before automatic reconnect, preserve feature-specific conflict and
  failed-write diagnostics and require a deliberate manual read. Reconnect
  replaces the old executor and rejects stale completions while retaining edits.
- Host screen-average and playback-music modes use a bounded selected-device
  frame path and separate OS samplers. Start only from a verified editable
  lighting baseline; Stop and close require verified restoration. A recognized
  host mode left after a crash can be replaced only by explicit onboard-effect
  selection through the guarded lighting transaction. Keep unknown replies
  opaque; send no automatic reset on startup.
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
- Settings permits one scalar field per apply. The macro snapshot-file workflow
  plans against one fresh slot revision and stages through correlated session
  import and the guarded executor. Its backend snapshot is not a portable macro
  document; changed programs with stored repeat count zero remain unwritable.
- Per-key color files require a complete advertised map and matching cached
  picture revision/context; stage changed colors through one session command.
  Do not claim a per-write fresh selector comparison. A full Nia87 archive has
  only the picture response under its captured selector. Developer restore APIs
  reject a transaction changing both the selector and picture colors until a
  verified multi-selector backup/recovery representation exists.
- Keep CLI parsing closed and typed, load only the command's associated file,
  preserve offline archive comparison before discovery, and use the shared
  bounded JSON reader for snapshot inputs. Read-only CLI commands are suitable
  for unattended Linux smoke tests.

## Work sequence and evidence

- Follow `docs/engineering-status.md` and feature acceptance notes for dated
  evidence and open hardware gates. Preserve protocol fixtures, research and
  captures. Headless tests do not prove physical output, playback or Linux
  hardware behavior.
- Reuse reviewed codecs selectively. Screen sampling is an OS effect separate
  from HID.
- Test stale results, conflicts, rejected edits, unchanged drafts on failure,
  exact encoding and verified readback. Keep future physical-coordinate key
  selectors device-neutral and styled by `UiStyle` tokens. Do not pursue
  speculative UI polish or accessibility work before architecture checkpoints.
- Use bounded GPT-6 Sol/Luna subagents for independent tasks with explicit
  ownership. Review their changes; avoid parallel edits to the same files.
- Commit after major completed steps. No PRs, issues, messages to others or
  Firefox automation. Use Edge/Codex browser for research. User-directed
  exception (2026-09-25): coordinate directly with the Windows agent through
  `docs/linux-to-windows.md` and remote replies. Commit and push requests/status
  as documented there; preserve both checkouts and never force-push. This does
  not authorize hardware writes or fault experiments.
- Coordinate physical interaction with the user. Keep hardware writes backed up
  and bounded. The recorded recovery failure remains an open acceptance gate;
  do not revive deferred work without authorization.

## Current work boundary (2026-09-26)

The 2026-09-23 stop after flagged UX fixes remains the default boundary. The
current request authorizes cleanup review and refactoring, documentation
cleanup, a scoped macro fix on a separate branch, and regression verification.
The current task also includes official-app macro write profiling, coordinated
physical checks and restoration. This does not resume unrelated parity work or UX redesign.
Keep the persistent keyboard workspace, automatic coalesced lighting
choices, native color picker, modal discard prompt, passive macro discovery,
and foreground read of an unbound candidate. Unknown macro slots must not be
assumed empty. Physical LED response and final rendered layout remain for user
review.

# Byakko engineering rules

This is a functional-first house. Read this file before changing code. The
approved direction is `docs/pre-alpha-proposal.md`, amended below for browser
delivery. Iced is the selected desktop toolkit; the egui application is a
research baseline, not the structure to port.

## Product constraints

- Native Windows/Linux desktop, Rust + Iced. No JavaScript, Electron, webview,
  QML or vendor helper in the desktop product or its build.
- Keep core logic usable from a future browser frontend or a service exposing
  the native backend. Browser support is an architectural requirement now, not
  authorization to add a web stack or server now. Never assume browsers can
  access every HID interface or bypass browser permission prompts.
- USB Nia87 stock firmware first. Do not flash firmware. Preserve the full
  configurator parity objective; QMK/VIA and 2.4 GHz are later capabilities.
- Original implementation and UX. Do not copy vendor, Sharkfin, GPL or other
  copyleft source. Review dependency and asset licenses separately.

## Architecture and functional discipline

- Three boundaries: `core` owns domain values and deterministic transitions;
  `devices` executes effects and owns firmware/OS details; `desktop` renders
  state and emits messages. Core imports neither outer layer.
- Within `devices`, put reusable observed Rongyuan wire/transaction behavior
  in a protocol-family driver and Nia87 identity/layout/capabilities in a
  declarative profile. Treat cross-board compatibility as unverified until a
  second PCB supplies evidence. QMK/VIA is the next independent backend and
  should reuse the portable contract, not emulate Rongyuan reports. Follow
  `docs/protocol-family-boundary.md`; do not build a general report interpreter.
- Implement in `crates/byakko-{core,devices,desktop}`. The legacy root package
  may depend on these packages; the new desktop must not depend on the legacy
  package. Nia87 implementations belong under `byakko-devices::nia87`, with
  root re-exports only for compatibility. Do not maintain duplicate codecs.
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
- Recording is an exclusive local session activity. Feed explicit timestamps
  into core, reserve held-input releases, and finish before close or focus loss.
  Do not poll the device worker while only recording, or capture global input.

## Work sequence and evidence

- The pure model, memory backend, Nia87 adapter, and Iced keymap, macro,
  built-in global lighting, per-key picture, scalar settings and native archive
  capture/review/apply workflows now exercise the approved boundary. The Iced
  picture flow has only read-only USB baseline verification; keep live picture
  writes pending while physical acceptance is unavailable and the earlier
  recovery discrepancy is unresolved. RGB storage
  is separate from selecting the global picture effect. Settings stage one
  field per native transaction and have read-only USB verification in Iced.
  Native archive apply now has typed recovery outcomes and a reviewed Iced
  action, but has no live write acceptance in this slice; the earlier failed
  automatic recovery remains open. Iced now has a read-only, bounded USB
  discovery worker and an idle reconnect flow. Each desktop executor holds an
  immutable Nia87 HID target, including recovery opens; never fall back to an
  arbitrary unique match after a target check fails. Physical unplug/replug
  and Linux runtime acceptance remain open. Host-driven effects and 2.4 GHz
  remain later capabilities. Do not pursue UI polish or accessibility work before
  architecture review checkpoints.
- Preserve existing protocol fixtures and research evidence. Reuse reviewed
  codecs selectively; do not move whole screen controllers into new packages.
- Test invariants and failure boundaries: stale results, conflicts, rejected
  edits, unchanged drafts on failure, exact encoding and verified readback.
  Headless tests do not prove physical playback or Linux hardware behavior.
- Use bounded GPT-6 Sol/Luna subagents for independent tasks with explicit ownership.
  Review their changes. Avoid parallel edits to the same files.
- Commit after major completed steps. No PRs, issues, messages to others,
  Firefox automation, or file deletion. Use Edge/Codex browser for research.
- No physical keyboard interaction is available while the user is away. Keep
  hardware writes backed up and bounded; retain the recorded recovery failure
  as an open acceptance gate. Do not revive deferred work without authorization.

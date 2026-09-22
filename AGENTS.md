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
- Capabilities describe actual backend constraints. Shared UI must not assume
  Nia87 layer counts, matrix slots, report widths or effect IDs. Preserve opaque
  values losslessly and distinguish portable profiles from native backups.
- One serialized executor owns each connected device session. Keep backups,
  pacing, expected-state checks, verification and recovery outcomes explicit.
  Failed or unknown recovery is never reported as a successful save.

## Work sequence and evidence

- Prioritize the approved pre-alpha vertical slice: pure model and memory
  backend, Nia87 adapter/session, then one Iced keymap workflow. Migrate macros
  and other features after that boundary is exercised. Do not pursue UI polish
  or accessibility work before architecture review checkpoints.
- Preserve existing protocol fixtures and research evidence. Reuse reviewed
  codecs selectively; do not move whole screen controllers into new packages.
- Test invariants and failure boundaries: stale results, conflicts, rejected
  edits, unchanged drafts on failure, exact encoding and verified readback.
  Headless tests do not prove physical playback or Linux hardware behavior.
- Use bounded Sol/Luna subagents for independent tasks with explicit ownership.
  Review their changes. Avoid parallel edits to the same files.
- Commit after major completed steps. No PRs, issues, messages to others,
  Firefox automation, or file deletion. Use Edge/Codex browser for research.
- No physical keyboard interaction is available while the user is away. Keep
  hardware writes backed up and bounded; retain the recorded recovery failure
  as an open acceptance gate. Do not revive deferred work without authorization.

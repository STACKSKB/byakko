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
  and Linux runtime acceptance remain open. Iced screen-average lighting now
  has a pure session lifecycle, a bounded frame path through the selected
  device executor, and a separate OS sampler. It advertises Start only from a
  verified editable lighting baseline and requires verified restoration on
  Stop/close. This is headless-verified only: physical Iced streaming, focus
  loss, disconnect behavior and Linux capture/runtime acceptance remain open.
  Iced playback music now uses the same host lifecycle with 32-band OS audio
  sampling and Nia87-only fixed green/upright effect presets; it is headless
  verified, with physical and Linux runtime acceptance still open. Editable
  music parameters and 2.4 GHz remain later capabilities. Do not pursue
  UI polish or accessibility work before architecture review checkpoints.
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
- No physical keyboard interaction is available while the user is away. Keep
  hardware writes backed up and bounded; retain the recorded recovery failure
  as an open acceptance gate. Do not revive deferred work without authorization.

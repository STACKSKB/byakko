# Byakko engineering rules

## Current authority

The user authorized replacing the application on 2026-09-26. Work on
`codex/application-rewrite`; Git preserves the prior implementation at `3db624f`.
Remove old orchestration instead of maintaining a parallel legacy application
or compatibility wrappers. Temporary feature gaps are allowed during construction
and must be recorded in [plan.md](plan.md); the full configurator target remains.

This file, [plan.md](plan.md), and [rewrite constraints](docs/rewrite-constraints.md)
are current. Older architecture documents describe the previous implementation.
Preserve protocol fixtures, research, captures, useful research tools, licenses,
and the untracked `Research/deferred-accessibility.patch`.

## Human readability and composition

Write an application a person can understand feature by feature. Aim for the
small explicit models, pure transitions and composition the user values in Elm
and Xmonad. This is an aspiration, not permission to copy code or imitate another
language at the expense of readable Rust.

- One clear owner per concept. Modules represent responsibilities, not line-count
  limits. Keep a feature's state, edits and completion rules discoverable together.
- Constrain editors through one shared lifecycle and explicit implementations
  under `editor/`, rather than letting each feature invent a controller hierarchy.
  The common model owns baseline/draft, dirty state, load/revert/apply acceptance
  and failure retention. Feature implementations provide values, edits,
  validation and write planning. Directory locality alone is not architecture.
  Keep genuine independent activities such as recording outside that lifecycle;
  do not add policy flags or extension hooks for hypothetical differences.
- Organize application modules by responsibility at the crate's top level:
  `model/`, `editor/`, `validation/`, `library/`, `recorder/`, `projection/`,
  and `workflow/`, with feature-named Rust files inside. Desktop uses `form/`,
  `view/` and `widget/` for those responsibilities. Do not preserve parallel
  feature folders or compatibility facades. Create roles only when needed.
  Native protocol families and board backends retain their hardware boundaries.
- Separate decisions from effects. Prefer ordinary functions, algebraic data
  types, exhaustive matches and explicit inputs/outputs.
- Use higher-order functions where they express real common operations: mapping
  results, projecting controls, applying edits or parameterizing a transaction
  step. Keep ordered device I/O as a straightforward readable sequence.
- Abstractions should remove repeated reasoning. Avoid pass-through services,
  speculative traits, generic event buses, dependency containers, excessive
  encapsulation, blanket cloning and helper proliferation.
- Small top-level routers delegate to owners. Do not split a giant mutable object
  across many `impl` files or build a giant universal reducer.
- Each feature owns its baseline and draft; derive dirty state. Submitted work
  and newer user intent are distinct facts. Preserve submitted targets until
  completion without synchronizing redundant editable copies.
- Name concepts clearly. Never parse English diagnostics to drive behavior. Use
  typed errors where outcomes differ, without wrapping every value in a new type.
- Validate external boundaries and real invariants. No hypothetical hostile
  users, competing configurators, repeated preflights or redundant guards.
- Optimize for understandable behavior, not line counts, tiny functions or
  synthetic complexity scores. Model real differences explicitly within the
  shared editor contract; preserve readback versus transport-acceptance evidence.

## Architecture

- `byakko-core`: portable domain values, feature editors, connection/operation
  identity, pure workflows and owned serializable commands/completions. No Iced,
  HID, files, clocks, threads, networking, environment access or OS conditionals.
- `byakko-devices`: native effects, one selected-device serialized executor,
  durable storage, transport, firmware codecs and independent OS samplers.
- `byakko-desktop`: Iced rendering, local forms, navigation, input, subscriptions
  and effect delivery. Views project state and emit intent; they do not determine
  whether a device save succeeded.
- `byakko-cli`: independent client of the same core/executor. Parsing and output
  belong here; do not duplicate domain workflows.
- Separate connection, feature and operation state. Connection is not keymap
  readiness. Core accepts correlated completions once and returns typed outcomes.
- Cross-feature workflows such as macro save-and-assign belong in core and retain
  partial outcomes. Window close and form parsing belong in desktop.
- Rongyuan framing is shared only where observed. YC500/Gen2 command families
  remain separate. Nia87 identity/layout/capabilities are board data; QMK/VIA is
  a later independent backend. No general report interpreter.
- Selectively preserve audited values, codecs, native transactions and formats.
  Rewrite old application orchestration; do not reinvent undocumented bytes.
  Retained code needs a named responsibility and evidence, not old API compatibility.

## Product and device constraints

Native Windows/Linux Rust + Iced. Byakko-owned work is GPL-3.0-or-later; preserve
third-party notices. No JavaScript, Electron, webview, QML, vendor helper or local
server in the native product/build. Keep core suitable for a future browser.
Stock USB Nia87 first; no flashing. No unrelated UX redesign during the rewrite.

The complete feature/read/pacing/recovery/file/UX/localization/performance rules
are in [rewrite constraints](docs/rewrite-constraints.md). Especially:

- Load each feature once, back up its cached before-image, write with established
  pacing. Keymap/settings/macros get one readback. Ordinary lighting/picture
  setters finish on transport acceptance after pacing, with no automatic getter.
- Preserve exact collection/report validation, immutable selected target, opaque
  bytes, stale completion rejection and typed recovery. No fallback to another
  keyboard, unrelated cache invalidation, extra rereads or scan restarts.
- Only real selector changes invalidate selector-dependent picture observations.
  Unknown macro slots are not empty. Retain foreground candidate reads.
- Keep persistent keyboard workspace, coalesced lighting, native color picker,
  modal discard and passive macro discovery as the interaction target.
- Public archives are diagnostic capture/export only. Keep developer APIs/tools.
- User-directed Nia87 global-lighting rule (2026-09-27): mirror the official
  white convention, semantic `FF FF FF` → wire `FA FF FA` → semantic white.
  Treat this as intentional backend behavior, not a bug or channel cap. Keep
  raw snapshots lossless and report exact archive byte differences honestly;
  the convention does not close unrelated recovery failures. See the
  [RGB evidence](Research/rgb-white-boundary-20260927.md).
- Recording is exclusive/local with explicit timestamps and held-input releases.
  Host lighting restores on stop/close. Startup never resets unknown device state.

## Work and evidence

Use bounded GPT-6 Sol/Luna subagents with explicit independent file ownership.
Prefer Luna for small inventories/docs and Sol for bounded implementation/review.
Review their changes; no parallel edits to the same files. Update plan status and
commit coherent completed steps. Do not push this rewrite unless requested.

Test behavior and boundaries: stale/wrong completions, rejected edits, retained
drafts, partial success, exact bytes, backup order and actual recovery. Test real
user-message paths, not production branches invented for tests. Run local builds,
tests, formatting and Clippy; no CI requirement. Headless tests do not prove UI,
physical output/playback, persistence, recovery faults or Linux hardware behavior.

Coordinate physical work with the user. Rewrite authorization does not authorize
keyboard writes, fault injection or flashing. Preserve recorded acceptance gaps.
No PRs, issues, messages to others or Firefox automation. Existing specifically
authorized Linux/Windows coordination follows `docs/linux-to-windows.md`; preserve
both checkouts and never force-push.

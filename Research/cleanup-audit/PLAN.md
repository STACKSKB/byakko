# Cleanup audit and implementation plan — 2026-09-25

Baseline: `a39cc1c`, pulled by fast-forward from `origin/master` (previous local
HEAD `264102c`). This is an analysis and plan, not an implementation of the
cleanup. The pre-existing untracked `Research/deferred-accessibility.patch`
was left untouched. No hardware commands were run.

The largest structural opportunity is retiring the old egui application and
its compatibility paths together, after preserving the useful research tools.
The best immediate work is smaller: obsolete names/UI messages, misleading
documentation, the CLI polling loop, and handwritten Win32 declarations.
Avoid replacing a few typed feature functions with a new generic framework.

## Evidence and method

- Windows native `cargo clippy --workspace --all-targets --all-features --offline -- -D warnings`: passed. Includes research target compilation; does not execute examples.
- `cargo fmt --all -- --check`: passed.
- `cargo test --workspace --all-features --offline --lib --bins --tests`: passed, 486 tests, zero failures. These are local/headless tests, not physical acceptance.
- AST body comparison: see [methodology](README.md), [results](results.txt) and [tool](ast/src/main.rs). Parsed 183 files and 1,954 function bodies, with zero parse errors. Exact bodies, identifier/literal-normalized bodies, and approximate structural similarity identify candidates; manual review distinguishes useful repetition from accidental duplication.
- Functional tracing: followed CLI planning → session staging/command correlation → executor → bound adapter → backup/setter/readback/recovery. Also compared the two desktop sampler workers and inspected live UI message producers.
- Repository-wide reference searches covered compatibility aliases, traits, old UI messages, root reexports and dependency consumers. Public APIs can evade compiler dead-code warnings; absence of in-tree consumers is not a proof about external consumers.
- Documentation was checked against current AGENTS amendments and implementations, rather than assuming every dated document is obsolete.
- Dependency review used manifests and cached source. In particular, the existing `windows-sys 0.61.2` provides the screen-capture declarations now written by hand.

Limits: this is syntax/reference and manual data-flow analysis, not whole-program
formal verification. Clippy does not prove that abstractions earn their cost.
Windows checks do not compile every Linux-only branch. No fresh Linux build,
Miri run, hardware test, upstream version survey, or dependency vulnerability
scan was performed. Existing refactor decision counts are a syntax metric,
not cyclomatic complexity; they also show that fewer decisions can coexist
with more functions/closures. Do not use a reduced score alone as acceptance.

## Prioritized candidates

### 1. Remove obsolete names and frontend-only paths

**High confidence, small scope.**

- `crates/byakko-core/src/session.rs:291`: `KeymapSession = Session` now has only in-tree test uses. Rename the tests and remove the old alias.
- `crates/byakko-devices/src/device.rs:167` and `src/lib.rs:5` within that crate: `KeymapDevice` aliases `Device`; remaining consumers are executor tests. Rename those uses and remove the alias/reexport.
- `crates/byakko-desktop/src/picture.rs:70`: the message enum suppresses dead-code warnings wholesale. `Apply`, `Revert`, and `Edit` retain manual-workflow handlers at lines 172–180, while the current UI paints through `Live`. Trace constructors including test aliases, then remove unused frontend variants or make deliberately test-only variants explicit. Test the actual emitted workflow instead of keeping production handlers only for tests. Do not remove core/CLI edit/apply APIs.

Acceptance: all-target Clippy without the broad enum suppression, existing
desktop workflow tests, no change in emitted commands or user-visible controls.
Do not treat ordinary field accessors or typed commands as obsolete aliases.

### 2. Retire the old application as a coherent unit

**Highest potential reduction, medium scope; research inventory comes first.**

`Cargo.toml:3,11` still makes the root research app the default run and enables
`gui`/`eframe` by default. `src/app.rs`, `src/macro_ui.rs`, `src/lighting_ui.rs`,
`src/keymap_ui.rs`, `src/settings_ui.rs`, and other root UI modules maintain a
second application architecture. `src/lib.rs` exposes roughly twenty one-line
compatibility modules as well as actual legacy behavior. Root macro labels and
stream workers are not merely reexports.

1. Inventory root binary subcommands and every example against current CLI,
   core/device APIs and recorded research use. Label each keep/migrate/retire.
2. Preserve useful capture, diagnostic, restore and fault-reproduction tools
   as explicitly selected research targets. Retain their fixtures and evidence.
3. Migrate retained tools to direct `byakko-devices`/`byakko-core` imports.
4. Make native desktop/CLI the explicit ordinary build/run path. A target
   default change must keep packaging and documented commands working.
5. Retire the egui UI, old `KeymapBackend`, duplicate UI state/recording code,
   and compatibility exports together. Remove `eframe` only when the last
   retained consumer is gone; update the lockfile/notices accordingly.

Do not remove the root package wholesale: its HID-access utility and research
commands still have value. The user's request here authorizes the plan, not
executing file deletions; the repository's no-deletion instruction still applies.

### 3. Collapse unique-device and bound-device adapter duplication

**Medium confidence until research migration is complete. Depends on step 2.**

`crates/byakko-devices/src/nia87/adapter.rs` has parallel `Device` implementations
for `Nia87Adapter` and `BoundNia87Adapter`. Feature adapters add `read/apply`
wrappers choosing `Access::unique()` alongside `*_with` methods.
`nia87/device/access.rs` represents both `Access::{Unique,Bound}` and borrowed
`Selection::{Unique,Expected}` for the same selection choice.

Migrate retained tools to discover/select a target once and use the bound path.
Then remove unused unbound trait implementation and wrappers; simplify selection
representation only after all callers are accounted for. Keep user-requested
unique discovery as a selection operation, not an implicit fallback on each
feature operation. This cuts layers without weakening exact-target recovery.

Acceptance: retained examples compile; selection tests cover multiple candidates,
missing original target, wrong collection/report shape and recovery opens.

### 4. Correct and consolidate documentation

**High confidence. Can proceed independently of code retirement.**

| File | Problem | Planned action |
|---|---|---|
| `docs/configuration-archives.md:9` | Claims repeated section reads, two full captures and 100-slot progress; implementation uses one sweep | Describe one capture sweep and 50-slot progress |
| Same file, line 23 | Presents REVIEW/APPLY as current native UI | Describe public Iced capture/export; move retained restore instructions under clearly marked developer research |
| `src/main.rs:38` | Prints “Two complete captures matched” after the current one-sweep capture | Correct this observable diagnostic alongside the docs |
| `docs/backend-architecture.md:132` | Describes cross-feature trust invalidation after successful writes | Update to current independent-cache policy |
| Same file, lines 203 and 304 | Old migration work is described as still pending | Keep historical findings separately; link current architecture/status |
| `PLAN.md:71` | Historical egui/toolkit/package direction competes with current direction | Mark superseded and point to approved architecture |
| `docs/pre-alpha-proposal.md` | Mixes approved direction and superseded licensing/toolkit exploration | Separate current decisions from dated rationale without rewriting history |
| `docs/linux-handoff.md`, `docs/linux-to-windows.md`, acceptance/evidence docs | Long histories also contain still-open acceptance gates | Keep current summary + dated evidence; do not erase failed recovery records |

Give each subject one current source: architecture, user operation, protocol,
and acceptance status. Other documents link to that source. Historical records
get an explicit date/status, not silent edits that make old evidence look new.
Validate relative links and documented commands after any reorganization.
Keep GPL and third-party notices, protocol fixtures and unresolved hardware
acceptance records. A document's age alone is not a retirement criterion.

### 5. Replace CLI polling with the standard channel API

**High confidence, small/medium scope.**

`crates/byakko-cli/src/lib.rs:218` implements blocking completion waiting by
repeated `Executor::try_receive`, deadline checks and 10 ms sleeps. The executor
already owns a standard `mpsc::Receiver` (`executor.rs:366`). Add a narrow
blocking receive surface using `recv_timeout` for reads and `recv` when no
timeout is supplied, then remove the polling loop and redundant clock logic.
Keep desktop's nonblocking receive path and preserve stale completion handling,
channel-close errors and the “outcome unknown” timeout meaning. A timeout must
not be represented as cancellation of a possibly active write.

Acceptance: completion, channel disconnect, stale completion and timeout tests;
verify that an unbounded write wait does not acquire a new artificial timeout.
No generic async runtime or new dependency is needed.

### 6. Use existing Win32 bindings instead of handwritten declarations

**High confidence for display sampling; separate audio decision.**

`crates/byakko-devices/src/screen_sample.rs:83` hand-declares user32/gdi32 calls
and ABI structs. The direct dependency `windows-sys 0.61.2` already contains
`GetDC`, `GetMonitorInfoW`, `CreateDIBSection`, `BITMAPINFO`, `MONITORINFOEXW`
and desktop APIs. Enable its needed `Win32_Graphics_Gdi` and
`Win32_System_StationsAndDesktops` features, import generated declarations and
remove the equivalent manual definitions. Keep RAII ownership and cleanup;
generated bindings do not replace resource-lifetime rules.

`audio_sample.rs` manually indexes COM vtables and transmutes function pointers.
This deserves a separate binding migration investigation. Do not claim that
the current `windows-sys` features automatically replace those interfaces, or
promote a transitive `windows` crate to direct dependency without comparing
the code/dependency cost and testing COM ownership.

Acceptance: native Windows compilation, format/layout tests and user-coordinated
read-only display/audio sampling checks where changed. No HID write needed.

### 7. Share sampler lifecycle only after settling semantics

**Confirmed duplication; medium behavior risk.**

`crates/byakko-desktop/src/{audio_stream,screen_stream}.rs` share preparation,
atomic states, one-event channel, start/stop/Drop and paced production loops.
There is a meaningful difference: audio uses blocking `send(Failed)` to retain
terminal failure behind a pending frame; screen uses `try_send(Failed)` and
can lose the error. A clone extraction must not hide this distinction.

First define and test terminal-event delivery, full queues, receiver drop,
stop-before-ready and repeated start. Then use a small private shared worker
implementation with sampler-specific initialization/next-frame closures if
it actually removes more lifecycle code than it adds. Preserve creation/use/
drop of the audio sampler on its worker thread, separate 30/40 ms pacing,
screen's first-frame reuse and audio silence processing. Avoid a public
sampler trait hierarchy or generic event bus for two clients.

### 8. Unify the two blocking file-task adapters

**High confidence, small scope; strongest normalized AST match.**

`crates/byakko-desktop/src/archive.rs:122` and
`crates/byakko-desktop/src/macro_files.rs:293` have structurally identical
156-token bodies after normalization. Both spawn a named filesystem thread,
send through an Iced/futures oneshot, convert spawn/receiver errors and map
completion into a frontend message. Use one small private typed function for
this plumbing, leaving ticket/state and message construction at each caller.
Keep macro export's destination-check warning on lost completion. Do not run
blocking filesystem work directly on Iced's async executor merely to shorten
the implementation. Compare both callers plus helper when measuring reduction.

Acceptance: archive/macro file workflow tests, worker failure and stale ticket
handling. No shared mutable file controller or job framework is needed.

### 9. Reduce repeated validation at its actual owner

**Selective, evidence required per guard.**

One concrete candidate is `crates/byakko-cli/src/lib.rs:62`: `read_macro`
looks up the advertised slot before calling `Session::select_macro`, which
delegates to `macros::editor::Editor::select` (`editor.rs:198`) and performs
the same membership check before mutation. Remove the CLI membership scan,
retain the editor check, and preserve the useful slot-specific error context
if needed. This is a redundant local check, not a redundant hardware read.

CLI feature reads and planners repeat status/capability checks already present
in session/editor APIs. Some have different semantics: CLI file planning checks
revision/backend identity before mutation; session owns connection/idle and
activity exclusion; editors own shape/value policy; devices validate report
shape and backup/recovery. Those are distinct boundaries, not automatically
redundant guards.

For each proposed removal, record the invariant, entry points and the one
remaining enforcing owner. Remove a check only if all paths still reach that
owner before mutation and retain useful errors. Prefer direct calls over a new
generic feature framework. `settings_ops.rs`'s idle check and `begin_feature`
are not duplicates: the latter allocates correlation and does not check idle.

Small `Draft` forwarding methods in lighting/picture/settings editors are low
priority. Existing `Draft`, `FeatureCommand`, `Envelope`, `DeviceCall::Rejected`
and durable-backup tokens have concrete jobs. Replacing these with macros,
public fields or one universal transaction engine would likely worsen the
first priority. Keymap and archive recovery ordering differs from scalar
round trips; preserve that distinction.

## Implementation batches and completion criteria

1. **Small removals and truthfulness:** obsolete aliases/UI messages, archive
   diagnostic, current docs. Run affected tests and strict workspace Clippy.
2. **Stdlib/dependency reuse:** CLI waiting, file-task plumbing and screen Win32 bindings in separate
   commits, with their failure/lifetime checks.
3. **Sampler consolidation:** test behavioral differences, then consolidate;
   measure resulting function bodies and module size including the new helper.
4. **Legacy retirement:** inventory and migrate research consumers first, then
   remove whole obsolete paths and corresponding dependencies. Requires the
   implementation instruction that resolves deletion/product-default choices.
5. **Reassess residual repetition:** rerun AST checks. Extract only remaining
   duplication that changes together and produces a net simpler call graph.

For each batch report deleted/added production lines separately from tests and
tools, removed entry points/dependencies, and changes in call depth. Count all
new helpers. Preserve existing invariant/failure tests rather than adding tests
that merely mirror wrappers. Run Linux checks for platform-affecting work;
headless success must not be called physical acceptance.

Do not remove: exact HID selection, generation/operation correlation, bounded
queues/JSON input, raw opaque preservation, durable before-images, setter pacing,
required post-write reads, verified recovery, or the vendored Iced clipping fix
without an equivalent passing raster regression. Ordinary lighting/picture
setters must retain transport-acceptance semantics and must not gain getters.

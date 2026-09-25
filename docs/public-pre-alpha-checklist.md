# Public pre-alpha review and release checklist

Review date: 2026-09-25. Reviewed source: `31de12abba9f2e3d3dfb8fb4ad68bf783f67082c`.
The clean `master` checkout was fast-forwarded from `977df91` using
`git pull --ff-only origin master`. This report changes no product code.

## User-directed scope (2026-09-25)

This is a personal public pre-alpha, not a corporate release process. There is
no CI/CD requirement. Byakko-owned material is GPL-3.0-or-later. Use local
build/test commands and a source checkout; release automation, signing and
installer integration are not prerequisites imposed by this review. Ask the
user before changing exposed behavior, UX or release feature availability.
Physical tests and Windows official-app packet captures are coordinated with
the user. Remaining acceptance questions below are evidence to resolve, not
authorization to disable features or expand the pre-alpha scope.

The renderer builds from tracked sources: the issue identified below is in the
optional source-inventory script, not the Cargo build or the renderer itself.

## Original review findings (status tracked below)

1. **P2 — the source inventory cannot handle the shipped renderer patch.**
   `Cargo.toml:46` selects a local `iced_tiny_skia` dependency, but
   `tools/audit_release_sources.py:49` rejects every non-registry source.
   Running the full inventory fails with
   `ValueError: Unexpected source for ('iced_tiny_skia', '0.14.1'): None`.
   Support explicitly reviewed vendored sources, preserve upstream revision and
   patch provenance, and hash their license/source inputs. Add a regression for
   this actual dependency graph. Do not silently omit the renderer from notices.

2. **P2 — strict Linux Clippy fails at the reviewed revision.**
   `crates/byakko-devices/src/screen_sample.rs:792` triggers `collapsible_if`
   with Rust 1.98.0 and `-D warnings`. This blocks the documented native gate,
   including the permission-helper gate through its device dependency. It is a
   lint failure, not evidence of a runtime defect. Fix it and rerun the exact
   strict commands; allowing that lint was used only to inspect the remaining
   diagnostics, not to claim the gate passed.

3. **P2 — standalone recovery loses the unknown-versus-mismatch distinction.**
   `crates/byakko-devices/src/nia87/device/apply_error.rs:30` and its macro,
   lighting and settings equivalents map every rollback error to
   `Recovery::Failed`. That includes an unreadable verification result, where
   recovery may have succeeded but cannot be established. The contract provides
   `Unverified` (`crates/byakko-core/src/session.rs:169`), and archive recovery
   already distinguishes unreadable from mismatched results
   (`crates/byakko-devices/src/nia87/device/configuration.rs:293`). Preserve
   that distinction through typed rollback results and failure-boundary tests.
   Neither current label reports success; this is diagnostic/contract accuracy.

4. **P2 — public instructions and acceptance claims are stale.**
   `README.md:30` describes verified staged lighting saves and no physical
   picture writes. Current ordinary lighting/picture saves are automatic and
   carry `TransportAccepted` evidence; newer physical checks exist. The parity
   ledger, Linux installation page and older acceptance sections also contradict
   later dated evidence. Clearly distinguish Iced from the retained research GUI,
   document which controls send writes automatically, and reconcile the read
   policy in AGENTS with the newer transport-accepted implementation before
   further maintenance or publishing instructions.

## Required release checklist

### Device safety and functional acceptance

- [ ] **Resolve the existing archive recovery gate before exposing archive
  Apply publicly.** The recorded injected fault caused unplanned macro/picture
  changes. Recovery at `nia87/device/configuration.rs:234–255` repairs only the
  planned macro slots/picture differences; final verification catches remaining
  mismatches, but cannot repair those collateral changes. Capture a correlated
  controlled fault trace, establish the cause, and demonstrate restoration of
  the complete before-image. Preserve the failure evidence. If unresolved,
  ask the user how to expose the limitation; do not silently disable Apply.
  See [fault evidence](../Research/configuration-fault-verification.md).
- [ ] **Accept partial-upload and interrupted-write behavior.** Exercise
  transport errors before and after picture pages, settings/keymap/macro writes,
  and lighting setters using bounded tests and coordinated hardware checks.
  Require durable before-images, accurate non-success outcomes, retained user
  intent, no speculative retries, and a demonstrated user recovery procedure.
  Ordinary picture/lighting acceptance must never be presented as readback proof.
- [ ] **Complete physical behavior and persistence checks for exposed features.**
  Cover both key layers, shortcuts/media/mouse actions, macro counted/hold/toggle
  playback and timing, clear/replace/bind, settings behavior, picture selectors,
  and power-cycle persistence. Keep stored repeat-zero snapshots lossless and
  outside writable macro-editor policy. Record firmware, board, OS, source
  revision, before-image and restore result for each case.
- [ ] **Accept host lighting lifecycle and document remaining limits.** Verify Iced
  screen/music Start, Stop, focus loss, close, device removal, sampler failure,
  reconnect and explicit exit from a stored host mode. Verify restoration and
  sustained streaming; test Linux audio routing and X11 display loss. Wayland
  screen capture is explicitly unsupported by `screen_sample.rs:770–778`;
  disclose that limit and distinguish it from Wayland GUI support.
- [ ] **Validate the final rendered workspace and asynchronous interactions.**
  Exercise rapid edits during uploads, mode switches, selector invalidation,
  settings queues, macro foreground reads during scanning, recording/focus
  loss, discard modal, close while busy, multiple-device ambiguity and
  disconnect/reconnect with dirty/conflicted/failed editors. Confirm navigation
  introduces no reads and successful writes retain unrelated caches. Headless
  tests are supporting evidence, not rendered or physical acceptance.

### Builds, platform delivery and reproducibility

- [ ] **Close the confirmed code/tool findings above.** Rerun strict formatting,
  Clippy, product tests, helper checks, renderer regressions and source inventory
  against the final release commit. Preserve logs with the commit/toolchain.
- [ ] **Record local Windows/Linux release checks.** Build the selected Iced
  desktop, CLI and helper with the lockfile. Record the tested toolchain and
  source revision; keep research tools/legacy egui separate from the product.
  No CI/CD. Recheck core's WebAssembly build where the target is installed.
- [x] **Provide source-build instructions.** The user selected source checkout
  and local builds only; see [source build](source-build.md). Binary downloads,
  packaging, signing, upgrades and installers are out of this release scope.
- [ ] **Verify normal-user Linux access and current read-only behavior.** Keep
  the narrow helper/udev instructions for source users. Verify only the intended
  node gets the active-seat ACL, then follow [Linux handoff](linux-handoff.md).
  Existing permissions are used during this run; no permission changes are made.

### Distribution audit and public documentation

- [x] **Include Byakko's chosen distribution license.** The user selected
  GPL-3.0-or-later. The root LICENSE contains GPL v3; the README grants the
  later-version option for Byakko-owned code, docs and assets, and all five
  workspace manifests declare `GPL-3.0-or-later`. Third-party licenses remain
  intact, including the renderer's MIT license.
- [x] **Preserve source licenses and provenance.** The source-only checkout
  includes GPL v3, the project later-version grant, the vendored MIT license and
  supplemental notices. The renderer inventory is repaired and records its
  source/patch hashes. Cargo fetches other dependencies with their own notices;
  no third-party binary notice bundle is shipped by this pre-alpha.
- [x] **Check dependency advisories.** Current RustSec check reports no known
  vulnerabilities and one informational `ttf-parser` maintenance warning;
  see [audit record](dependency-source-audit.md#current-rustsec-check-2026-09-25).
- [x] **Publish one accurate support/acceptance matrix and recovery guide.**
  Reconcile README, parity ledger, Linux setup, AGENTS and dated acceptance notes.
  State supported Nia87 USB/firmware scope, automatic-write behavior, backup
  locations, transport versus readback evidence, unaccepted capabilities and
  known failures. Include concise bug-report instructions and release notes.
  Keep private/ignored captures and vendor material out of the release bundle.
- [ ] **Finish the required resource comparison before efficiency claims.**
  Measure comparable startup, idle, active editing and sustained host lighting
  against Sharkfin and the official application; include child processes,
  methodology and limits. Existing warmed-idle results do not cover all workloads.
- [x] **Record the i18n catalog and initial-language rollout plan.** See
  [localization plan](localization-plan.md). Cover English,
  Hindi, Bengali, Kannada, Telugu, Tamil, Marathi and Japanese; keep protocol/state
  behavior independent of UI text. The repository requires a plan now, not
  premature translation polish. State the actual language availability of the
  pre-alpha explicitly.

QMK/VIA, 2.4 GHz, browser delivery, continuous tablet controls and unrelated visual
redesign remain deferred. They are not added to this pre-alpha checklist.

## Verification performed at the reviewed source

Environment: Linux x86_64, Rust/Cargo 1.98.0. No device access, setters, permission
changes, GUI acceptance or Windows runtime tests were performed.

| Check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| Locked offline tests for core/devices/desktop/CLI | 375 passed, no failures |
| Permission-helper unit tests | 2 passed |
| Vendored renderer clipping regressions | 2 passed |
| Python audit-tool unit tests | 7 passed |
| Linux desktop/CLI release build | Passed |
| Linux permission-helper release build | Passed |
| Strict Clippy for four product crates, all targets | Failed on `screen_sample.rs:792` |
| Strict helper Clippy | Failed through the same device lint |
| Diagnostic Clippy run allowing only `collapsible_if` | Passed; not a replacement for strict gate |
| Windows/Linux license metadata gates, desktop and CLI | Passed: desktop 126/175 packages; CLI 13/12 |
| Complete release source inventory | Failed on vendored `iced_tiny_skia` source |
| `ldd` for Linux release desktop | No unresolved directly linked libraries; dynamically loaded libraries not established |
| Windows build/runtime, current advisory database, WebAssembly check | Not performed |

The initial cross-target audit lacked cached dependencies; an authorized locked
fetch succeeded, then the metadata gates passed and the inventory failure was
reproduced end to end. No dependency versions or lockfile were changed.

Review covered recent changes and critical session/executor, write/recovery,
CLI, storage and release-tool paths, with independent device and frontend passes.
It was not an exhaustive proof of every FFI or protocol path. A suspected
reconnect issue was rejected after tracing the queue-blocking logic and is not
a finding.

The latest [official picture capture](../Research/official-picture-capture-20260924.md)
records physical preset/steady/hue response, twelve consecutive uploads and a
later full 128-entry readback match. Those successful checks are credited here;
they do not close partial-failure recovery, power-cycle or Linux write gates.

## Fix progress, 2026-09-25

- [x] GPL-3.0-or-later declared for all five Byakko packages; full license text
  and repository scope notice added. Third-party MIT notices preserved.
- [x] Removed CI/CD and corporate release-process prerequisites per user direction.
- [x] Renderer investigation: desktop and CLI release builds pass from a fresh
  `git archive` extraction of tracked source, with locked cached dependencies.
- [x] Source inventory supports the reviewed vendor directory and hashes source,
  license and patch provenance. Full inventory passes (182 versions); all nine
  audit-tool tests pass. Finding 1 is closed; no renderer code change was needed.

The verification table above remains the original review record.

- [x] Finding 2 closed: the Linux screen-sampler lint is fixed. Formatting and
  strict Clippy pass for all four product packages/all targets and the helper,
  with no lint allowances.

- [x] Finding 3 closed: standalone recovery reports typed, fully observed
  mismatches as Failed and unknown transport/read/planning outcomes as
  Unverified, including in the diagnostic text. Four native restore comparisons
  create the typed mismatch. Tests cover all four paths and misleading error
  text without parsing it. Device tests (200 unit + 1 integration), formatting
  and strict device Clippy pass. HID sequencing and recovery strategy unchanged.

The user selected a source-checkout-only pre-alpha. Binary packaging, signing,
installers and CI/CD are out of scope. Source-build and normal-user Linux access
instructions remain required. Hardware is available on Linux and Windows;
physical interaction will be coordinated around the user's availability.

- [x] Finding 4 closed: README, source-build/support guides, AGENTS, frontend
  contract and acceptance docs distinguish current behavior from dated evidence.
  Source-only scope and English availability are explicit; the localization
  plan records all requested initial languages without adding unapproved UX.
- [x] Current Linux read-only pass completed at `f8583d7`: all scalar/picture/
  macro checks and one archive capture passed, with the archive matching the
  historical hash. Existing user ACL retained; no setters or permission changes.
- [x] Windows historical failure evidence reconciled offline; exact byte diffs
  recorded in the investigation. No fault-run transport trace exists. Cause
  and recovery acceptance remain open; no new fault experiment was performed.

- [x] All four product crates check successfully for `x86_64-pc-windows-msvc`
  from Linux. This is compile checking, not Windows linking or runtime evidence.
- [x] Current RustSec advisory scan completed with no ignored advisories; zero
  known vulnerabilities, one upstream unmaintained-font-parser warning recorded.

- [ ] Supervised Linux lighting follow-up: visible mode/color switching confirmed,
  but the user saw no brightness difference at steady green 4 versus 1. Normal
  command readbacks matched. Exact raw lighting archive restore mismatched and
  rolled back with Verified recovery; original visible settings were restored
  afterward with user-approved canonical RGB bytes. Root cause and physical
  brightness remain open; see the Linux handoff and fault investigation.

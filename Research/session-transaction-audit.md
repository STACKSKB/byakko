# Session and transaction audit — 2026-09-25

This audit follows the user's architecture review and a further Linux macro
failure: macro slot 0 read page 1 returned EPROTO, then recovery write page 0
returned ENODEV. The screenshot identifies verification readback as the failing
stage, but not whether the getter's request or response failed. The user also
reported a failed macro library scan. Neither observation alone establishes a
malformed macro packet. No hardware writes were performed for this refactor.

## Follow-up correction

The initial refactor left parallel execution/failure dispatch and repeated
verification control flow. Its completion summary overstated the simplification.
The subsequent revision shares feature Read/Apply types, one executor dispatch,
session request construction and the direct verified transaction sequence.
See [reproducible measurements](refactor-metrics/README.md) for production line
counts, an explicitly limited AST decision-syntax score, and remaining costs.
There is no claim of a measured conventional cyclomatic-complexity reduction.

## Changes

### Session contract and completion handling

`Command` and `Completion` are now aliases of `Envelope<T>`. Generation and
operation occur once, outside `CommandPayload` and `CompletionPayload`.
The executor's exhaustive `token()` match is gone; execution and failure
dispatch match only the payload and attach correlation once.

`Activity` separates local activities from one pending device operation.
`DeviceActivity::Read/Apply(Feature)` retains the feature, including the exact
macro slot. Archive review additionally retains its target. The passive catalog
keeps its independent ticket, so it can complete during foreground work.

`Session::accept` performs one exhaustive payload dispatch. A shared completion
guard checks the pending operation, feature, direction and macro slot before
changing state. Wrong or stale completions leave the pending operation intact.
Failed feature writes share one invalidation policy: invalidate ready feature
baselines, preserving existing failure/conflict diagnostics. Successful writes
retain unrelated baselines. A real lighting selector change still invalidates
picture context. Archive apply retains its existing request-time invalidation.

This changes the serialized session command/completion shape to
`{generation, operation, payload}`. Both in-tree clients and tests are migrated
together. Stored device snapshots, automatic backup JSON and portable macro
documents are unchanged; this is not a backward-compatible session wire format.

### Device transactions and macro validation

The common transaction module owns durable JSON backup creation, the shared
apply/recovery/error flow, and named firmware pacing constants. A private-field
`DurableBackup` value gates the recovery helper. Normal JSON backups still use
exclusive file creation and `sync_all`; archive backups retain their existing
validated serializer through the same backup factory.

Keymap, macro, verified lighting and settings transactions use the common
recovery flow. Feature-specific comparisons and restoration plans remain
explicit. Ordinary lighting/picture writes still mean transport acceptance;
they do not gain a getter or an automatic recovery sequence. Developer archive
restore retains its distinct multi-section recovery, setter-start tracking and
mismatch evidence capture. Forcing those operations into a single ordinary
write/read/compare policy would change their semantics.

Pacing is named and centralized without changing its values: macro pages
30 ms, macro settle/mismatch retry 200 ms, picture pages 20 ms, archive per-key
color writes 100 ms, lighting/settings setters 500 ms, keymap setters 1 s.
These are firmware transport delays, separate from desktop coalescing windows.
The comments distinguish observed timings from evidence for their necessity;
this audit does not establish that each value is the minimum safe delay.

`ValidatedBeforeImage` preserves exact macro bytes and their decoded value.
The adapter validates and projects the baseline once, then passes that value
to the low-level transaction. Raw restore entry points still validate before
reaching the shared path. The before-image is not re-encoded: noncanonical but
recognized stored bytes remain exact for backup and recovery.

### Additional defects and duplication

1. **A background library scan could be silently abandoned.** The executor
   cleared its scan for foreground keymap reads and unrelated writes, while
   the session preserved the catalog ticket. That stranded “Finding saved
   macros…” and ongoing polling. Those foreground operations now run between
   catalog slots, then scanning resumes. Macro/archive writes, recording,
   host activity and disconnect retain their intentional cancellation rules.
   Failed writes also cancel the ticket and worker scan even when the selected
   macro editor was not Ready; late catalog results cannot validate uncertain
   state. The scanning label now appears only while a scan is actually pending.
2. **Retry scan allowed duplicate submissions.** Passive scans do not make
   the editor busy, so Retry stayed enabled during a retry. It is now disabled
   while a catalog ticket is pending; no new explanatory UI text was added.
3. **Collection identity was independently formatted twice.** Desktop discovery
   now uses the same typed HID identity projection as exact device selection.
   Labels remain outside identity; all six selection fields remain included.
4. **Getter errors omitted the failing transport phase.** Feature reads now
   distinguish sending the request from receiving the response, preserving
   the underlying error as their source. The existing macro slot/page context
   remains. No retry or timing was added.
5. **A research-only example no longer compiled.** `probe_macro_slot50` still
   called the removed runtime `stable_reads` helper. Its deliberate comparison
   of two diagnostic captures is now local to that research example. No repeated
   preflight was reintroduced into the desktop or CLI. The probe was compiled,
   not run against hardware.

## What remains unresolved

- The Linux macro verification failure and unavailable recovery are still a
  public pre-alpha blocker. The cleanup does not demonstrate a working save,
  assignment, playback or restoration on this unit. A bounded traced physical
  reproduction is needed before changing framing, pacing or recovery policy.
- A genuine catalog slot read error still fails the catalog and exposes Retry.
  Partial results are not promoted to a complete library or used to infer free
  slots. Fixing the scheduling hang does not fix an EPROTO from the device.
- The earlier native archive/picture recovery discrepancies remain separate
  evidence gaps. Public archive import/restore remains deferred.
- Windows official-app profiling remains deferred until interactive access is
  restored. Linux and Windows observations concern different physical units.

See [the original Linux failure audit](linux-macro-save-failure.md),
[macro boundary evidence](macro-boundary-audit.md), and
[Linux handoff](../docs/linux-handoff.md).

## Integrated validation

- `cargo test --workspace --all-targets --locked --offline`: 480 tests passed,
  including the legacy research baseline and current product crates.
- Strict workspace/all-target Clippy passed, both with default features and
  with `research-tools` enabled. The research/all-target compile check passed.
- Linux release desktop/CLI builds and Windows MSVC cross-target checks passed.
- Formatting and `git diff --check` passed.

New regressions cover wrong-feature/direction completions retaining the pending
operation, session JSON envelopes, catalog continuation through keymap work,
terminal scan failure followed by retry, failed writes canceling scans while
preserving macro drafts/diagnostics, all recovery classifications, backup failure
before setters, exact validated macro bytes, complete HID identity and getter
request/response error context. Existing codec, failure and workflow tests passed.

Local logs: `/tmp/byakko-architecture-{workspace-tests,workspace-clippy,build,windows-check,research-check,research-clippy}.log`.
After simplifying desktop payload matching, its 102 tests, strict Clippy,
release build and Windows check passed again; those logs use the prefix
`/tmp/byakko-architecture-final-desktop-`.
No physical acceptance is implied by these checks. The rebuilt Linux executable
is `target/release/byakko-desktop`; a running older process does not pick up the
new code until restarted.

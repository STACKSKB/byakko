# Configuration fault investigation

The normal multi-section round trip passed. The first injected-failure test
failed and must not be counted as proof of automatic recovery.

A research-only hook reported one error after delivering lighting setter07.
Earlier writes changed Pause, unbound macro49, picture91 and debounce. Recovery
reported no setter errors, but its final capture failed with Windows error995
(I/O aborted). A fresh complete capture found macro0 empty, picture9 red instead
of black, and Wave instead of Ripple. The test did not intend to edit macro0 or
picture9. The user confirmed no other app/task was deliberately configuring the
keyboard. Code review found no obvious packet mutation or index mixup. Cause is
unresolved; a firmware reset or specific host defect has not been established.

An explicit archive restore then failed on inconsistent picture reads and did
not verify recovery. After picture reads were strengthened to require two
consecutive matching captures within three attempts, a second explicit restore
succeeded. The complete original archive was restored and verified: both maps,
all50 macros, all128 picture colors, lighting and settings. No physical keys or
macro playback were used.

Ignored local evidence includes `Research/captures/configuration-first-complete.json`,
`configuration-after-fault-lighting.json` and `configuration-after-recovery-attempt.json`
in the same directory. Durable before-images include
`Research/captures/backups/configuration-before-1790054321767804900.json` and
`configuration-before-1790054625539803000.json` in that backup directory.

Hardening from this investigation:

- Failed setters wait one second before recovery, since an error does not prove
  the command was not delivered. Previously `?` skipped the normal settling delay.
- Recovery may retry a failed complete read once on a fresh handle under the
  same lock. No setter is retried; a state mismatch remains a failure.
- Picture reads reject oscillating or incomplete data, allowing one transitional read.
- Headless native UI tests cover progress, completion and close/error handling.
  Panic reporting explicitly says restoration is unverified.

The research example accepts a baseline path followed by `--fault-after-lighting`
or `--fault-after-macro-page`. The latter case has not been run. The hook is
thread-local, scoped, single-shot, limited to known setters and excluded from
normal builds. Its before/after delivery semantics have unit coverage.
Further fault injection is deferred until the unexplained changes are better
understood. These improvements do not prove that the original failure is fixed.

## Read-only stability follow-up (2026-09-22)

After the keymap backend separation and macro editor lifecycle changes, a fresh
`capture-configuration` completed two matching full sweeps without setters.
The resulting ignored local archive
`Research/captures/configuration-stability-after-macro-ui.json` is byte-for-byte
identical to `configuration-first-complete.json`. Both SHA-256 hashes are
`2137480f0ba425bf06c9ef9a37881096834c928f6d4c21208ed65daf210ba4d4`.
The bidirectional configuration planner reports zero changed bindings, macros,
picture colors, lighting or settings. Thus the restored baseline remained
intact during this interval. This narrows the investigation to the earlier
transaction/recovery circumstances; it does not identify their cause or prove
that repeating fault injection is safe.

## Unreadable-keymap recovery fallback

A later code review found that recovery discarded a failed keymap reread with
`.ok()` and treated every writable slot as different. That could issue 126
setters per unreadable layer, including unrelated keys the transaction never
intended to change. It is not established that this branch ran during the
recorded failure, and this is not a root-cause claim.

Recovery now uses observed differences when a complete map is available. When
it is unavailable, it restores only slots differing between the attempted and
original maps. Read failures are included in diagnostics if final recovery
verification fails. Invalid shapes or reserved-slot differences are rejected;
full-archive comparison still decides whether recovery succeeded. Pure tests
cover zero-change and one-change unreadable maps, unexpected observed changes,
and invalid/reserved data. No fault injection or device setters were run for
this change; the hardware recovery acceptance gate remains open.

The subsequent complexity review found the same broad fallback still present
in standalone `apply_keymaps`, separate from archive restoration. That path now
uses the same pure slot-selection function. Its ordered layer loop still reads
both maps before restoring Fn, reads both again before restoring base, and
requires a complete final snapshot equal to the original. A failed intermediate
read limits writes to that layer's planned changes; it does not establish that
unrelated slots are correct. Final verification remains mandatory. The four
device-free planner tests pass; this change has not been fault-injected on hardware.

## Research setter trace

Fault-mode runs now reserve a new JSON trace file before writing and collect
setter attempts in memory throughout the transaction and recovery. Each entry
contains the complete host report, sequence, elapsed microseconds, and outcome:
transport success/error, injected before/after transmission, or unfinished after
a panic. Transport success means the API returned success, not proof that firmware
applied the report. This is not a USB bus capture and does not record getter replies.

The trace is thread-local, limited to 4,096 entries with a dropped-entry count,
and only available with `research-tools`. Nested scopes are rejected without
resetting the outer trace. Panics preserve unfinished entries and clear the scope.
The example saves and syncs the trace before evaluating the recovery result;
process termination or a failed file write can still lose this in-memory evidence.
There is no disk I/O in the setter wrapper. Mocked tests cover normal and failed
transmission, both injection modes, panic cleanup, nesting and isolation. No
additional hardware fault injection was performed for this diagnostic change.

## Native editor result checks

The picture, lighting and scalar-settings editors now check successful worker
results against the requested draft before replacing their loaded state. Picture
comparison covers all128 RGB triples. Lighting compares canonical setter reports
so the documented white sentinel is accepted without accepting a different
effect or value. Scalar settings compare the applied value with both the worker
result and current draft; unrelated settings drafts remain staged.

Unexpected results retain the old baseline and draft, mark the state unverified,
and require another read before applying. Device-free worker-message tests cover
these cases and accepted matching results. The transport layer still performs
the full reserved-byte/readback checks; these UI checks neither establish USB
recovery nor identify the cause of the earlier hardware failure.
## Recovery coverage audit after state separation

A read-only review found no concrete report-lifetime, slot-index, or handle-drop
error explaining the recorded collateral changes. It did find a coverage limit:
archive recovery restores only planned macro slots, picture colors, settings,
and lighting. Unlike keymap recovery, these sections do not select repairs from
observed differences. Thus an unexpected change to an unplanned macro 0 or
picture 9 survives that recovery pass. Full final comparison detects the
mismatch and reports recovery as unverified; it must not be treated as success.

This is not a root-cause finding. Expanding recovery writes without reliable
observations would also enlarge the failure surface. The next hardware audit
must capture getter replies alongside the bounded setter trace, distinguishing
an unstable read from persistent collateral state before changing recovery
policy. No device reads, setters, or fault injection were performed for this audit.

## Getter diagnostics

Research traces now include `read_payload` exchanges in the same ordered,
thread-local buffer as setter attempts. The combined limit is 16,384 events
to retain several complete archive sweeps, including identity barriers. Each getter records its
request and the complete initialized 65-byte reply buffer, plus the returned
length when the exchange succeeds. A failed exchange retains any partial bytes;
these are diagnostic buffer contents, not a valid reply. Length/prefix validation
still happens in the transport after tracing, so malformed responses remain
available for analysis. Transport success alone does not establish protocol
validity or stable device state.

The example artifacts use `byakko-research-transport-trace`, version 2, and
`operation` distinguishes getters from setters. Existing version-1 setter-only
artifacts remain unchanged. Direct inspection requests outside `read_payload`
are not traced. The recorder performs no disk I/O and does not change request
order, request bytes, or transport delays. Getter requests are never passed to
the setter fault injector. The shared cap reports dropped events explicitly;
an incomplete trace cannot prove that an unrecorded operation did not occur.

Mocked exchanges cover success, partial-buffer errors, malformed lengths,
panics, and mixed getter/setter ordering. This instrumentation has not yet been
used for another hardware fault injection; recovery acceptance remains open.

## Live read-only trace baseline

The new `capture_configuration_trace` example completed against the attached
Nia87. It captured two matching complete configurations: both maps, all 50
macro slots, picture, lighting, and settings. The trace contains 1,952 getter
exchanges, all with transport success, returned length 65 and report ID zero;
zero events were dropped and no setters were recorded or called by this path.

Local artifacts (ignored capture data):

- `Research/captures/configuration-getter-trace-baseline.json`:
  SHA-256 `2137480f0ba425bf06c9ef9a37881096834c928f6d4c21208ed65daf210ba4d4`.
- `Research/captures/configuration-getter-trace-baseline-transport.json`:
  SHA-256 `d3b88f454bbf0782c1cb3b51b13105b361a1d8cf33f7e726077e30c7b814cafa`.

The archive is byte-identical to `configuration-before-macro-events.json`.
Bidirectional planning reports zero differences in every section. This verifies
the live getter instrumentation and continued baseline stability; it does not
establish recovery under a transport fault.

Repeat with distinct, unused output paths:

```text
cargo run --no-default-features --features research-tools --example capture_configuration_trace -- NEW_ARCHIVE.json NEW_TRACE.json
```

The example reserves the trace path before USB I/O, saves diagnostics even if
capture fails or panics, and saves an archive only after two full captures match.
Both outputs refuse overwrites. If the archive path aliases the reserved trace,
the command stops before USB I/O and retains the empty reserved trace file.

## 2026-09-25 offline evidence reconciliation

At the user's request the Windows Agent compared the original ignored files
without opening a device or changing them. The Linux checkout does not contain
those files; the following is the agent's reported byte comparison, not an
independently replayed USB trace. A is `configuration-first-complete.json`, B is
`configuration-after-fault-lighting.json`, and C is
`configuration-after-recovery-attempt.json`, all under `Research/captures`.
Indices are zero-based; values are hexadecimal.

| JSON byte path | A | B | C |
| --- | --- | --- | --- |
| `/macros/0/0` | 0F | 00 | FF |
| `/macros/0/2` | 04 | 00 | 00 |
| `/macros/0/3` | 80 | 00 | 00 |
| `/macros/0/4` | F2 | 00 | 00 |
| `/macros/0/5` | 01 | 00 | 00 |
| `/macros/0/6` | 04 | 00 | 00 |
| `/macros/0/8` | F2 | 00 | 00 |
| `/macros/0/9` | 01 | 00 | 00 |
| `/lighting/raw/1` | 05 | 04 | 04 |
| `/lighting/raw/2` | 04 | 02 | 02 |
| `/lighting/raw/4` | 07 | 08 | 08 |
| `/lighting/raw/5` | 08 | FF | B4 |
| `/lighting/raw/6` | 08 | FF | B4 |
| `/lighting/raw/7` | 08 | FF | B4 |
| `/picture/9/0` | 00 | FF | FF |

Both keymaps, macro slots 1–49, all other picture bytes, all settings (including
reserved bytes) and metadata match. A→B and A→C each have 15 changed scalar byte
leaves; B→C has four. Slot 0 in A begins
`0F 00 04 80 F2 01 04 00 F2 01` then 246 zero bytes; B is all zero; C begins
`FF` then 255 zero bytes. The complete lighting prefixes are
`87 05 04 04 07 08 08 08`, `87 04 02 04 08 FF FF FF`, and
`87 04 02 04 08 B4 B4 B4`, each followed by 56 zeros. `87` is the response
opcode, not a host report-ID byte.

| File | Bytes | SHA-256 |
| --- | ---: | --- |
| A | 139657 | `2137480f0ba425bf06c9ef9a37881096834c928f6d4c21208ed65daf210ba4d4` |
| B | 139658 | `3e190a04d21e62372fc51977aa9619844ac038b73c2ef0a8a22a61701b92545e` |
| C | 139660 | `8386ba883037c175c22f3e2895023cd12560cf1cba0132cbd209d182d6ca5c92` |

The backup files ending `1790054321767804900`, `1790054625539803000`, and
`1790054976352601200` respectively match A/B/C by complete-file hash.
No transport trace of that fault run was found. The 1,952-getter baseline trace
and the later macro-event setter trace are separate runs; they cannot supply
the missing fault-run ordering. Tracing was added after the fault test.

This confirms C differs from A. Picture bytes were captured under differing
lighting contexts, so their difference alone does not prove persistent damage
to one picture bank. The macro and lighting changes remain unexplained.
Snapshots alone cannot distinguish persistent firmware changes, transitional
reads, selector-dependent responses or host transport defects. No root cause
is claimed and no fault test was repeated. Before a new experiment, review the
current baseline and propose a bounded trace-producing test to the user; do not
reuse the historical multi-section fixture blindly.

## New Linux mismatch during an ordinary explicit restore (2026-09-25)

See [supervised Linux test](../docs/linux-handoff.md#supervised-lighting-check-and-restoration-2026-09-25).
There was no injected fault. Restoring the original Wave/rainbow lighting bytes
from steady green planned a single lighting setter and no other section writes.
The complete target readback mismatched; automatic rollback then fully verified
its own before-image (steady green at brightness 1). The original visible
settings were subsequently restored through the normal lighting command at the
user's direction, with the codec's canonical rainbow RGB bytes retained.

The original raw target used `FF FF FF`; ordinary encoding uses `FA FF FA`.
This is a candidate factor to inspect, not an established cause of the full
archive mismatch. No trace or mismatched snapshot was retained by that restore
path, so do not infer its actual failing section. This narrow Verified recovery
is new Linux evidence, not proof that the old multi-section Windows failure is
fixed. Preserve diagnostics before proposing any further write experiment.

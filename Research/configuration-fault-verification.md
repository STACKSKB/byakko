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

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

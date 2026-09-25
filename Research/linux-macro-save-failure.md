# Linux macro save failure audit (2026-09-25)

The user reported two Save & assign failures on the Linux Nia87. Both showed
`Protocol error (os error 71)` and automatic recovery reported
`No such device (os error 19)`, classified as Unverified. The user clarified
that they unplugged and replugged **after** the save/assignment failure.
This does not establish whether the device also reset itself during the write.

## Findings

The macro save fails before the assignment stage: the desktop only starts
the keymap assignment after a verified macro save. Unverified recovery
invalidates other feature baselines because device state is uncertain. That
explains why lighting editing is blocked; enabling writes against those
baselines would conceal the failure.

There was a separate recovery UI defect. After discovery marks the device
missing, cautious reconnect requires a manual read, but the lighting retry
button was disabled while disconnected. The shell now exposes one Reconnect
button on every page while disconnected and idle. It uses the existing manual
refresh path: retire the old executor, bind the current unique supported
collection, advance the generation and read the initial features. Redundant
disconnected page retry buttons are hidden. The failed macro draft and its
diagnostic remain available; reconnect does not silently retry its write.

The macro transport audit found no established malformed packet. Current
encoding replaces all 256 logical bytes using five padded pages, with 30 ms
between pages and 200 ms settling. Earlier Windows long-to-short/empty tests
support full replacement because variable-length writes left stale tails;
see [boundary audit](macro-boundary-audit.md). That evidence comes from another
unit and does not establish this Linux write's success. Rollback uses the
same selected handle; an unavailable device can therefore also prevent
restoration. Exact-target selection remains mandatory.

Previously the error did not identify which page or whether a setter or
verification getter failed. Macro errors now include the zero-based slot,
read/write direction and page. No report bytes, pacing, read counts or
recovery policy were changed. The original transport failure remains
unresolved pending a trace of a failing operation; no new hardware setters
were performed during this audit.

## Read-only evidence after replug

Both backups are slot 0 with 256 zero bytes:

- `macro-0-before-1790336679834337227.json`
- `macro-0-before-1790336760934463162.json`

They are under `/home/three/.local/share/byakko/backups`, each 1824 bytes,
SHA-256 `9357e9d3ec447c4d2dba5de02b9a270498f91d4b1fb8e531b67e4fd9a684e0c2`.

Normal-user host CLI reads found `/dev/hidraw2` and succeeded for slot 0,
keymap and lighting. Slot 0 matches both empty backups. The keymap matches
the earlier Linux baseline (firmware 0x0100, profile 0), SHA-256
`75b94b74a58f5f012ada73b19763469eba22a8cab50b2ec1ded9278de8d8952f`.
Lighting reads Wave/rainbow, brightness 4, speed 2, right. Its JSON hash is
`ee3a8262010578c60ab498ceb6ae647f20ec0cd96649b9a1e5d6eef411c0c8df`.
There is no immediate pre-failure lighting snapshot establishing that its
exact bytes were unchanged by these failures.

Local stdout/stderr files are in `/tmp/byakko-macro-failure-audit` and are
not tracked. Kernel journal access was unavailable to the current user;
noninteractive sudo required a password. No conclusion about kernel USB
events can be drawn from that unavailable log.

## Validation and remaining acceptance

Device tests: 201 unit plus one integration test passed. Desktop tests:
101 library plus one binary test passed. The new regression drives failed
macro save, device disappearance and the global Reconnect action, confirming
fresh keymap/lighting/settings baselines and retention of the failed macro
draft and diagnostic. Strict Clippy passed for both crates and all targets.
Linux desktop/CLI release builds and the Windows MSVC cross-target desktop
check passed. These are not physical macro write or Windows runtime evidence.

Before public release, reproduce and diagnose the Linux macro transport
failure with bounded, backed-up writes and transport evidence; verify macro
save, assignment, playback and restoration on this unit. Also exercise the
rebuilt desktop's reconnect action after a physical replug. The present audit
fixes the inaccessible recovery action, not the underlying USB failure.

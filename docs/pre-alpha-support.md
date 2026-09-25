# Pre-alpha support and recovery

Current scope: source-only native Rust/Iced desktop and separate CLI, Windows
and Linux, Nia87 stock firmware over USB. The observed board uses firmware
`0x0100`, profile 0, VID/PID `3151:4015`, and the validated configuration HID
collection. Matching USB IDs alone do not establish another board's write
compatibility. QMK/VIA, 2.4 GHz configuration, macOS and browser delivery remain
outside this pre-alpha. Current UI text is English.

## What sends a write

Keys and macros retain explicit staged saves. Lighting controls automatically
send choices; RGB/picture edits coalesce while idle and after picker release.
Clicking a key in per-key mode paints it with the retained brush. Settings queue
one field per transaction. The default idle delay is 2000 ms; see
[desktop configuration](desktop-configuration.md). Navigation itself sends no
read; connection initialization loads scalar features once and passively scans
macro slots. Selecting an editable macro reads that slot.

Before a setter, the native transaction saves its cached before-image to a new
backup file. Keymaps, macros and settings verify affected-feature readback.
Ordinary lighting and picture uploads report **transport acceptance**, not
verified firmware persistence. Explicit later reads carry readback evidence.
Host-lighting restoration and archive apply still require verified readback.
A changed lighting selector invalidates dependent picture data.

## Evidence and remaining limits

| Area | Recorded acceptance | Still to verify |
| --- | --- | --- |
| Keymap, macro, settings | Windows reversible native/CLI writes and full readback; Linux read-only baselines | Broader physical key/macro playback, timing, sleep behavior, persistence; Linux writes and final GUI behavior |
| Ordinary lighting/picture | Windows physical presets, steady/hue response, retained brush; twelve consecutive bulk uploads and later full picture readback | Interrupted-upload recovery, broader effect/selector behavior, persistence and Linux physical writes |
| Native archives | Full captures and normal round trips; final comparison detects mismatches | Cause of the earlier collateral changes and failed automatic recovery; current Iced physical apply |
| Host screen/music | Headless lifecycle plus earlier research-backend physical evidence | Current Iced streaming, focus/close/disconnect restoration and Linux routing/runtime |
| Linux GUI | Earlier X11 demo startup | Current rendered interaction and Wayland startup; screen capture is X11-only |

See [parity ledger](parity-status.md), [Linux handoff](linux-handoff.md), and the
[2026-09-24 picture evidence](../Research/official-picture-capture-20260924.md)
for exact dated sources. Earlier research-app behavior is not automatically
acceptance of the Iced UI. No blanket efficiency or complete-parity claim is made.

## If a write fails

1. Keep the error text and backup path. A failure does not mean no report reached
   the keyboard. Do not repeatedly click Save or run unrelated setters.
2. A **Verified** recovery means the affected before-image was read back and
   matched. The original save still failed; retain the draft and inspect the error.
3. **Failed** means recovery readback proved a mismatch. **Unverified** means
   the application cannot establish the resulting state. Neither means saved.
   **NotAttempted** means the transaction did not attempt recovery; inspect the
   reported preflight/backup error.
4. Preserve the durable backup and obtain an explicit read after the connection
   is usable. Do not stage another edit from an uncertain baseline. If a read
   fails, retain the failure and stop; ask for help with the exact artifacts.
5. For native macro backups, the CLI has `plan-restore-macro` and `restore-macro`.
   Other feature backups are diagnostic/native formats, not interchangeable
   editable snapshot files. Do not feed them to an unrelated apply command.

Backups live in the `backups` subdirectory of the normal-user data location:
`%LOCALAPPDATA%\Byakko` on Windows, or `$XDG_DATA_HOME/byakko` (falling back to
`~/.local/share/byakko`) on Linux. Full locations and fallbacks are in
[local storage](local-storage.md). Backup/export files are not overwritten.

The known archive fault changed an unplanned macro and picture slot; a later
explicit restore verified, but automatic recovery's cause/coverage remains
unresolved. Keep [that evidence](../Research/configuration-fault-verification.md)
open. Do not repeat the old multi-section fault example unattended or assume it
matches your current baseline. Any new fault test needs a reviewed before-image,
exact planned writes, retained trace and an agreed recovery procedure.

## Reporting a problem

Provide the source revision, OS/display session, firmware if known, feature and
exact actions, error text, and whether the result was transport-accepted or
readback-verified. Build failures should include the build command and toolchain.
For device failures, retain before/after snapshots and the named backup locally.
Review them before sharing: macro/keymap files can contain personal content.
Attach only the relevant evidence to the conversation with the maintainer; no
telemetry, account or automatic report submission is involved.

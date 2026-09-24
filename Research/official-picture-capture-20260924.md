# Nia87 official picture-write evidence, 2026-09-24

This note separates the captured official behavior from Byakko's physical
acceptance. The reference was Nia87 Driver 2.1.97, observed through the local
3815-to-3814 RPC proxy and an x86 CDB breakpoint at the helper's
`HidD_SetFeature`. The debugger recorded the final HID buffers (report ID,
checksum, and two trailing zero bytes in a length-67 call). Instrumentation
and RPC forwarding affect timing; packet intervals below are not an
uninstrumented performance measurement. Vendor source was inspected for
behavior only, not copied into the product.

## What happened on the keyboard

The earlier official-app RGB session captured eleven complete picture uploads.
The user saw C change physically through red, green, blue, and later colors;
the captures include successive images differing only at matrix slot 28 (C).
Every edit still uploaded the complete image. A later independent CLI getter
matched all 128 RGB entries of the final captured image (C was
`[192,130,11]`) under selector context `[13,0]`. That late agreement does not
establish immediate getter timing or power-cycle persistence. Evidence:
`Research/captures/official-rgb-20260924.log`, `rpc-rgb-final.json`, the
`rpc-179026817*-*.bin` pairs, and `official-rgb-after-read.json`.

An initial Byakko bulk-C experiment had a different outcome. The user did not
see the intended physical change, and the immediate picture getter returned
the old value, so the then-current verified transaction treated the write as
a mismatch and attempted rollback. A later getter returned the new desired C
value `[32,160,224]` at slot 28, rather than the original `[192,130,11]`.
`Research/captures/bulk-C-test.json` and `bulk-C-after-failure.json` retain
the before and later snapshots. This is evidence of delayed visibility and of
an unsuccessful immediate verification/rollback conclusion in that run; it
does not identify whether firmware buffering, timing, or another condition
caused the first observation. The user later factory-reset the keyboard before
the official app detected it successfully. The reset's necessity and relation
to the bulk-C mismatch are unknown.

A subsequent, more controlled official session separated editing from saving.
Changing F1 to red in Light Edit sent no HID command; **Save to Layer 1** sent
one `0x07` picture-effect selection followed by seven `0x0c` pages. Explicit
**Load Layer 1** sent six `0x8c` getter requests, each with a `readMsg`.
Changing F1 to green locally again sent no HID command; the second Save sent
seven `0x0c` pages with no preceding selector write. Neither Save sent a
picture GET or rollback after its seven setters. The user observed the saved
F1 color on the physical keyboard. The path-free ordered fixture is
`Research/captures/official-2026-09-24-hid-sequence.json`; the original RPC
request files and `official-reset-profile.log` remain under
`Research/captures/`. A later camera frame
`keyboard-camera-1790271217889727000.png` shows a red physical F1 after one
rebuilt Byakko seven-page submission. This is one successful physical color
observation, not acceptance of repeated edits, all keys, persistence, or
failure recovery. Rapid-change stress remains pending.

## Packets and scheduling

Each official Save used seven bulk `0x0c` reports, not the `0x14` single-key
setter. The 64-byte payload has opcode `0x0c` at byte 0, profile 0 at byte 1,
little-endian picture length 384 (`80 01`) at bytes 2–3, page 0–6 at byte 4,
zero at bytes 5–6, a BIT7 checksum at byte 7, and 56 RGB-stream bytes at
bytes 8–63. The last page pads eight bytes with zero. The 384-byte stream is
128 matrix-indexed RGB triples. Even a single-key edit sends the full image.
The helper's HID call included the report ID and two trailing zeros; those
are outside the 64-byte payload. The final HID page headers in
`official-reset-profile.log` occur at lines 446–512 for the first Save and
1157–1223 for the second, typically about 40–70 ms apart under instrumentation.

The exact Nia inheritance path in the local `main_ccea61a6.js` bundle is
`Pft` (character offset 10,719,888) → `CHe` (9,980,682) → `PB` (7,729,263)
→ `rB` (7,707,646) → `UD` (7,688,808). `UD._setLightPic` at 7,695,306
awaits seven `writeFeatureCmd(page, BIT7, LITTLECMDDELAY)` calls and then
**awaits `vp(BIGCMDDELAY)` before returning success**. The inherited
`LITTLECMDDELAY` is 20 ms near 7,662,505; `PB` overrides `BIGCMDDELAY` to
**1000 ms** near 7,730,326. The generic `writeFeatureCmd` near 7,553,507
waits its delay argument before the RPC send. The RPC wrapper near 2,630,897
also schedules a 10 ms wait. These source waits explain a nominal pre-page
schedule but do not turn the debugger's packet intervals into a USB timing
benchmark. The final 1000 ms wait happens after the seventh send, so packet
spacing alone cannot confirm it.

An earlier reading of `_setLightPic` near character 7,647,940 followed a
different class branch and incorrectly concluded that Nia had **no final
settling wait**. That conclusion is withdrawn. `CHe.setLightPicSimple` near
9,986,904 defines the separate `0x14` setter, but this Save handler used the
inherited seven-page path. The Light Edit upload handler near 16,626,898
selects picture mode when needed, invokes bulk `setLightPic`, and has no
automatic picture GET; its explicit load handler is near 16,628,187.

## Other captured UI behavior and Byakko boundary

The same ordered session recorded standalone global-lighting choices as
individual `0x07` sends (Always On, fixed mode, red preset, and a completed
hue drag), with no following `readMsg`. `PB.setLightSetting` near 7,743,227
uses `writeFeatureCmd(report, BIT8, 500)`, then awaits its inherited
`COMMONDELAY=500` ms: the generic call waits 500 ms **before** sending and
the method waits another 500 ms **after** successful transport. The captured
two sensitivity changes, 1→2→1, sent two `0x11` setters with no getters.
Entering Macros, creating a local macro, and visiting Main Other Settings
sent no HID requests in this session; macro files are loaded from the app's
local `APP_macro` database (`getAPPMacroList` near 15,883,211). These are
bounded observations of those UI actions, not a claim that those features
never read the device. Startup did include eight send/read pairs, and the
explicit picture Load included six.

The rebuilt Byakko picture path in
`crates/byakko-devices/src/nia87/device/picture.rs` now backs up the cached
before-image and sends seven complete `0x0c` reports in one serialized
operation. A successful completion means **transport accepted all seven
reports**. It no longer performs an immediate GET or automatic rollback after
successful submission, avoiding the specific early-getter conclusion seen in
the failed bulk-C run. Transport failure remains explicit and points to the
backup. At this note's revision it schedules 20 ms before each page and does
not reproduce the official method's inherited final 1000 ms wait. Its
transport-accepted result is therefore not evidence of verified firmware
readback or complete timing parity. Explicit later reads and physical checks
remain separate acceptance steps. The original failed-recovery evidence is
still relevant; this is not evidence of power-cycle persistence or recovery.

## Subsequent rebuilt Byakko acceptance

The rebuilt desktop preset controls changed physical F1 to green and then blue,
matching the UI, without a failure banner. Webcam stills
`keyboard-camera-1790271615439147800.png` and
`keyboard-camera-1790271691281526400.png` record those observations.

The opt-in `exercise_picture --write-f1` research example then submitted twelve
consecutive complete pictures through one bound adapter, cycling red/green/blue
on F1, using each accepted result as the next baseline. There were no inserted
reads or final settling waits between uploads. The local transport trace records
84 setters, all transport-successful, and zero getters; the sequence took
5.976 seconds including backups, HID opens and the existing per-page pacing.
The final blue F1 is visible in `keyboard-camera-1790271821065867700.png`.
A separate later `read-colors` matched all 128 RGB entries of the final accepted
frame. These files are in `Research/captures/consecutive-f1-20260924/`.
This is bounded evidence that consecutive uploads work on this board without
adding the official method's final one-second wait, not a general timing guarantee.

Ordinary global-lighting writes now likewise return transport-accepted evidence
after one setter and the existing 500 ms post-send scheduling, without a getter
or speculative rollback. Host-mode start and restoration retain their distinct
verified path. Both snapshot types distinguish transport acceptance from a
device read, including in serialized CLI output.

The final rebuilt desktop also passed a physical steady-effect check: choosing
Always On lit the F-row, choosing the red preset made it red, and completing a
hue drag changed it to blue/violet without a failure banner. Webcam stills
`keyboard-camera-1790272463154750500.png` and
`keyboard-camera-1790272504016735600.png` preserve the latter two results.
Camera white balance is not a colorimeter; this establishes visible response,
not exact optical RGB calibration. The app was left on this tested steady effect.

The broader read audit found no remaining duplicate prewrite read in ordinary
keymap or scalar-setting apply. Those paths retain one post-write verification
of their feature (18 responses for keymap, four for settings). Keymap setter
pacing has separate recorded cross-layer-write evidence and was not altered
based on the lighting trace. This session did not physically profile keymap or
macro programming, and does not claim those workflows have been rebuilt.

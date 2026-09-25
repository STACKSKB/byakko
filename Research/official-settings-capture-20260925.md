# Official Nia87 settings profile, 2026-09-25

This is a bounded behavioral reference for the connected Nia87 Driver 2.1.97
session, not a prescription to remove Byakko's verification. During authorized UI profiling we changed
automatic OS detection off → on → off, Bluetooth backlight-off time 2 → 3 → 2
minutes, and debounce 1 → 2 → 1. The official UI displayed “Loading in
progress” after sleep and debounce changes. The capture was made through the
existing localhost 3815-to-3814 gRPC-Web proxy; it was not an uninstrumented
USB timing measurement.

## Captured requests and readback boundary

`Research/captures/settings-profile-final.json` analyzes the main connection
`rpc-1790306417677545900-4-{request,response}.bin`: 42 complete request and
42 response messages, including 13 `sendMsg` and eight `readMsg` HID RPCs.
The analyzer reports no trailing request or response bytes, and the listed HID
RPCs completed with gRPC status 0. One initial `0x80` response appears before
the seven captured baseline send/read pairs; the paired baseline requests are
`0x84`, `0x85`, `0x87`, `0x86`, `0x91`, `0x92`, and `0x97`. The six later settings
setters, in observed order, were:

| UI action | RPC exchange | Payload prefix | Observed follow-up |
| --- | ---: | --- | --- |
| Auto OS on | 24 | `17 01` | No settings GET |
| Auto OS off | 27 | `17 00` | No settings GET |
| Bluetooth backlight off at 3 min | 30 | `12 00 00 00 00 00 00 00 b4 00 78 00 58 02 58 02` | No settings GET |
| Bluetooth backlight off at 2 min | 33 | `12 00 00 00 00 00 00 00 78 00 78 00 58 02 58 02` | No settings GET |
| Debounce 2 | 36 | `11 00 02` | No settings GET |
| Debounce 1 | 39 | `11 00 01` | No settings GET |

These prefixes are the 64-byte feature payload before any host report-ID byte.
The `0x12` values encode seconds, so `b4 00` is 180 and `78 00` is 120;
the other three timers stayed at 120, 600, and 600 seconds. The absence of a
GET means this official UI flow accepted setter transport success and updated
its local state without immediate device verification. It does not prove
power-cycle persistence, recovery behavior, or that every settings action in
the app follows this pattern. An independent Byakko read-only snapshot before
and after the exercise, respectively
`settings-profile-before-20260925.json` and
`settings-profile-after-20260925.json`, has byte-for-byte identical raw
revisions and document contents. This supports restoration of the tested
baseline at the later read time, not per-step readback.

## UI and selected source path

Character offsets below refer to the locally extracted
`Research/extracted/official-app/resources/app/dist/static/js/main_ccea61a6.js`.
The Nia87 registry entry near 1,967,519 selects `Pft` (10,719,888), inheriting
through `CHe` (9,980,682) and `PB` (7,729,263). Its `Iu` settings descriptor
near 1,598,318 advertises auto OS, debounce, and Bluetooth/2.4 GHz normal and
deep sleep ranges.

The settings panel near 17,269,551 keeps debounce slider movement in local
React state; `setValueFinish` near 17,270,800 calls `setDebounce`. The
descriptor-dependent low-end choice can show a confirmation first. The auto-OS
checkbox calls `setAutoOsen` on click near 17,271,400. Sleep controls near
17,255,161–17,260,346 call `setSleepTime` on input completion or checkbox
click. The panel also exposes a separate “Read current configuration” button
near 17,272,200; the six setter actions above did not invoke it.

The device store's `setDeBounce` (~15,698,500), `setAutoOsen` (~15,700,030),
and `setSleepTime` (~15,701,000) pass the setter through `doAsync`, then
update the corresponding local `other` value on success. The sleep store
skips an unchanged object. `doAsync` near 15,771,314 starts each Promise
immediately, tracks a busy sender, and clears that sender on completion. The
root renders an absolute, high-z-index loading overlay while busy
(~17,661,777; inserted near 17,787,018). It appears to cover ordinary pointer
controls during a write; the store itself has no explicit pending-edit queue
or reject-while-busy guard. Actual hit-testing and overlapping programmatic
calls were not measured in this profile.

On the Nia class path, `PB.setDeBounce` near 7,732,253 writes `0x11` with BIT7
mode and awaits `COMMONDELAY=500` ms after transport success.
`PB.setSleepTime` near 7,734,026 writes `0x12` with four little-endian
second values at payload bytes 8–15, then awaits the same 500 ms.
`CHe.setAutoOsen` near 9,983,779 writes `0x17` with BIT7 mode and returns
after transport completion, without that extra common delay. The generic
`writeFeatureCmd` near 7,553,507 defaults to a 10 ms pre-send wait, and its
RPC wrapper near 2,630,897 adds another 10 ms before invoking it. These are
source-level scheduling values; they do not establish actual USB completion
latency or a safe minimum for Byakko.

`PB.setKeyboardOption` near 7,730,412 defines the `0x06` keyboard-option
setter, but no shared Nia settings UI call to `currentDev.setKeyboardOption`
was found in this bundle. The visible light panel reads keyboard-option state
to disable lighting controls when the backlight is off. The captured Main
Other Settings navigation itself sent no HID command. Thus this profile does
not establish the official UI's backlight-toggle write behavior; the tested
Bluetooth backlight-off timer is the separate `0x12` sleep field.

## Byakko implementation boundary

Byakko's current desktop settings queue (`crates/byakko-desktop/src/settings.rs`)
keeps the latest pending value per field and projects it while a one-field
transaction is active. The native `Config` default is a configurable 350 ms
quiet interval (`crates/byakko-desktop/src/config.rs`). The serialized executor
continues to apply one setting at a time through the existing Nia transaction,
which backs up the cached before-image and performs one four-response settings
verification sweep after each write (`crates/byakko-devices/src/nia87/device/settings.rs`).
That verification deliberately differs from the captured official no-GET UI
flow. No new settings opcode, report layout, or unverified transport-success
claim is inferred from this profile. The live queue's responsiveness and
failure handling require their own acceptance checks.

## Desktop acceptance in this change

The rebuilt native review window was inspected through the user-positioned
webcam. Settings appeared as aligned compact rows with bounded sliders; the
lighting selector appeared above the picker and the keyboard stayed in place
when switching pages. In per-key mode, selecting F2 and choosing green changed
its physical LED. Clicking F3 then painted it green without touching the picker;
the camera showed both F2 and F3 green and the brush still green. This verifies
the retained-brush physical path, not subsecond batch timing. Deterministic
tests cover the idle deadline restarting, multiple painted keys in one batch,
and settings edited while an earlier write is active. The existing release
window had unsaved changes and was preserved; the reviewed executable is
`target/ux-review/release/byakko-desktop.exe`.

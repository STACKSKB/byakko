# Live Nia87 evidence

Research date: 2026-09-21. Windows host, user reports wired USB.

## Descriptor

The native enumerator found `3151:4015`, interface 2, usage `FFFF:0002`, manufacturer `ROYUAN`, product `Gaming Keyboard`. HIDAPI's Windows-native backend reconstructed this report descriptor from Windows preparsed data:

```text
06 ff ff 09 02 a1 01 09 02 15 80 25 7f 75 08 95 40 b1 02 c0
```

It describes one unnumbered 64-byte Feature report. HIDAPI's host buffer has a leading zero report-ID byte (65 bytes total).

## Version and profile

The original native implementation sent only known version/profile read opcodes. A zero-filled version request failed response validation, returning stale opcode `97`. The official app was running at that point, so the failure alone does not establish causality.

After closing the official GUI and stopping the helper it had launched, adding a BIT7 checksum produced fresh matching replies for two different opcodes:

| Operation | Request prefix (remaining bytes zero) | Response prefix |
| --- | --- | --- |
| Read version | `80 00 00 00 00 00 00 7f` | `80 00 01 00 00 00 00 7f` |
| Read profile | `85 00 00 00 00 00 00 7a` | `85 00 00 00 00 00 00 7a` |

Interpretation from the Nia87 vendor call chain: little-endian raw version `0x0100`, active profile index `0`. The checksum hypothesis is byte 7 = complement of the wrapping sum of bytes 0–6. Replies preserve the request checksum instead of recomputing it over returned data; generic response checksum validation would reject legitimate version data.

Raw local records: `Research/captures/identity-bit7-read.json` (private, ignored). No settings were changed by these requests.

## Official application observation

The supplied extracted GUI recognizes `Nia87`, `USB`, and configuration `Nia87_1`. Its accessibility tree exposes Key Setting, Other Setting, Combination, Macro, Media, and Mouse. This establishes categories, not end-to-end verification of our implementation. The official app started a helper under the user's roaming profile; that helper was stopped before direct native read testing to avoid competing HID transactions.

## Verification limits

The user is AFK. Physical key press/output behavior, key releases, macro timing and visible onboard lighting still require stronger evidence than matching configuration readback. Do not count those requirements as verified merely because an Apply operation is acknowledged.

## Keymap milestone

Both base (`89`) and Fn (`90`) maps were read twice, 128 slots each; duplicate snapshots matched. The initial base was compared with the Nia87-specific default matrix in the supplied package: zero differing slots. The application board profile was generated from our live base-map observations, not from Sharkfin or a copied vendor source table. It remains independent of user remappings.

Using the Nia87-specific single-key command (`13`), the native test changed Pause at slot 91 from `00 00 48 00` to F24 (`00 00 73 00`). Complete reads of base and Fn maps confirmed that this was the only change. A second backed-up transaction restored the initial value, and complete reads matched the initial snapshot exactly. Backups were flushed to disk before each write. This proves configuration write/readback and restoration, not physical key output or power-cycle persistence.

## Macro storage milestone

The inherited Nia87 macro read command is `8b`, not the separate shared `96` implementation. Slot 49 was read twice using four raw 64-byte pages, with an intervening version read to reject stale identity responses. All 256 bytes were zero, and no key in either layer referenced slot 49.

The native writer stored repeat count 1, F24 down with 50 ms delay, and F24 up with 50 ms delay using command `16` and BIT7 framing. A repeated full read matched the encoded 256 bytes exactly. A second transaction restored the all-zero original macro. Complete keymap reads after the test matched the original base/Fn maps. No key was bound to the macro and no playback was triggered. This validates short macro storage and restoration only; multi-page truncation, playback timing, modes and mouse movement need further checks.

## Extended macro storage verification

A 242-byte stream (60 alternating F24 events, including zero and 300 ms delays) stored successfully across five pages. Replacing it with a short macro exposed stale bytes in later pages; the original variable-page writer could not clear these. Readback rejected the mismatch and verified rollback. The corrected writer sends all five pages on every replacement, with only page 4 marked final. The original empty slot was restored from its saved backup and verified.

A fresh complete test then passed: long macro, short replacement, and empty restoration all matched every one of the 256 bytes. Both full keymaps matched their pre-test snapshots. Slot 49 remained unbound throughout; no playback was triggered. Regression tests simulate storage replacement to cover stale-page clearing.

## Lighting read result

Two reads using command `87`, each preceded by a verified `80` barrier, returned identical 64-byte data: `87 05 04 04 07 08 08 08` followed by zeros. The codec interprets effect 5 (ripple), brightness 4, speed 0, normal color, RGB `(8,8,8)`. This establishes stable read framing. At this first-read milestone, writes were still unverified; later write evidence follows below. Visual effects remain unverified.

## Global lighting write verification

The original ripple setting was backed up, brightness changed from 4 to 3 with command `07` and the byte-8 complement checksum, then restored to 4. Both transactions passed repeated full reads, including unchanged reserved response bytes. Final setting bytes and both keymaps matched their originals. This confirms BIT8 framing and brightness storage on the attached firmware; visual output and other effect families remain unverified.

## Per-key color storage verification

Command `8c` returned six raw pages twice with identical results: 128 RGB triples, seven red and the rest black. Command `14` changed only Pause's matrix slot 91 to `(8,16,24)`, verified against all 128 colors, then restored its original color. All colors, both keymaps, and the full global lighting response matched the pre-test values afterward. Backups were flushed before writes. This validates current-picture color storage, not visual display or the three picture-selection options.

## Fn write investigation and recovery

The vendor-traced simple Fn command `15`, index 0, slot 91 did not change the Fn map as intended. It affected the base slot, and the first rollback was rejected by whole-map verification. A fresh snapshot showed only base Pause cleared. An explicit backup restoration using the verified base command `13` restored both complete maps exactly.

The separate full Fn command `10` also failed intended-map verification. Improved rollback inspected both maps and restored all observed differences, then verified both originals. Fn writes are therefore blocked in the backend and read-only in the GUI pending stronger evidence; the source's UI index is indeed zero, so changing it speculatively is not justified. See `Research/protocol-fn-correction.md`.

An isolated base-only test subsequently passed all three macro binding modes (`09 00 31 00`, `09 01 31 00`, `09 02 31 00`) at Pause, restoring the original maps after each mode. Slot 49 was empty and unbound before the test; no playback was triggered.

## Original operating-system HID adapters

HIDAPI was removed from the manifest and resolved lockfile after its build-script licensing discrepancy was found. The original Windows adapter enumerated the same seven collections and selected `FFFF:0002`. It successfully performed the explicit keymap recovery, the complete long/short/empty macro test and all three base macro-binding tests. Windows raw descriptor retrieval now reports unsupported rather than reconstructing a descriptor.

The original Linux hidraw adapter and full GUI pass `cargo check --target x86_64-unknown-linux-gnu --offline`; this is a cross-target type check, not Linux linking, execution or hardware validation. An OS-held file lock serializes Byakko transactions across processes; a test verifies exclusion and release on handle drop. The OEM helper does not participate in that lock and still must be closed.

## Scalar settings and native UI checks

Repeated native reads matched the official captures: debounce 1, auto OS false, sleep timers `[120,120,600,600]` seconds, options flags `0x10`, Fn-matrix flag false and power-save value 1. Backed-up debounce `1 -> 2 -> 1` and auto OS `false -> true -> false` passed complete settings readback. Both keymaps and global lighting matched afterward. Sleep and option writes were not attempted.

The native GUI launched and loaded the keyboard without the official helper. Ctrl+3 opened lighting and displayed the verified ripple/brightness4/speed0/RGB8 state; Ctrl+4 opened the physical color layout and reported matching picture reads; Ctrl+2 exposed the native macro editor. An event-modifier shortcut fix was necessary for fast press/release batches. Screenshot capture for native windows remains unavailable in this session; accessibility state establishes these controls and values, not full visual layout quality.

## Current official web Fn comparison

With a fresh native backup and the supplied helper, the current official web app selected `FN Settings`, Pause, Play/Pause, then Confirm. After stopping the helper, complete native reads showed exactly one change: **function** slot 91, from zero to `03 00 cd 00`. The base map was untouched. Reopening the helper and choosing “No feature set” restored both complete snapshots byte-for-byte. Files are `keymaps-before-official-fn.json`, `keymaps-after-official-fn.json`, and `keymaps-restored-official-fn.json` under private captures.

This proves Fn configuration works through the current official interface. It contradicts any broad claim that this firmware lacks Fn writes. The unresolved difference is between our direct packet path and the successful current app/helper path. Native Fn writes remain guarded while that difference is traced; do not infer a different index or toggle undocumented flags.

## Final HID capture and successful native Fn media replay

Microsoft's x86 CDB observed the supplied helper's `HidD_SetFeature` calls. Current FnSettings Confirm sends one `15` command, index 0, slot 91, with binding `03 00 cd 00`; No feature set sends the same command with zero binding. The helper passes 67 bytes (report ID, 64-byte payload, two trailing zeros). Startup reads use the same length. The trace is private `official-fn-hid-20260922.log`; no helper code was incorporated. Both complete maps after the official restore had SHA-256 `BFF029863CA0A8B6B534E8D45E715F9AC5F5E4B4C6125F9706AA74292038731C`, matching the original export.

The independent `examples/fn_capture_replay.rs` sent the same command through our existing **65-byte** native transport, with a 500 ms settling interval. Complete base and Fn snapshots matched exactly the intended single media-binding change, and both maps were restored exactly. Evidence prefix: `fn-replay-1790046788003896300`. The extra two helper bytes are therefore not required for this successful replay. Earlier failed tests do not justify a universal claim that command `15` aliases the base map.

A follow-up F24 replay with a 100 ms interval stalled. Debugger stacks placed the main thread inside `HidD_GetManufacturerString`, called by enumeration within `open_unique`, rather than inside a binding report. Only its before snapshot was saved (`fn-replay-1790046859360046800-before.json`). The test was stopped. Optional manufacturer/product string requests have now been removed from Windows enumeration; device identification still uses VID/PID and HID capabilities.

Subsequent configuration reads returned Windows error 31 (device not functioning), while PnP still listed the collections as present and OK. Restarting only `USB\\VID_3151&PID_4015&MI_02\\9&33655970&0&0002` was denied by Windows. **State after that interrupted follow-up has not been reverified**; do not report restoration for that run. After USB recovery, compare both maps with its saved before snapshot and restore any difference before more tests. Fn writes remain guarded pending that recovery and verification through the application transaction path. Physical Fn output remains untested.

## Recovery after the user's USB replug

The new complete snapshot `keymaps-after-user-replug.json` differed from the
interrupted test's backup only at Fn slot 91: `00 00 73 00` (F24). Thus that
write survived the physical replug; the interrupted restoration had not
completed. The application restored the backup and verified both complete maps.

The application transaction path then passed separate Fn F24 and Play/Pause
write/readback/restore cycles, leaving the base map unchanged. Each restore
matched the entire original snapshot. Optional USB string lookups remain removed.
This supports enabling Fn edits and imports; physical output is still untested.

Mixed edits assigning base F23 and Fn F24 to Pause in a single transaction
failed full-map comparison. Changing write order, inserting a version-read
barrier, and reopening a handle for each write did not resolve the mismatch.
Every failed transaction verified restoration of both original maps. Those
experimental workarounds were removed. The precise firmware/transport cause
is unresolved; do not claim a proven timing or handle issue. The application
now rejects simultaneous changes to both layers of the same physical slot
before device access. `verify-fn-roundtrip` covers the supported Fn-only cases.

## Fn macro bindings and transitional macro reads

All three Fn macro binding modes (`09 00 31 00`, `09 01 31 00`,
`09 02 31 00`) passed application transactions at Pause with complete-map
restoration after each. Slot 49 was empty and unbound before testing; no
playback was triggered. `verify-fn-bindings-roundtrip` reproduces this check.

The macro writer now checks firmware `0100` and profile 0 on its actual write
handle. Subsequent long/short/empty tests exposed a transitional macro read:
the first 32 bytes of page zero reflected the new data, while bytes 32–63
still reflected the previous contents. The next full read was current.
Longer delays and using the same handle alone did not resolve this. Earlier
two-read validation correctly rejected the differing copies; fresh standalone
reads confirmed the rollback had restored the original empty macro.

The reader now accepts only two **consecutive, identical, complete** 256-byte
snapshots, within at most three reads. It does not accept a non-consecutive
majority or ignore errors. A regression test covers transitional data,
alternating copies, incomplete copies, and read failure. The original timing
was retained. With this rule, the complete long → short → empty test passed
byte-for-byte, including zero/long delays; both keymaps were unchanged.

Twelve previously missing visible actions were added from independently
recorded wire facts in `Research/action-coverage.md`. These expand the native
catalog but do not establish physical host behavior for those actions.

## Sleep timers and further mixed-layer investigation

Requiring two consecutive matching reads of each individual keymap page did
not fix the mixed-layer transaction. Its failed test restored both original
maps. That experimental reader was removed; the same-slot mixed-edit guard
remains, and its cause remains unresolved.

The current web bundle's selected sleep setter was retraced separately from
the old installer: command `12`, BIT7 checksum, four little-endian values at
payload bytes 8–15. The response still stores those values at bytes 1–8.
A native probe changed Bluetooth normal sleep from 120 to 180 seconds and
restored it; complete settings and keymaps matched afterward.

The application transaction then changed all four timers from
`[120,120,600,600]` to `[180,180,660,660]` and restored the originals.
Complete raw settings and both maps matched. The native settings page now
supports those timers in minutes, including zero to disable. Bounds follow
the current descriptor: normal 1–60 minutes, deep 10–60 minutes. Physical
idle/sleep behavior and wireless transport remain untested.

## Onboard lighting family coverage

`examples/lighting_families.rs` now exercises native lighting transactions
with restoration after every case. Six initial cases covered static white
(including the near-white wire sentinel), wave direction/speed/custom RGB,
wave dazzle, neon without RGB, picture selection 2, and off. A second run
covered every remaining onboard effect ID through 19, with brightness 2,
speed 2 where applicable, RGB `(8,16,24)` where applicable, and the last
advertised option where present.

Every case passed exact stored-field verification, decoded back to the desired
setting, and restored the original lighting. Each run also compared both full
keymaps, all 128 picture colors, and all raw scalar settings with its before
state; all matched. This establishes selected parameter cases for every
onboard effect ID 0–19, not every combination or visible animation behavior.
Host music/screen modes 20–22 were not exercised by these tests.

## Mixed-layer writes resolved by write spacing

A direct post-write GetFeature response did not resolve mixed-layer writes at
100 ms spacing; both original maps were restored. The official web UI then
assigned base A and Fn B to Pause, and native full-map reads confirmed the
two distinct values. The official UI restored the original assignments; the
complete exported snapshot hash matched the original backup. This rules out
a general inability of the firmware to store different layer assignments.

With one second between native writes, the standalone mixed F23/F24 replay
passed. The application transaction path then passed both base F23 + Fn F24
and base F23 + Fn Play/Pause, **without** extra response reads. Every test
restored and verified both complete original maps. `write_binding` now waits
one second after each setter, including recovery writes, and the mixed-layer
guard is removed. This establishes a verified interval, not a measured
minimum or an explanation of the firmware internals. Physical output remains
untested. `verify-mixed-roundtrip` reproduces the application checks.

## Mixed macro event storage (2026-09-22)

`verify_macro_events` passed on firmware0100/profile0 using empty, unbound slot49.
Its 33 events cover keyboard up/down at 0, 1, 127, 128 and 65,535 ms; movement
with signed coordinates -128/+127 at all five delays; and both event directions
for every mouse action byte240–248, distributed across those delay boundaries.
All 256 stored bytes matched the native encoding and decoded to the original
event list. The slot was restored to empty. A complete two-sweep configuration
capture after restoration matched the before archive, including both keymaps,
all50 macros, picture colors, lighting and settings. No macro was bound or played.

The before archive `Research/captures/configuration-before-macro-events.json`
has SHA-256 `2137480f0ba425bf06c9ef9a37881096834c928f6d4c21208ed65daf210ba4d4`,
identical to the earlier restored baseline. The ignored local trace
`Research/captures/backups/macro-events-setters-1790059585362888900.json`
contains exactly ten successful setter API calls: slot49 pages0–4 to write,
then pages0–4 to restore; no dropped entries. The trace is not a USB bus capture.

Reproduce with `cargo run --features research-tools --example verify_macro_events -- BASELINE.json`.
This command requires a matching full baseline and performs actual reversible
device writes. It does not establish physical timing, mouse behavior, repeat
modes, power-cycle persistence or fault recovery. The selected official reader's
movement-delay inconsistency is documented in `Research/macro-boundary-audit.md`.

## Read-only USB replug observation (2026-09-23)

With the user physically unplugging and reconnecting the Nia87, a 250 ms
read-only HID inventory monitor observed its configuration collection disappear
at 08:51:40 and return at 08:51:47 local time. The same inventory saw seven Nia
OEM collections before and after, including `3151:4015`, interface 2,
usage `FFFF:0002`; eleven Wacom collections remained a separate target.

The shared core/executor keymap read completed successfully before and after
the observed cycle, with no setters. The two ignored completion captures,
`Research/captures/replug-read-20260923-085045.json` and
`replug-read-20260923-085205.json`, are byte-identical (SHA-256
`6ECA5479044CDB2F8A79F3A39AA644C5E13B6D38B88B6C26B89FD17958DB8190`).
This establishes collection rediscovery and a fresh getter path after one
physical replug. It does not establish the Iced window's automatic UI state,
feature-panel refresh or Linux hotplug behavior.

## Native Iced launch and physical layout (2026-09-23)

After the replug, the user confirmed ordinary keys still typed and launched
`target/release/byakko-desktop.exe` without model setup or browser authorization.
They reported the Nia87 key layout opened with the “Readback verified” status,
but the first build displayed keys in a left-hand column. The keymap and
per-key color selectors were changed to render the advertised physical
coordinates through one device-neutral Iced board widget. The user then
launched the rebuilt release app and confirmed that the Keys page showed a
TKL-shaped board and clicking a key selected it. No Apply action or device
setter was used in this UI check. The user also confirmed the same behavior
after the shared widget switched to coordinate-pinned placement for taller
future keys. Visual acceptance of the per-key color page,
Linux runtime and live write recovery remain open.

## Independent CLI read (2026-09-23)

`byakko-cli read` used the same core session command and selected Nia87
executor as the Iced app, with no setter command. The ignored local JSON export
`Research/captures/cli-read-20260923.json` contains two 128-binding layers.
Its revision and both binding maps matched the earlier post-replug shared
executor completion capture exactly. The CLI export SHA-256 is
`30DCFF7A58F4767676AFDD415682A36175469F7C5BEF2105D245F1D523B2515E`.
This exercises a second frontend through a physical USB getter; it does not
prove a CLI apply workflow or Linux runtime behavior. Windows and Linux-target
Clippy checks pass. A Linux release link from this Windows host remains
unverified with plain Cargo because this environment has no `cc` cross-linker;
the retained Zig toolchain later cross-linked both the desktop and CLI.

## Automatic color-page read and idle resource sample (2026-09-23)

The Iced page transition now requests a per-key color read when its editor is
unloaded or invalidated and the keymap is ready. If keymap read is still active,
the color read starts after that completion. It does not repeat on every page
visit or silently retry a failed/uncertain read. Memory-backend tests cover
these transitions; the Iced window itself has not been visually rechecked for
this change. `byakko-cli read-colors` made a live read-only pass through the
same core session and executor and returned 87 color entries (ignored capture
`Research/captures/cli-colors-20260923.json`, SHA-256
`2B28FD8A0AE7A419AE6CC5BF8BF065F52CABA914A16827B3C0C6BA038D83A018`).

With the native release app open at idle on the Keys page, a 15-second
`Get-Process` sample showed 23.6 MiB working set, 9.9 MiB private bytes,
seven threads, 235 handles and 0.188 CPU-seconds/minute extrapolated from the
CPU counter delta. No comparable Sharkfin or official-app process was running,
so this is a baseline measurement, not a performance comparison. Windows
Graphics Capture could not return a window screenshot (`SetIsBorderRequired`
failed with `0x80004002`); accessibility exposed only the Iced title bar. A
single authorized 640×480 webcam frame, captured with the existing local
OpenCV runtime, visibly showed pink/purple light between the Nia87 keycaps
(`Research/captures/webcam-20260923.png`, SHA-256
`DDCE56AB1E63F079F3ACCAC1B4086860FE4EFEDF7DAC3EB234504826BDF1657C`).
This is a visual baseline, not evidence of a lighting change or of UI color
readback. The portable core compiles for `wasm32-unknown-unknown`; no browser
app has been built.

## Linux release cross-link and competitor idle sample (2026-09-23)

The current Iced desktop and read-only CLI cross-linked for
`x86_64-unknown-linux-gnu` with Zig 0.15.2 and cargo-zigbuild 0.23.4 using
`--release --locked --offline`. Both files have ELF magic `7F454C46`.
After the subsequent CLI, page-read and Windows entry-point changes, the
refreshed desktop is 10,087,712 bytes (SHA-256
`D849F0B0AC7E687F26F885EF77ABAF9D9DF76E50579D66715F7896616FDD3553`);
the CLI is 1,496,240 bytes (SHA-256
`377A89D908E8F7A5F44E6964D9580709329DFAF9F5F3A943C69C7D70D76B8E04`).
This proves linking, not Linux startup or HID access.

The user located the installed Sharkfin at
`C:\Users\two\AppData\Local\sharkfin`. Its connected Nia87 Lighting page was
left idle, with no settings touched. We sampled its main process and the six
WebView2 descendants belonging to it, excluding unrelated WebView2 processes.
The first Byakko build used the Windows console subsystem, creating an
additional `conhost.exe` that the initial one-process baseline omitted.
Adding the GUI subsystem to the desktop entry point removed that child.
`tools/measure_process_tree.ps1` then sampled each app's complete descendant
tree using `Win32_Process` working set, private pages, and cumulative
user+kernel CPU time 15 seconds apart after launch:

| Idle app/page | Processes | Working set | Private pages | CPU delta, extrapolated per minute |
| --- | ---: | ---: | ---: | ---: |
| Byakko / Keys | 1 | 23.4 MiB | 9.6 MiB | 0.25 CPU-seconds |
| Sharkfin / Lighting | 7 | 476.2 MiB | 283.6 MiB | 5.625 CPU-seconds |

This is an observed idle footprint advantage for this Windows session, not
an active-use benchmark: the pages differ, 15-second CPU deltas are noisy,
and memory changes with uptime and renderer state. An earlier 15-second pair
using the same Windows counters gave 23.3/490.4 MiB working sets and
0.812/2.25 CPU-seconds per minute, illustrating CPU variation. The official app was not
profiled; starting its retained installer could launch its helper on the
connected keyboard. No app control or device setter was activated in this
comparison. A second webcam frame afterward still showed pink/purple light
between the keycaps (`Research/captures/webcam-after-profile-20260923.png`,
SHA-256 `85103B6D819A37548D9F52901D6C4061DD37D516DF9B93BE972F1930CD0BB4F5`),
but changed room exposure prevents a quantitative before/after lighting
comparison.

## Read-only lighting and settings CLI follow-up (2026-09-23)

The independent CLI's `read-lighting` and `read-settings` commands now drive
the same keymap-first session, executor and accepted-completion path as Iced.
Both completed against the attached USB keyboard with no setter. Their ignored
JSON captures are `Research/captures/cli-lighting-20260923.json` (SHA-256
`52F90144A8875F06D6F241C72E642CEDF9EC2E543070BBFB24BCD3BB0F08252C`)
and `Research/captures/cli-settings-20260923.json` (SHA-256
`F386EC90F827E8F3A931DA9A3379426DC6BE82439265143D0C76599567E3F433`).
The settings decode reports debounce 1, auto-OS off, backlight on, normal
sleep 2 minutes and deep sleep 10 minutes on both radios.

The lighting result was stable across a second read, but its effect byte was
1 rather than the saved baseline's 5. To bound the difference, a full
two-sweep read-only configuration and transport trace completed after the
Sharkfin profiling launch. Compared with the earlier restored archive, both
keymaps, all 50 macros, all 128 picture colors and settings were identical;
only lighting byte 1 differed (`5 -> 1`). The new ignored archive is
`Research/captures/configuration-after-sharkfin-launch-20260923.json`
(SHA-256 `31C9B1C5CBF8E41A7018355E114361FC967C54732A4D5DA88E5A690506FAD3C8`).
The timing suggests a possible app-startup effect, but the reads cannot assign
causality. No restore write was attempted while the user was AFK, because
unexpected collateral changes in the earlier archive recovery remain
unexplained. The on-device lighting baseline is now effect 1 for subsequent
tests; do not assume the older effect-5 archive still describes current state.

## Automatic feature-page reads and CLI macro read (2026-09-23)

The Iced Lighting, per-key Color and Settings pages now share one entry-read
decision: when the selected page has an unloaded or invalidated snapshot, it
reads once after keymap readiness. Revisiting a verified page does not repeat
the request; failed reads remain manual retries. Memory-backend tests cover
each page and preserving a staged color draft through reconnect. The rebuilt
window has not been visually checked for these new transitions.

The CLI's `read-macro slot-49` completed directly over USB without a setter.
The complete 256-byte revision was zero and decoded to no events with stored
repeat count zero, consistent with the earlier unbound empty-slot capture.
The ignored JSON export `Research/captures/cli-macro49-20260923.json` has
SHA-256 `9A9FA47A17C3FA2D615E4165141068BCD6E89B4B9262FFE70C5EB92FEFE55306`.
The CLI preserves that raw zero; the editor still blocks staging or binding
counted mode zero without physical playback evidence. A repeated CLI settings
read produced byte-identical JSON to the preceding one.

The updated Windows release launched again as one GUI-subsystem process, with
no `conhost.exe` child. A read-only lighting CLI check after this launch still
returned effect 1 and the same first eight raw bytes. This shows no further
change on that launch; it does not identify the cause of the earlier `5 -> 1`.

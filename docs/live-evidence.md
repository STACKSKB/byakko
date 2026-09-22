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

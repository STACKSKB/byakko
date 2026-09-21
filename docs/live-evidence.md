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

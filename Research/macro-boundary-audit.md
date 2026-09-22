# Nia87 macro boundary audit

## Live follow-up supersedes the header-length inference

The attached Nia87 firmware0100/profile0 accepted exact 248-byte macros in empty,
unbound slots 1, 24 and 49, followed by short replacements and restoration to
empty. All 256 bytes were compared after each write; all three slots and both
keymaps matched their original state after restoration. See
`examples/verify_macro_boundaries.rs`. This verifies storage, not playback.

An official helper HID capture on 2026-09-22 disproved the inferred total-length
header for this device. Assigning a short local comparison macro to Pause emitted
payload header `16 00 00 38 01 00 00 b0`, followed by data
`01 00 f0 81 f0 32` and zeros. Thus header byte3 is 56 even for six used bytes;
the final flag is set on this single transmitted page. Byakko's 56-byte header
is correct for this observation. Its five-page clearing remains intentional;
no transport change was made based on the generic bundle inference below.

The capture is retained locally at ignored
`Research/captures/macro-official-headers-1.log`. The helper was stopped and the
debugger detached. Both keymaps were restored and verified against the previous
baseline; slot0 was backed up before the comparison, restored, exported again,
and all 256 original bytes matched. No physical playback was triggered. The
browser-local comparison macro remains saved, unassigned, for future reference.

## Original static audit (not authoritative for selected runtime overrides)

Scope: original `src/macros.rs` compared with protocol facts in the supplied, ignored `Research/extracted/web-current/main_68eaf5ce.js`. This note records observations only; no bundle code or tables are incorporated. No keyboard I/O was performed for this audit.

## Findings

| Boundary | Current codec | Bundle evidence | Assessment |
| --- | --- | --- | --- |
| Encoded capacity | Accepts events through byte 247, requires bytes 248–255 zero, and can fill exactly 248 bytes. | The Nia87-relevant simple macro writer around offset 21,539,241 allocates 256 logical bytes and its UI length check around 21,544,700 reports full at `>=248`. | Conservative and sensible. The UI would reject an event that reaches exactly 248, whereas Byakko permits exactly 248. Need a reversible device test to establish whether the exact boundary is usable. |
| Repeat count | `u16`, including zero, little endian at bytes 0–1. | The setter around 21,539,241 writes an unsigned 16-bit little-endian repeat count. The official UI around 25,193,154 offers 1–65535 for repeat-times mode and forces 1 for the other two play modes. | Wire range matches; Byakko's zero value is outside the official editor's repeat-times range. Check the UI's constraints and avoid claiming zero playback semantics without a physical test. |
| Movement delay | Action byte 249, then a raw 7-bit short delay or zero marker, signed `dx`, `dy`, and optional LE16 delay. | The simple setter around 21,539,241 emits movement byte 249 and does not shift its short delay before writing. Its own reader around 21,530,900, however, shifts a nonzero movement short delay right one bit. | Bundle encoder and decoder conflict for short movement delays. Byakko follows the setter's wire byte and preserves a full 1–127 ms round trip internally. A device readback may clarify storage; physical movement timing remains unverified. |
| Zero delay | Encodes an explicit long-delay marker and LE16 zero after each action; decoder preserves zero. | The setter around 21,539,241 handles the zero-valued delay event through its long-delay branch; its reader around 21,530,900 filters zero delay events from the UI event list. | Wire representation is plausible; preserving zero is an intentional Byakko data-model difference. The supplied writer's event-list flow can add another zero word for an explicit zero-delay token, so bundle output is not a clean fixed vector for this case. |
| Write pages | Always sends five 56-byte pages of the 256-byte logical buffer, including zero-filled trailing pages. Header has opcode `0x16`, slot, page, `report[3]=56`, final flag only on page 4. | The simple writer around 21,539,241 sends only pages through the last nonzero page, puts total used byte count in header byte 3, and marks its last sent page in byte 4. | **Material wire difference.** Byakko's five-page clearing worked in earlier native readback tests, but the header length is not the bundle's value and the final marker differs for short macros. The native long→short→empty replay should compare entire 256-byte readback, particularly slot 49 and the exact 248-byte edge. Consider adopting observed length/final semantics only if hardware evidence supports them; retain full clearing if needed to prevent stale tails. |
| Slots | Guard accepts 0–49. | The configuration store around 23,589,000 sets a default maximum of 50 macro assignments and assigns the first free index starting at 0. Some other device families use 20. | Generic 50-entry evidence supports 0–49 for this device family, but the bundle excerpt does not prove the firmware's last accepted index. Slot 49 deserves a reversible write/read/restore test. |

The supplied bundle is minified and has several hardware-family implementations. Offsets above refer to the current bundle's character positions and identify the simple macro implementation; they are not source line numbers. A passing codec unit test alone cannot establish firmware interpretation or physical playback.

## Offline comparison and macro editor lifecycle

`cargo run --no-default-features --example compare_macro_capture -- Research/captures/macro-official-headers-1.log`
parses the retained official HID log without accessing a device. It validates
complete 67-byte debugger dumps, ordered macro pages, header length and checksum.
The captured short macro contains one page: 56 logical bytes observed and 200
unsent bytes unknown. With the explicit `--assume-zero-unobserved` comparison
option, its observed data and opcode/slot/page/length fields match the native
writer. The final marker differs (official page0; native page4). Unsent bytes
are not evidence of firmware clearing, and this comparison does not prove
playback or interrupted-write semantics. No transport policy was changed.

The native macro editor now cancels window close while a device worker is
running, before processing same-frame completion. Worker panics become visible
unverified-state errors; drafts survive errors or unexpected returned bytes.
Device-free tests cover close/error races, mismatched readback and panic
completion. These checks do not resolve the separate hardware recovery failure.

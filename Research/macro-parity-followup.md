# Macro parity follow-up, 2026-09-23

The retained official HID capture in
`Research/captures/macro-official-headers-1.log` has one 56-byte write page for
slot 0. Its header is `16 00 00 38 01 00 00 B0`; the used data begins
`01 00 F0 81 F0 32` (repeat 1, mouse-left down with 1 ms wait, then up with
50 ms wait). The current codec produces those event bytes and the same
opcode/slot/page/length fields. The offline comparison confirms this when its
200 unobserved logical bytes are assumed zero.

The concrete wire-parity gap is the final-page flag and page count: the
official short write ends at page 0, while Byakko writes five pages and marks
page 4 final. This is intentional for replacement safety. The attached-device
long-to-short-to-empty replay in `Research/macro-boundary-audit.md` established
that sending only a shorter stream leaves stale later pages; complete 256-byte
readback passed after Byakko's five-page replacement. The official capture
does not show the 200 unsent bytes, so it cannot establish how the official
helper clears an existing long macro. Changing the native writer to match this
single short capture would weaken the verified replacement behavior. No codec
or editor change is supported by the current fixtures.

The editor already treats raw repeat zero as readable while limiting staged
counts to 1–65,535 and requiring count 1 for toggle/hold bindings. The
remaining acceptance gaps are physical playback modes and timing, including
repeat zero and movement short-delay behavior; the selected official writer
and reader disagree on the latter. Neither difference should be resolved from
an offline round trip alone.

Offline validation: `compare_macro_capture` matched observed data and header
prefix. A device-crate regression now pins the complete observed 64-byte
payload, including zero padding and checksum, and checks that only the
intentional final-page flag and its checksum differ from Byakko's first page.
No device writes were made.

## Slot count on the attached firmware

The selected official configurator sets `MACROMAX` to 50 and normally allocates
indices 0–49. Its inclusive allocator loop can mention index 50, but the
preceding capacity check stops ordinary allocation once 50 macros exist. This
is a configurator policy, not a proven firmware storage boundary.

A read-only research probe on firmware `0x0100`, profile 0, requested all four
pages of slots 0, 49 and 50 through the validated configuration collection.
Each page used an identity barrier, and two complete copies had to match.
Slot 0 decoded as the existing program; slot 49 decoded as an empty program.
Slot 50 repeatedly returned zeros at the start and `FF` at offsets 42, 45,
63, 231, 246 and 249. That 256-byte response is distinct from both comparison
slots and fails the safe macro decoder because nonzero bytes occur beyond its
usable limit. A complete read-only archive after the probe compared equal to
the before archive (`[]`). No setter or binding was sent.

The result does not prove whether slot 50 can be safely written or played. In
particular, a stable getter response could expose adjacent or uninitialized
storage. Keep the product capability at the official 50 slots (0–49); do not
promote index 50 from this read alone. The isolated probe is
`examples/probe_macro_slot50.rs` and is excluded from normal product builds.

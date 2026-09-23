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

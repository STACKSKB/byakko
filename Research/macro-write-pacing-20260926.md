# Macro write pacing investigation — 2026-09-26

Windows Nia87, VID 3151/PID 4015, MI02, firmware 0100, profile 0. Work is on
`codex/macro-write-fix-2026-09-26`, separate from the cleanup branch. No firmware
flashing or fault injection. Per-feature before-images are retained locally.

## Official application capture

User recorded `ab`, counted playback 1, and assigned Pause in Nia87 Driver 2.1.97.
The first debugger session ended before the assignment and contains no macro
setter evidence. A fresh helper capture recorded the user's reassignment:

| Local time (UTC+05:30) | Payload header/data | Meaning |
|---|---|---|
| 16:41:28.712 | `13 00 5b 00 00 00 00 91 09 00 01 00` | Base Pause slot91, macro 1 counted |
| 16:41:29.113 | `16 01 00 38 01 00 00 af 01 00 04 fc 04 00 96 08 05 f8 05 32` | Macro1, page 0, length56, final1; remaining bytes zero |

The macro decodes to repeat1; A down124ms, A up2198ms, B down120ms, B up50ms.
There were two setters and no macro getter in this assignment capture. The
approximately 401 ms gap includes debugger overhead, not a native performance
measurement. The official short writer sends one page; native full replacement
continues to send five to clear stale tails. Save/verify precedes assignment in
Byakko. The helper and debugger were detached/stopped before native tests.

Local capture `official-macro-20260926-r2.log` SHA256:
`8bf2b56038bdedefdfbd4b398fd81b46d0583e1bc3e9decf401254b7064df70d`.

## Reproduction and minimal fix

The unchanged native 200 ms writer reproduced the Linux failure on Windows:
slot 1 readback page 1 aborted with Windows error 995; recovery page 0 setter failed
with error 433. A fresh read found an empty slot. Full comparison also found
macro 0 cleared and Pause restored to its ordinary action. Current lighting and
settings were preserved. The original slot 1 backup is
`macro-1-before-1790421278561454400.json` under the normal Byakko backup directory.

History at successful boundary commit 508d698 used the same five packets and
200ms settle, but identity checks between every page and two complete copies
added incidental timing. Commit 977df91 removed those redundant reads. That is
a timing change, not proof of the firmware's internal cause.

A 1 s settle restored the saved macro and, after restoring Pause's assignment,
the user observed `a`, a delay, then `b` in Notepad. Boundary testing at 1 s then
returned old page 0 and new pages1–3 for slot 24. Recovery similarly returned old
page 0 before a fresh read confirmed empty. This candidate was insufficient.

The pacing change uses 2 s final settling, retaining 30 ms/page. It removes the
conditional mismatch reread and its shared-helper
parameter. Each normal write uses the cached before-image backup, setter,
settling, and one complete readback; recovery also has one readback. There are
no identity exchanges within macro reads and no executor pre-write reread.
Exact collection selection, packet validation and result comparison remain.

The transport trace records exchange start times, not separate send/get end
times. It cannot distinguish delayed flash commitment from reply preparation
or caching; no firmware-internal mechanism or minimum safe delay is claimed.

## Windows verification

- Slots2,24,49: full248-byte/count65535 program → short → empty. All nine
  transactions matched 256 bytes; all three slots restored, keymaps unchanged.
  Trace has 45 setter reports, no dropped events. Each first read began
  2031–2033ms after the final setter attempt, and used one four-page copy.
- Slot49 mixed fixture: 33 keyboard/button/movement events, signed extrema and
  delays 0, 1, 127, 128, 65535. Exact write/restore and complete configuration equality.
- Shared CLI/session macro path: official `ab` repeat1 →2 →1 verified. An old
  snapshot was correctly rejected for stale revision before restoration; using
  the returned current revision restored it. This rejection sent no setter.
- User confirmed physical `ab` playback after native restoration and native
  key assignment. This establishes that keyboard sequence, not all mouse/toggle
  or hold-mode playback timing, nor every Iced gesture.

Boundary trace `macro-boundaries-transport-1790421801184552200.json` SHA256:
`9343148317163891dc167c6416e5f443b1e1803568de21540d30f9d940d31283`.
Earlier 1 s trace: `macro-boundaries-transport-1790421733539362400.json`.
Mixed-event trace: `macro-events-transport-1790421899941869100.json`.
All are ignored local files below `Research/captures/backups`.

The wider-session final comparison found picture slot9 (A) red→black, between
the pre-candidate snapshot and the snapshot before mixed-event testing. That
window includes the 1 s mismatch/recovery, playback and boundary tests. It does
not establish which operation caused it. The color was restored separately
under the unchanged lighting selector, followed by targeted collateral checks.

## Final-page boundary correction

The same red baseline passed a mixed-event write/restore in slot 2, but failed in
slot 49: its raw picture page 0 byte 27 changed FF→00. A final-page length of 32
instead of 56 also failed; that candidate was discarded. A long program read
immediately after writing did not alias its data into the picture.

A temporary pattern across all 87 physical keys then established that the
slot 49 round trip cleared picture bytes 0–29; bytes 18–20 were already zero for
an unmapped matrix entry. Other picture bytes, lighting, keymaps, settings and
all restored macro bytes matched. Trace:
`macro-events-transport-1790422779793932300.json`.

Changing only the final-page declared length to 26 passed the same patterned
baseline: 33 events stored, slot restored, and the complete configuration
equal. Trace: `macro-events-transport-1790422933274028200.json`. This supports
four 56-byte pages plus one 26-byte page, totaling 250 writable bytes. The getter
still returns 256 bytes: even with nonzero picture bytes 0–5, macro 49 bytes 250–255
were zero. The editor's existing 248-byte encoded limit is unchanged. We do not
infer the firmware's complete flash layout or its behavior for invalid lengths.

The final implementation names this 250-byte writable extent, retains all five
pages for replacement/clearing, and keeps the 2 s settling and one readback. It
adds no color workaround or extra runtime getters. Codec regression tests use
the packet's declared lengths and neighboring sentinel bytes; the old test
clamped every copy to 256, masking the oversized final packet.

The final boundary run also seeds an adjacent macro and patterned colors, so
restoration equality can detect damage that empty neighboring slots concealed.
It passed all nine long/short/empty writes in slots 2, 24 and 49, preserving the
adjacent `ab` macro in slot 3 and the complete patterned picture. Trace
`macro-boundaries-transport-1790423046470366000.json`, SHA256
`6d673be3f6d20bc15c4eb663573d173b96d8c36a660c3dec8045a115f78d9c02`.

Final validation: 422 workspace tests passed; formatting, strict all-target and
all-feature Clippy, and Windows desktop/CLI release builds passed. The unchanged
renderer had passed its two clipping tests during the cleanup review. Regression
coverage includes one successful readback and one recovery readback after a
mismatch/error, full declared-length replacement, and untouched neighbor bytes.

Test macro slots and Pause were restored to their original state; the original
picture colors were restored, retaining the user's current Wave lighting and
settings. `macro-final-complete-restoration-20260926.json` matches the complete
`macro-before-settle-test-20260926.json` baseline. That baseline retains the
official-app lighting choice, rather than reverting to the morning's effect 19.
Both complete archive files have SHA256
`6bbc238f9b2e2bda6703f361168c68394c71b4e3e1c367187aa38fc4f77c7cf6`.

Linux verification is explicitly deferred by the user. Earlier archive/picture
fault-recovery gates remain separate; these tests do not close them.

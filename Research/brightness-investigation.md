# Nia87 brightness investigation, 2026-09-25

Status: Linux camera evidence now supports dimming for steady green 4→1.
Windows physical comparison remains deferred. No codec change is justified.

The user confirmed that Linux and Windows have different physical Nia87 units.
Keep baselines, restoration targets and physical conclusions specific to each
unit. Both cameras are available for observation. The Linux camera was checked
read-only after the user released another camera session: the keyboard and
exposed switch LEDs are visible, but no paired brightness images have been
captured yet. Do not infer brightness behavior from that setup frame.

A read-only camera-control query found automatic exposure (mode 3), automatic
white balance and dynamic framerate enabled on the Linux camera. Manual
exposure time is advertised over 2–1250, with current read value 156; the camera
does not expose a separate gain control in this query. No camera controls were
changed. For a meaningful brightness comparison, retain and restore the camera
settings and verify a fixed exposure without saturated LED regions; otherwise
report the visual evidence as limited. Local control log:
`/tmp/byakko-laptop-camera-controls-20260925.txt`.

The supervised Linux test changed steady green from brightness 4 to 1. The
readback changed only raw byte 3, but the user reported no visible dimming.
Visible mode/color switching worked. See [Linux evidence](../docs/linux-handoff.md).

## Windows read-only findings

The Windows agent returned request `WIN-20260925-001`, revision 1, directly to
the Linux task. It inspected source `b99fe6b073ffdcbaffe35fd0f06a74b1b87d9bc5`
and existing local evidence. No hardware access or new official-app test occurred.
The following capture findings are the Windows agent's report; the ignored
files are not present in the Linux checkout.

Byakko encodes brightness directly in zero-based payload byte 3, range 0–4.
For steady green, source-calculated setters are:

- Brightness 4: `07 01 04 04 07 00 FF 00 E9`, followed by 55 zero bytes.
- Brightness 1: `07 01 04 01 07 00 FF 00 EC`, followed by 55 zero bytes.

These are calculated reports, not captured official steady-green setters.
The HID report-ID byte precedes these payloads. No percentage scaling,
brightness inversion or multiplication of RGB occurs in this encoder.

Existing official evidence supports the stored field, but contains no matched
steady-green brightness sweep with visible observations:

| Windows-local evidence | Observation | SHA-256 |
| --- | --- | --- |
| `Research/captures/rpc-analysis-v4.json`, connection `1790018314442415300-5`, exchange 12 | Getter response `87 05 04 04 07 08 08 08` + 56 zero bytes: Ripple, stored brightness 4. Firmware reply identifies 0x0100; profile 0. | `dc5b9ad9416ebf4e7d3603f43654a5d0f2cd5ae7e91b0eb9df4df6f54ab6d1c1` |
| `Research/captures/host-lighting-final-2.log`, lines 298 and 3986 | Official screen setter `07 15 04 04 07 00 00 00 D4` and music setter `07 16 04 04 00 B4 B4 B4 BE`, each + 55 zeros. Both store 4. | `8a00e88e6fcacd3cd2fe4e9ef3328aa57b5ee0b9cfc941311ca6e57f060aaaa5` |
| `Research/captures/identity-bit7-read.json` | Historical ROYUAN Gaming Keyboard, VID3151/PID4015, interface 2, usage page FFFF/usage 2, firmware 0x0100/profile 0. | `2c887dff97ec713ee35d258f0e0157a7faf47a26ed6a5bc84f18b4f432a53a81` |

The HID log records helper API buffers (report ID + 64-byte payload + two
trailing zero bytes), not USB bus packets. It has ordering but no per-report
wall-clock timing or verified displayed brightness values. Screen's value 4
is fixed by source behavior. Neither setter proves static-effect dimming.

The Windows checkout lacks the newer referenced
`official-2026-09-24-hid-sequence.json` and `settings-profile-final.json`.
Do not attribute their claimed UI values, hashes or timing to these older logs.
The official PB wrapper's documented 500 ms pre-send delay and Byakko's 500 ms
post-send delay differ, but this alone does not establish a brightness cause.

### Newly located official capture (request 004)

Windows later found `official-2026-09-24-hid-sequence.json` under
`C:\Users\two\.codex\worktrees\5d1f\Byakko\Research\captures` and analyzed it
read-only. Its earlier absence applied to the inspected checkout, not the whole
machine. Length 10,791 bytes; SHA-256
`eeff9073f51ed830522750c212ba4bb23edb87b6ca427ff723d1c97ddf72e7a6`.
It is a derived official-app localhost RPC request fixture, format version 1,
dated 2026-09-24, containing 49 ordered exchanges and no response content.

All five lighting setter payloads below have **brightness byte 3 equal to 4**.
Indices are zero-based `hid_exchanges` positions. Each prefix is followed by
56 zero bytes, including zero in the checksum position: these RPC requests
precede helper framing/checksum insertion and are not final HID packets.

| Index | Phase | Eight-byte prefix |
| --- | --- | --- |
| 16 | Picture selection | `07 0D 04 04 00 00 C8 C8` |
| 43 | Always on | `07 01 04 04 08 B4 B4 B4` |
| 44 | Fixed mode | `07 01 04 04 07 B4 B4 B4` |
| 45 | Red preset | `07 01 04 04 07 D0 02 1B` |
| 46 | Hue drag completed | `07 01 04 04 07 D0 02 4A` |

43→44 changes only option/color byte 4; later changes affect RGB. There is no
same-effect/color pair with different brightness. Requests 43–46 have no
represented intervening getter, second lighting command or commit operation.
The fixture has `readMsg` markers but no replies, success outcomes, per-report
timing, firmware identity or displayed brightness values. These additional
steady-effect observations do not resolve the brightness question or justify
changing the codec. Request 004 is completed without any hardware access.

## Proposed bounded official comparison

Authorized for Windows (request revision 3): save the connected board's complete baseline and identity;
then use only the official app to select steady green at its highest and lowest
nonzero brightness. Capture every intervening report, displayed UI values, and
visible output after equal two-second settling intervals. Use fixed ambient light
and locked camera exposure/gain where available, otherwise user observation.
Release the official session before diagnostic getter reads. Restore through
the agreed normal lighting path and distinguish visible restoration from exact
raw equality; do not use full archive Apply for this brightness test. Capture a
final read-only archive to check unrelated sections. Stop on unexpected behavior
or restoration failure and report the evidence; do not repeat setters blindly.

The first camera activation for `WIN-20260925-002` was rejected by Windows
automatic approval review for missing explicit physical authorization. That
attempt produced no image or keyboard write; the camera-availability response
was not accepted as sufficient approval. Revision 2 held the physical request
while Windows build/tests proceeded.
The user subsequently replied **"I authorize this Windows test and restoration"**
to the explicit webcam/baseline/max/minimum-nonzero/capture/restoration question.
Revision 3 published this authorization. The user also directly told the Windows
agent **"You have my authorization"**. After that reply, a c922 capture succeeded
at 07:50Z: Windows-local
`Research/captures/keyboard-camera-1790322620522953900.png`, SHA-256
`8e8ab24bf44dcaddf0b01ad4777b202d69ab63c17f70f8d65cc48a8243124534`.
The Windows agent viewed it and reported an almost black image with no
identifiable keyboard. This corrects the earlier overbroad statement that no
image occurred: it applied only to the rejected attempt, not the later capture.

Windows has asked the user to uncover/reposition the c922. No official-app
action, diagnostic HID capture or keyboard setting change occurred, so no
restoration was needed. Revision 4 records this prerequisite; authorization is
not pending and no repeat test is requested merely by the revision change.
The physical brightness outcome remains unknown.

Identical official encoding plus equally absent dimming would support a shared
firmware/visual limitation. Different packets or visible behavior would guide
the next investigation. Neither outcome is established yet.

## Linux fixed-camera comparison

After the user reported that the Windows camera could not currently be
repositioned and the Linux camera worked, the prepared Linux steady-green
comparison was performed. The first attempt had unusable framing: its images
showed the wall above the keyboard. The lighting readbacks matched and the
saved lighting/camera controls were restored. A full archive matched the prior
baseline. This attempt, at
`/tmp/byakko-camera-brightness-20260925-l4lyrs79`, establishes no visual result.

The user then replied **"Repositioned and fixed"**. A fresh automatic-exposure
preview showed the board. Before further lighting writes, a fixed-exposure
preview was captured and inspected; it also showed the exposed switch LEDs.
The test then set steady green (effect 1, RGB `00 FF 00`) at brightness 4,
read it back, waited two seconds and captured three frames. The maximum frames
were inspected before setting brightness 1 and repeating the read/settle/capture.
Only lighting readback byte 3 changed, from 4 to 1.

The camera used manual exposure value 40, white-balance temperature 4600 with
automatic white balance disabled, and dynamic framerate disabled. Control
queries before and after each three-frame group matched. The groups used
1280×720 MJPEG input, discarded the first 30 frames and sampled every 15 frames.
No exposure/gain setting was adjusted between brightness levels. This camera
does not advertise a separate gain control through the queried interface.

The images show reduced green spill around the keys at brightness 1. As a
supporting, uncalibrated comparison, the fixed exposed-switch rectangle
`x=[245,440), y=[420,650)` had these mean green-excess pixel values
(`max(G - (R+B)/2, 0)`, 8-bit decoded image channels):

| Brightness | Frame 1 | Frame 2 | Frame 3 |
| --- | ---: | ---: | ---: |
| 4 | 11.344 | 11.787 | 13.059 |
| 1 | 5.128 | 5.215 | 5.250 |

All three minimum frames have lower recorded green signal than all three
maximum frames. This supports physical dimming on this Linux unit for this
effect and color, consistent with the current brightness field. It does not
measure a linear luminance ratio: MJPEG/color processing, partly clipped LED
cores and visible frame variation limit quantitative interpretation. No claim
is made about the Windows unit, all brightness levels, other effects or
power-cycle persistence. The earlier unaided observation remains part of the
record; these controlled images provide additional evidence, not a reason to
change the codec.

The saved **current** Wave/rainbow lighting at brightness 4 was restored with
exact raw lighting equality. Its RGB bytes are `FA FF FA`, as before this test;
this does not resolve the older attempt to restore `FF FF FF`. Camera controls
were restored to their exact saved values (auto exposure 3, exposure value 156,
automatic white balance on/temperature 4600, dynamic framerate on). Every
apply/read command returned exit 0. A final full native archive equaled the
pre-test archive in all sections; offline comparison returned `[]`.

Local evidence: `/tmp/byakko-camera-brightness-20260925-ceq1qocb`. Images and
raw diagnostic data are not committed. Product code is unchanged from `e1af8b7`.

| Artifact | SHA-256 |
| --- | --- |
| `four-02.png` | `f631b583e5b6cb82d6e5e831decd7abe22260fa3fc2890d7b9165ae108fa89a3` |
| `one-02.png` | `9bbcf925aa254d33e437b8ee06b24ebe771e17fdeb6edceb7dad9db746eb50fc` |
| `result.json` (commands/restoration/control journal) | `64802b469075db38f62021dd9b567b2292ce23a67a67df35295298a447db20bc` |
| `image-summary.json` | `f6bf88384b12cb7bd7a79125aaa4f460e1032f075e9a5af7e7e60be47134b673` |
| `after-archive.json` (native wrapper) | `da3d9eb09ff50e3ff1386aae7b8fb22df3baf8a625e387422ad2eea368fb8056` |

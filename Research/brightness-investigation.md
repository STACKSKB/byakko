# Nia87 brightness investigation, 2026-09-25

Status: unresolved physical behavior; no codec change justified yet.

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

## Proposed bounded official comparison

Pending user approval: save the connected board's complete baseline and identity;
then use only the official app to select steady green at its highest and lowest
nonzero brightness. Capture every intervening report, displayed UI values, and
visible output after equal two-second settling intervals. Use fixed ambient light
and locked camera exposure/gain where available, otherwise user observation.
Release the official session before diagnostic getter reads. Restore through
the agreed normal lighting path and distinguish visible restoration from exact
raw equality; do not use full archive Apply for this brightness test. Capture a
final read-only archive to check unrelated sections. Stop on unexpected behavior
or restoration failure and report the evidence; do not repeat setters blindly.

Identical official encoding plus equally absent dimming would support a shared
firmware/visual limitation. Different packets or visible behavior would guide
the next investigation. Neither outcome is established yet.

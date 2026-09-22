# Nia87 settings protocol notes

This is a bounded static comparison of the shipped renderer bundle and the
closed RPC capture in `Research/captures/rpc-analysis-v4.json`. It describes
the 64-byte HID payload passed through the helper. It does not send commands
and does not claim that an untested write is safe.

The Nia87 registry entries in the bundle identify `yc3121_nia87_soc` (VID
`0x3151`, PIDs `0x4011` and `0x4015`, feature report length 65) and select
class `Pft` (bundle offsets around 1,967,519 and 13,533,799). Its `otherSetting`
descriptor is the object named `Iu` (around 1,598,318): `auto: true`,
`deBounce: 10`, and Bluetooth/2.4 GHz sleep ranges of 1–60 minutes and deep
sleep ranges of 10–60 minutes. The Nia87 layout `nc` advertises report rates
125, 250, 500, and 1,000 Hz.

## Captured Nia87 reads

The capture contains one send/read pair for each command. Requests contain the
opcode followed by zeroes at the RPC boundary; replies below are the relevant
payload bytes, with the remaining bytes zero unless shown.

| Opcode | Bundle name and direction | Captured reply | Interpretation |
| --- | --- | --- | --- |
| `0x84` | Legacy/shared `GET_LEDPARAM` (`SET_LEDPARAM` = `0x04`) | `84 00 01 00 00 00 00 7b` | This is the older side-LED path. It is not Nia87's selected main-light path; Nia87's validated PB path is `0x07`/`0x87`, documented in `docs/lighting-protocol.md`. |
| `0x86` | `GET_KBOPTION` (`SET_KBOPTION` = `0x06`) | `86 00 10 00 01 00 00 79` | Profile 0; option flags byte 2 = `0x10` (`ledOff`); byte 3 is 0 (`keyboardFnKeyMatrix` false); byte 4 is 1 (`powerSaveMode` true). |
| `0x91` | `GET_DEBOUNCE` (`SET_DEBOUNCE` = `0x11`) | `91 00 01 00 00 00 00 6e` | Nia87's inherited `PB` method reads debounce from byte 2: raw value 1. |
| `0x92` | `GET_SLEEPTIME` (`SET_SLEEPTIME` = `0x12`) | `92 78 00 78 00 58 02 58 02` | Four little endian seconds: Bluetooth sleep 120, 2.4 GHz sleep 120, Bluetooth deep sleep 600, 2.4 GHz deep sleep 600. |
| `0x97` | `GET_CMD_AUTOOSEN` (`SET_CMD_AUTOOSEN` = `0x17`) | `97 00 00 00 00 00 00 68` | Boolean byte 1 is false in this read. |

The 0x92 values are 2 minutes for normal sleep and 10 minutes for deep sleep,
which are the lower bounds in the Nia87 descriptor. Zero is decoded by the
bundle as “no sleep” or “no deep sleep”; nonzero values are seconds and are
clamped to the descriptor's minute ranges by the inherited sleep getter.

## Exact field layouts

All buffers are 64 bytes and are zero-filled by the bundle before fields are
written. Byte 0 is the command. The byte positions below are payload positions
and exclude the host HID report-ID byte implied by `featureReportByteLength:65`.

### Keyboard options, `0x06` / `0x86`

The setter writes the current profile to byte 1, packs the following booleans
into byte 2, writes the Fn-matrix flag to byte 3 bit 0, and writes the
power-save value to byte 4:

* bit 0: Windows-key lock
* bit 1: system mode (`0` Windows, `1` Mac)
* bit 3: swap WASD and arrow keys
* bit 4: main LED off
* bit 5: secondary LED off
* bit 6: keyboard mode
* bit 7: keyboard lock
* byte 3 bit 0: Fn-key matrix
* byte 4: power-save value

The Nia87 capture therefore describes the main LED-off flag and Fn-key matrix
as enabled. The method passes checksum mode `0` to the common feature writer,
which the bundle's `CheckSumType` enum labels `BIT7`.

### Lighting, `0x04` / `0x84`

The older/shared lighting setter uses byte 1 for effect type, byte 2 for
`5 - speed`, byte 3 for effect value, byte 4 for the effect option, and bytes
5–7 for RGB. The captured 0x84 packet belongs to that legacy side-LED path.
It must not be used as the Nia87 main-light command: the Nia87 `Pft → CHe →
PB` override sets `FEA_CMD_SET_LEDPARAM = 0x07` and
`FEA_CMD_GET_LEDPARAM = 0x87`, with BIT8 write framing and BIT7 read framing.
See `docs/lighting-protocol.md` for that exact codec and the validated
0x07/0x87 field map.

This setter calls `writeFeatureCmd` without an explicit checksum-mode
argument. The call site therefore leaves the mode to the shared writer's
default. Its getter calls `commomFeature` with explicit mode `0` (`BIT7`).

### Debounce, `0x11` / `0x91`

Nia87 inherits the `PB` implementation. The setter places the value in byte 2,
calls the common writer with mode `0`, and waits 500 ms after a successful
write. The getter sends `0x91` with mode `0` and returns byte 2. The mode enum
labels numeric 0 as `BIT7`, but this report's checksum placement is delegated
to the native writer.

Other bundle classes use a different byte-1 layout, so the byte-2 placement is
specific to the inherited Nia87 path documented here.

### Sleep timers, `0x12` / `0x92`

The Nia87 `PB` override uses four unsigned little endian seconds fields:

| Bytes | Field |
| --- | --- |
| 1–2 | `time_bt` |
| 3–4 | `time_24` |
| 5–6 | `deepTime_bt` |
| 7–8 | `deepTime_24` |

The getter treats a zero field as the corresponding `noSleep` or
`noDeepSleep` flag. The setter writes the same fields, calls the common writer
with mode 0, and waits 500 ms after success. Because this override consumes
bytes 7–8 as the high byte of the two deep-sleep fields, the source layout
overlaps the generic BIT7 byte-7 convention. The bundle does not expose the
native helper's resolution of that overlap; do not synthesize or validate a
separate checksum at byte 7 for this report. The Nia87 descriptor clamps normal timers to
1–60 minutes and deep timers to 10–60 minutes while allowing zero as the
disabled value.

### Automatic operating-system setting, `0x17` / `0x97`

`CHe`, the direct ancestor of Nia87's `Pft`, adds these command constants. The
setter writes byte 1 as `1` or `0`, calls the common writer with mode 0
(`BIT7`), and the getter returns the boolean value of byte 1. The captured
reply has byte 1 = 0. Nia87's descriptor explicitly advertises this setting,
and the settings UI calls it “automatic” operating-system detection.

### Report rate, `0x01` / `0x81`

The shared ancestor maps report-rate values as follows:

| Rate | Byte 2 code |
| ---: | ---: |
| 1,000 Hz | 1 |
| 500 Hz | 2 |
| 250 Hz | 4 |
| 125 Hz | 8 |

The setter writes command byte 0, current profile byte 1, and the code in byte
2. The getter sends command `0x81` and returns the rate represented by reply
byte 2. Both call sites omit the checksum-mode argument, leaving it to the
shared writer/reader default. Nia87's `nc` layout lists these four rates, and
the device-information loader uses inherited `getReportRate` when the
descriptor lacks `keybordReportRate`.

The visible report-rate controls are conditional on the separate
`otherSetting.keybordReportRate` flag. Nia87's `Iu` object does not set that
flag, so the bundle does not expose the 1k/8k report-rate control for Nia87;
the static code and this capture do not prove a Nia87 report-rate write path.

## Checksum and host-lighting boundary

The bundle declares `CheckSumType.BIT7 = 0`, `BIT8 = 1`, and `NONE = 2`.
Calls that pass numeric 0 explicitly select the BIT7 mode at the helper API.
The selected Nia87 lighting writer explicitly uses BIT8 and the lighting read
uses BIT7; those details are in `docs/lighting-protocol.md`. The settings
methods below use several different call forms: keyboard options, debounce,
sleep, and auto pass numeric 0, while report-rate and the legacy 0x84 lighting
setter omit the mode and leave it to the shared writer default. The sleep
override writes through byte 8, so JavaScript alone cannot establish where the
native helper puts its checksum. Treat the captured bytes as command fields
and observed replies rather than assuming byte 7 is a checksum for every
settings command.

The official RPC client also declares `/driver.DriverGrpc/setLightType` as a
separate unary call. The nearby host wrapper constructs a `SetLight` message
with device path, `lighttype`, `screenId` (default 0), and dangle device type;
the source labels lighttype 0 as start music data, 1 as start screen data,
and 2 as stop. This is host-stream control, separate from the Nia87 feature
command `0x04`/`0x84`. The bundle has no literal `nativeappmusic` setting and
the capture only shows the RPC boundary, so it does not establish how audio
or screen samples are produced beyond this control message.

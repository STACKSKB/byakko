# Current official web Fn and sleep protocol notes

This note records static observations from the current official web bundle at
`https://iotdriver.qmk.top/static/js/main_68eaf5ce.js`. The downloaded reference
is kept at `Research/extracted/web-current/main_68eaf5ce.js`; that directory is
ignored. Bundle size is 26,004,741 bytes and its SHA-256 is
`3708DB6C84EDCE9CA8A2424D744A9EE8A1EF66A267F6F3ECA019711D9BF3FECA`.

## Nia87 class selection

The device registry entry is `yc3121_nia87_soc` (VID `12625`, PID `16401`),
display name `Nia87`, `fnLayer: 1`, and feature report length 65. The device
factory selects the current class `Emt` for this identifier. `Emt` derives from
`YHe`; the latter supplies both the complete-map and per-key “simple” commands.

The minified bundle contains both generic save methods: `setKeyConfig` calls
`currentDev.setFnKeyConfig(...)`, while `setKeyConfigSimple` calls
`currentDev.setFnKeyConfigSimple(...)`. Method existence alone does not prove
which one the Fn screen selected; the live HID trace below establishes that the
current Nia87 Fn screen selected the simple method.

## Complete-map API present in the bundle

The complete setter is `setFnKeyConfig` followed by `_setFnKeyConfig`. It
converts the complete function layer to a 504-byte matrix and sends nine
64-byte feature reports. Each report is an eight-byte header followed by 56
matrix bytes:

```
byte 0   0x10                 SET_FN
byte 1   function index       setter argument `a`
byte 2   0xF8
byte 3   0x01
byte 4   page number          0 through 8
bytes 5-6 0x00, 0x00
byte 7   0xFF - (sum(bytes 0..6) & 0xFF)
bytes 8-63 56 bytes of matrix data for this page, zero padded
```

For function index 0, page 0, the header is `10 00 F8 01 00 00 00 F6`.
For page `p` with the same index, the checksum is `F6 - p` modulo 256. The
Nia87 inherited full setter calls `writeFeatureCmd(y)` without a checksum mode
argument, so its transport wrapper default is `CheckSumType.NONE`; the setter
still places the explicit page checksum in byte 7. It waits 50 ms after the
ninth report. A failed report aborts the sequence.

This complete-map API is present in the bundle but is not the current FnSettings
save path proven by the live trace below. Other device classes contain a similar
full setter that explicitly passes checksum mode `0`; that is a separate class
path and must not be assumed for Nia87.

## Current FnSettings per-key write

`YHe` exposes `setFnKeyConfigSimple`, command `0x15` (`GET` is `0x95`). The
64-byte feature payload has the function/index selectors in bytes 1 and 2 and
the four-byte binding in bytes 8–11; it uses `writeFeatureCmd(..., 0)` and then
the short common delay. It updates one binding at a time.

The live CDB trace in `Research/captures/official-fn-hid-20260922.log` captured
the official Pause → PlayPause save at the final Windows HID boundary. The
first 13 payload bytes, including the report ID at byte 0, were:

```
00 15 00 5B 00 00 00 00 8F 03 00 CD 00
```

The API call length was 67: the helper's 65-byte HID report buffer (report ID
plus 64-byte feature payload) is sent with two additional trailing zero bytes.
Interpreting the report ID separately gives the 64-byte feature payload
`15 00 5B 00 00 00 00 8F 03 00 CD 00 ...`: command
`0x15`, function/index byte 1 equal to 0, matrix index byte 2 equal to `0x5B`
(slot 91), and binding bytes 8–11 equal to `[3, 0, 205, 0]`. The restore action
captured the same header and zero binding bytes. This is direct proof of the
selected current UI method. It does not establish why the separate `0x10`
replay failed.

The browser UI observation was that choosing “No feature set” restored both
complete maps exactly after the single-slot save. That read/restore behavior
does not change the fact that the save itself was one `0x15` report.

## Current sleep setter and checksum mode

The current Nia87-compatible sleep setter is command `0x12`
(`SET_SLEEPTIME`); the getter is `0x92`. The four values are little-endian
16-bit integers in the feature report:

```
bytes 8-9    time_bt
bytes 10-11  time_24
bytes 12-13  deepTime_bt
bytes 14-15  deepTime_24
```

The bundle calls `writeFeatureCmd(report, 0)` and then the current class's
zero-delay continuation. Thus the setter uses explicit checksum mode 0 (`BIT7`)
while its data begins at byte 8. This is separate from the old installer sleep
layout that placed values in bytes 1–8; the old layout must not be combined with
the current setter's checksum interpretation. The bundle trace does not expose
any additional native checksum placement beyond the mode argument.

No source from the official bundle is copied into product code; the bundle and
this facts-only note are research artifacts.

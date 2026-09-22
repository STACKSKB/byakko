# Nia87 host lighting boundary (static research)

Follow-up: `host-frame-protocol.md` traces the selected inherited methods to
screen opcode `0x0f` and ordinary USB music opcode `0x0e`, with field layouts
and transport distinctions. The unknown-frame statements below record the
earlier investigation stage; final helper/HID capture remains outstanding.

This is a bounded trace of the selected Nia87 path in the supplied current
official web bundle, `Research/extracted/web-current/main_68eaf5ce.js`. No
device I/O was performed for this note and no vendor source is copied into the
product.

## Confirmed keyboard feature protocol

The Nia87 main-light feature codec is the device protocol already recorded in
[`docs/lighting-protocol.md`](../docs/lighting-protocol.md): 64-byte payload,
set opcode `0x07`, get opcode `0x87`; set uses the helper's `BIT8` mode and get
uses `BIT7`. The set fields are effect byte 1, inverse speed byte 2, value
byte 3, option/color byte 4, and RGB bytes 5–7. The advertised host-dependent
effects are `LightMusicFollow2` (ID 22), `LightMusicFollow3` (ID 20), and
`LightScreenColor` (ID 21). This is the complete device-side frame fact. It
does not contain audio samples, screen pixels, or a sample-rate field.

The catalog marks both music effects with upright/separate/intersect options,
and advertises RGB/dazzle for them. It does not expose a frame rate or a host
sample layout. `LightScreenColor` has no value or speed range.

## Confirmed host control and sample production

The current bundle declares the gRPC-Web unary method
`/driver.DriverGrpc/setLightType`. Its protobuf request is `SetLight`:

| Field | Type | Meaning in bundle |
| --- | --- | --- |
| 1 | string | `devicepath` |
| 2 | enum | `lighttype`: `MUSIC2=0`, `SCREEN=1`, `OTHER=2` |
| 3 | uint32 | `screenId`, default 0 |
| 4 | enum | `dangledevtype`: `NONE=0`, `KEYBOARD=1`, `MOUSE=2` |

The wrapper calls this RPC once to start music (`MUSIC2`), once to start
screen (`SCREEN`, with the selected screen index), and with `OTHER` to stop
host streaming. The string diagnostics in the bundle label these transitions
“start music data”, “start screen data”, and “stop send data”. This is a
control-plane message to the official helper; it is not the `0x07`/`0x87`
feature frame and does not itself carry samples.

The mode watcher gives a more precise dispatch boundary than the RPC name:

* `LightMusicFollow3` (effect ID 20) subscribes to `music3Delt` and calls the
  device class's `setMusicFollow(frame, 0, is24)` for each update.
* `LightMusicFollow2` (effect ID 22) also subscribes to `music3Delt` and calls
  `setMusicFollow(frame, 0, is24)` on the `RY Native` driver. On the Windows
  `wasapi` path it additionally starts the helper with `setLightType(MUSIC2)`
  after the media-device check and a 3-second delay; that branch does not show
  a per-frame helper RPC in the renderer.
* The generic `LightMusicFollow` path (the catalog's other music-follow name)
  subscribes to `musicDelt` and calls `setMusicFollow(frame, 14, is24)`.
* `LightScreenColor` subscribes to the 40 ms `screenDataArray`; on `RY Native`
  it calls the device class's `setScreen(pixel)` for each update. The helper
  `setLightType(SCREEN, screenIndex)` is started for the non-native screen
  branch after a 3-second delay.

Therefore mode IDs 20 and 22 are not both proven to be helper-only streams.
The current web code directly computes host frames and sends them through
device-class methods for the native path; only the `LightMusicFollow2` WASAPI
branch has explicit helper start control in this trace. The exact HID report
opcodes, field layouts, and cadence of `setMusicFollow`/`setScreen` remain
unknown here and must be captured before implementing them in Rust.

The current bundle does show how the Windows renderer creates the host data:

* Music uses `getUserMedia` with desktop audio/video constraints, an
  `AudioContext` analyser with `fftSize = 2048`, and a 30 ms interval. It
  computes three derived arrays (`musicDelt`, `music2Delt`, `music3Delt`) from
  frequency data. The trace does not show the subsequent helper message or
  HID frame carrying these arrays.
* Screen uses Electron `desktopCapturer` to enumerate windows/screens, then a
  1x1 desktop `getUserMedia` stream. A canvas samples one pixel every 40 ms
  (`setInterval(..., 40)`) into `screenDataArray` (RGBA). The trace does not
  show the subsequent helper message or HID frame carrying this pixel.

These intervals are renderer sampling intervals, not established device
update rates. No current-bundle evidence establishes a Rust helper RPC for
the arrays, a HID opcode/frame layout for per-frame data, batching, or the
actual device update rate. Existing captures record one `setLightType` RPC,
but their non-HID protobuf values are intentionally redacted; they do not
prove a sample transport. The bundle has no `nativeappmusic` literal.

## Boundary for a native Rust configurator

Implementing the static `0x07` lighting codec and the catalog entries is
within the known Nia87 device boundary. Implementing music-follow or
screen-color as a native host feature requires a separate capture before
its data path can be implemented. A native
UI may expose these effects as unavailable/experimental based on this
boundary, but must not invent an opcode, frame shape, or device rate from the
renderer intervals.

## Concrete next capture experiment (official helper + CDB, do not run here)

Use the official UI with the official helper, the existing localhost RPC
proxy where applicable, and CDB on the helper's HID calls. Capture relevant
request and response bodies for `127.0.0.1:3814`, retaining timestamps and
method URLs. Do not capture
account/database traffic; filter to these paths:

1. Start from a stable ordinary lighting effect. Enable `LightMusicFollow2`
   and record the exact `/setLightType` request body, then leave it running
   for at least 5 seconds. Stop it and record the `OTHER` request.
2. Repeat for `LightScreenColor`, once with screen 0 and once with another
   available screen, recording the `SCREEN` request and the stop request.
3. Decode only the `SetLight` protobuf fields above. Compare the CDB request
   count and body sizes during the 5-second windows against all other RPC
   methods. A repeated binary request or a new streaming method identifies a
   candidate sample path; preserve its raw body for offline decoding.
4. In parallel, capture the helper's HID feature traffic at the Windows HID
   boundary (a read-only trace first, then the reversible lighting state
   restore procedure already used by this repository). Correlate timestamps
   with the 30 ms music and 40 ms screen renderer ticks. Look for a changing
   opcode/payload after the one-time `setLightType` control call.

Evidence needed before native implementation is: the helper-facing method or
stream carrying sample data, its payload fields and cadence, the helper's
resulting HID report opcode and checksum mode, and a reversible stop/restore
observation. If the RPC trace shows only the one-time `setLightType` call while
CDB shows changing HID traffic, the missing transport is inside the helper and must be
captured at the HID boundary rather than inferred from JavaScript.

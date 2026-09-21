# Nia87 configurator feature inventory

Static research source: `Research/extracted/nia-app/resources/app/dist/static/js/main_ccea61a6.js`.
The offsets below are byte offsets in that minified bundle. No device was connected and no
configurator UI was used. These notes describe the observed contract in original wording; they
are not a copy of the application implementation.

## Device registration and layers

The device table registers `yc3121_nia87_soc` twice as `Nia87` (offset 1,967,519): VID 12625,
PID 16401/16405, feature report length 65, `support_onboard: 2`, company `Nia87`, layout `nc`,
and `fnLayer: 1`. Both entries use `otherSetting: Iu`. The device factory maps this name to
`new Pft(e)` (offset 13,533,799); `Pft` extends `CHe` and only installs a model default matrix
(offset 10,719,888).

The layer-count helpers are explicit (offset 11,718,930): a missing `layer` property gives one
normal configuration layer, while `fnLayer: 1` gives one Fn configuration layer. Therefore the
observed Nia87 inventory is one normal profile/layer plus one Fn layer. The normal key-matrix
writer carries the selected profile in its command; the Fn writer carries the Fn index. Confidence:
high for the counts, because both the descriptor and helper logic are direct.

## Remapping action kinds

The shared configuration encoder `hC` (offset 7,563,446) accepts these action records:

- `forbidden`: clears the four-byte action value. Nia87's `Iu` does not set the special
  `isforbidden` flag, so the observed normal clear value is `[0,0,0,0]`.
- `combo`: `[0, modifier, key, key2]`. Modifier values are `none=0`, `ctrl=224`, `shift=225`,
  `alt=226`, and `win=227` (offset 7,614,486).
- `ConfigMouse`: mouse buttons, DPI, wheel directions, and X/Y wheel-style actions. Their shared
  four-byte forms are defined by the `AC` map (offset 7,613,961); examples include left
  `[1,0,240,0]`, right `[1,0,241,0]`, middle `[1,0,242,0]`, DPI `[20,0,0,0]`, and wheel
  forward `[1,0,247,0]`.
- `ConfigFunction`: predefined media/system/Fn/profile/DPI/application actions. The function
  table is selected for keyboard versus mouse through `IF`; the table includes media transport,
  volume, calculator, Fn, profile and DPI controls, sleep/power, app launch, and related
  system actions (offset 7,542,000).
- `ConfigUnknown`: writes an arbitrary supplied four-byte value.
- `ConfigGamepad`: selects one of the shared gamepad entries (`TF`, offset 7,542,000), including
  axes, hat directions, and named gamepad buttons.
- `ConfigMacro`: `[9, play_mode, macro_index, 0]`.
- `ConfigSnap`: `[22, number, keyCode, 0]`.

The matrix reader classifies zero and `[0,0,3,0]` as forbidden, byte 0 as combo when zero, byte 0
as mouse when one, and function records for bytes 2, 3, 8, 10, 13, 14, 18, and 19 (offset
15,737,075). This is the useful decode boundary for a native implementation. Confidence: high
for action kinds and byte forms; the exact localized names in the function table are intentionally
not reproduced here.

## Macro storage and encoding

`CHe` declares `FEA_CMD_SET_MACRO_SIMPLE = 22`, `FEA_CMD_GET_MACRO_SIMPLE = 150`, and
`MACROMAX = 50` (offset 9,980,678). The UI-side capacity check sets Nia87's device-wide macro
limit to 50 because the 20-macro exception applies only to names containing `yc200`, `yc300`, or
`yc400`; Nia87 is not one of those names (offset 15,739,800). The allocator starts at index 0 and
its inclusive `<= 50` check exposes index 50 as an apparent off-by-one. A native implementation
should treat 0..49 as the conservative 50-slot range until a device read/write confirms whether
index 50 is accepted.

The three observed play modes and their action-byte values are:

| Configurator mode | value in `[9, mode, index, 0]` |
| --- | ---: |
| `repeat_times` | 0 |
| `on_off` | 1 |
| `touch_repeat` | 2 |

The logical macro buffer is 256 bytes. Bytes 0–1 hold `repeatCount` as little-endian `u16`; the
variable-length event stream begins at byte 2. The encoder emits 56-byte payloads in 64-byte
feature reports and can write up to five chunks. Each set request has an eight-byte header:
command 22, macro index, chunk number, payload length 56, final-chunk flag, then three zero bytes;
the remaining 56 bytes carry the corresponding slice of the logical buffer. The traced Nia87 reader requests
four 64-byte chunks with command 139 (`0x8b`), slot in byte 1 and page in byte 2,
then concatenates the returned payloads (offsets 9,980,678 and 13,735,344). The length guard marks
a macro full once the encoded cursor reaches 248 bytes (offset 13,743,000). Treat 248 bytes as the
safe encoded limit unless device testing proves the final padding behavior otherwise. Confidence:
high for command IDs, modes, buffer/chunk sizes, and header fields; medium for the practical
usable-byte interpretation because the guard and final padding are separate code paths.

The event stream is not a sequence of fixed four-byte records. Keyboard and mouse-button events
start with one action byte (keyboard usage 4–239, or the third byte from the `AC` mouse map), then
an action/delay byte. Bit 7 of that second byte is down when set and up when clear. Its low seven
bits carry a short delay; when they are zero, a little-endian `u16` delay follows, making that event
four bytes rather than two. Mouse movement uses marker 249 followed by signed `dx` and `dy`, with
the same optional long-delay tail. The reader stops at a zero terminator and removes zero-delay
records. The parser is `buffToMacroEvents` at offset 13,736,035; the matching writer is `_setMacro`
at offset 9,986,612.

## RGB lighting

Nia87's `nc` layout contains `light: {isFormal: true, isRgb: true, types: [...]}` and
`reportRate: [125, 250, 500, 1000]` (offset 1,713,931). The catalog contains these 22 modes:

`LightAlwaysOn`, `LightBreath`, `LightNeon`, `LightWave`, `LightRipple`, `LightRaindrop`,
`LightSnake`, `LightPressAction`, `LightConverage`, `LightSineWave`, `LightKaleidoscope`,
`LightLineWave`, `LightLaser`, `LightCircleWave`, `LightDazzing`, `LightRainDown`, `LightMeteor`,
`LightPressActionOff`, `LightUserPicture`, `LightMusicFollow3`, `LightMusicFollow2`, and
`LightScreenColor`.

Most animated modes expose value and speed ranges 0–4. The catalog's directional/options are:

- Wave: right/left/down/up.
- Snake: `z`/`return`.
- Kaleidoscope: `out`/`in`.
- Line wave: right/left.
- Circle wave: anti-clockwise/clockwise.
- User picture: picture slots `1`, `2`, `3`.
- Music follow 3 and 2: upright/separate/intersect.

RGB and dazzle flags are present on the catalog entries that support them; `LightNeon` and
`LightUserPicture` do not advertise RGB in the layout entry. The selected `PB` encoder on Nia87's
`Pft → CHe → PB` path uses `FEA_CMD_SET_LEDPARAM = 7`, a 64-byte report, type in byte 1,
speed transformed from the common maximum of 4 in byte 2, value in byte 3, option/dazzle in byte
4, and RGB bytes 5–7 (offset 7,743,227). The older shared ancestor's command 4 does not apply
to this path; see `docs/lighting-protocol.md`. The inherited `CHe` path also exposes
`setLightPicSimple`: it sends command 20 with the selected profile, a default-matrix key index,
and RGB in report bytes 8–10 (offset 9,980,678). This is explicit per-key color support, separate
from the global effect command. Confidence: high for the catalog, ranges/options, command IDs,
and per-key write shape; the catalog is declarative and does not itself prove every firmware build
implements every effect, so effect availability at runtime should remain probeable.

## Other Nia87 settings visible in the bundle

`Iu` is `{auto: true, deBounce: 10, sleep_24: {min: 1, max: 60, min_deep: 10, max_deep: 60},
sleep_bt: {min: 1, max: 60, min_deep: 10, max_deep: 60}}` (offset 1,598,318). No LED field is
present in `Iu`; RGB lighting comes from the `nc` layout catalog above. The shared base exposes
debounce and sleep commands, while the Nia87-specific descriptor does not add extra settings.

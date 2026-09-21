# Nia87 lighting protocol (static trace)

This describes the pure codec in `src/lighting.rs`. It was traced through the extracted official bundle at `Research/extracted/nia-app/resources/app/dist/static/js/main_ccea61a6.js`. No lighting command was sent to a device for this work. Offsets below refer to characters in that single-line bundle.

## Class path and command selection

Nia87 instantiates `Pft` near offset 10,719,888. `Pft` extends `CHe` (near 9,980,682), which extends `PB` (near 7,729,263). Neither `Pft` nor `CHe` replaces `setLightSetting` or `getLightSetting`; the `PB` methods at approximately 7,743,227 and 7,746,789 are the inherited implementation. `PB` sets `FEA_CMD_SET_LEDPARAM = 7`, `FEA_CMD_GET_LEDPARAM = 135`, `MAXSPEED = 4`, `DAZZLE = 8`, and `NORMAL = 7`. An older shared ancestor has LED commands 4/132; those are **not** the selected commands on this path. The earlier `docs/feature-inventory.md` statement citing command 4 describes that ancestor and is superseded by this trace.

`PB.setLightSetting` builds a 64-byte zero-initialized report. It sets byte 0 to `0x07`, byte 1 to the effect ID, byte 2 to `4 - speed` (or 4 for a mode without a speed field), byte 3 to value/brightness (or 4 for `LightScreenColor`), byte 4 to option and color mode, and bytes 5–7 to RGB where applicable. It calls `writeFeatureCmd(report, 1, 500)`. The checksum enum at offset 1,420,518 is `BIT7=0`, `BIT8=1`, `NONE=2`, so this requests **BIT8** at byte 8. The helper's checksum implementation is outside the bundle; the native codec uses the same ones-complement sum convention already confirmed for BIT7, extended over bytes 0–7 for BIT8. A later live brightness 4 -> 3 -> 4 test on ripple mode passed repeated readback and restoration, confirming this byte-8 formula on the attached firmware. See `live-evidence.md`.

`PB.getLightSetting` sends a 64-byte request with byte 0 `0x87` through `commomFeature(request, 0)`, selecting BIT7 at byte 7. The request header is `87 00 00 00 00 00 00 78`. The response decoder reads bytes 1–7 as effect, inverse speed, value, option/color mode, and RGB. It does not require a response opcode or verify a response checksum. `Lighting::decode` retains all 64 response bytes, including unknown/reserved bytes, instead of normalizing an unrecognized mode. A host HID API may require an additional leading report-ID byte outside this payload.

The native `device::read_lighting` probe is deliberately stricter than that decoder: it places an `0x80` identity read before each of two `0x87` reads, requires the corresponding opcode echoes, and requires both complete 64-byte LED replies to match. This establishes a stable read for the captured device state. It does not establish visible LED behavior or visible LED output. BIT8 framing was subsequently verified by the backed-up write/restore test.

## Advertised modes and options

The Nia87 `nc` layout catalog around offset 1,713,931 advertises 22 modes. `LightOff` is inherited as effect 0 but is not one of those 22 catalog entries. The inherited `PB` writer also recognizes effect IDs 23, 24, and 66; the Nia87 catalog does not advertise them, so the native validated writer excludes them.

| ID | Bundle name | Option indices |
| ---: | --- | --- |
| 0 | `LightOff` | — |
| 1 | `LightAlwaysOn` | — |
| 2 | `LightBreath` | — |
| 3 | `LightNeon` | — |
| 4 | `LightWave` | right, left, down, up |
| 5 | `LightRipple` | — |
| 6 | `LightRaindrop` | — |
| 7 | `LightSnake` | z, return |
| 8 | `LightPressAction` | — |
| 9 | `LightConverage` | — |
| 10 | `LightSineWave` | — |
| 11 | `LightKaleidoscope` | out, in |
| 12 | `LightLineWave` | right, left |
| 13 | `LightUserPicture` | 1, 2, 3 |
| 14 | `LightLaser` | — |
| 15 | `LightCircleWave` | anti-clockwise, clockwise |
| 16 | `LightDazzing` | — |
| 17 | `LightRainDown` | — |
| 18 | `LightMeteor` | — |
| 19 | `LightPressActionOff` | — |
| 20 | `LightMusicFollow3` | upright, separate, intersect |
| 21 | `LightScreenColor` | — |
| 22 | `LightMusicFollow2` | upright, separate, intersect |

The catalog gives value range 0–4 for every mode except `LightOff` and `LightScreenColor`. It gives speed range 0–4 for ordinary animated modes, excluding `LightAlwaysOn`, `LightUserPicture`, both music modes, and `LightScreenColor`. It advertises RGB and dazzle for the ordinary color modes and both music modes; `LightNeon` and `LightUserPicture` do not advertise them. The code preserves the catalog spelling `LightConverage`.

For regular RGB modes, byte 4's low nibble is 7 for normal color or 8 for dazzle. Direction and other options occupy the high nibble. Music modes instead use low nibble 4 for normal and 0 for dazzle. `LightUserPicture` uses only the option high nibble and overrides bytes 5–7 with `00 C8 C8`. The writer substitutes RGB `FA FF FA` for literal white `FF FF FF`; its reader converts that sentinel back to white. The codec follows this write behavior while exposing raw response RGB separately so bytes are never hidden.

## User picture and per-key color

The inherited `_getLightPic` method near offset 7,648,644 sends six BIT7 `FEA_CMD_GET_USERPIC` (`0x8c`) page requests, with page number 0–5 in byte 2 and profile byte 1 at zero. It concatenates the six full 64-byte replies into 384 bytes, or 128 RGB triples by default-matrix index. There is no response header removal in that method. The native codec returns those triples without conflating matrix index with USB HID usage. Its page requests are read-only; the catalog's picture options select a displayed picture slot, while this inherited read returns the raw current picture data.

`CHe.setLightPicSimple` near offset 9,986,904 accepts a key, resolves it through `findIndexInDefaultMatrix`, then writes command `0x14`, profile index in byte 1, resolved matrix index in byte 2, and RGB in bytes 8–10, using BIT7 checksum mode. Nia87 advertises one profile, index 0. The native `per_key_color_report` takes the resolved index explicitly; a caller must use the layout map before invoking it. This report shape is statically verified, while visible LED behavior on a physical Nia87 remains untested.

## Related settings

The Nia87 descriptor around offset 1,598,318 lists `deBounce: 10`, `sleep_24` and `sleep_bt` ranges with minimum 1 and maximum 60 minutes, deep-sleep minimum 10 and maximum 60, and report rates 125/250/500/1000 in the layout. Those are distinct settings and are not encoded by `lighting.rs`. Their inherited command paths need separate validation before native write controls are added.

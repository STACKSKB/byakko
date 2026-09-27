# Official Nia87 lighting catalog audit, 2026-09-27

The reported `Unrecognized Nia87 lighting fields` error is a decoder gap, not
a missing mode in the advertised Nia87 catalog. This audit inspected the
retained official Driver 2.1.97 bundle at
`Research/captures/onboard-profile-20260927/main_ccea61a6.original.js`.
It did not launch the official UI or write to the keyboard.

## Selected device and catalog

Character offsets refer to that unmodified, single-line bundle:

- Nia87 descriptors near 1,967,519 and 1,967,739 select layout catalog `nc`.
- `nc` near 1,713,932 advertises 22 lighting types. These match Byakko's
  effects 1 through 22, including Neon, per-key picture, Music Follow 3,
  Music Follow 2, and Screen Color. Off is an additional inherited command.
- Neon advertises brightness and speed ranges 0–4, with no color, rainbow,
  or option control. The other mode controls match the current native catalog.
- Device dispatch near 13,533,799 selects `Pft`, inheriting the lighting
  implementation through `CHe` and `PB`.

The generic inherited decoder also understands Train (23), Fireworks (24),
User Color (66), and options on some modes which this board does not advertise.
Those generic branches are not evidence that the Nia87 UI is missing controls;
they must not be added as board capabilities solely because they exist in a
shared multi-device implementation. In particular, generic Neon has a
Default/Random interpretation, but the Nia87 catalog exposes neither option.

## Failing state and decoder behavior

The root investigation read the currently failing device state as a 64-byte
response beginning `87 03 02 04 00 FF FF FF`, followed by 56 zero bytes.
The earlier official physical-mode capture contains the same Neon response.
Its effect is 3, inverse speed is 2, brightness is 4, and the color nibble is 0.

Before the fix, `recognized_setting` accepted a color nibble of 7 for Neon but
rejected 0 because the mode has no RGB capability. This imposed an RGB-mode
restriction on an effect without an RGB field.

The selected official getter near 7,746,788 handles these non-RGB modes as
follows:

| Mode | Official getter interpretation |
| --- | --- |
| Off (0) | Ignores all remaining lighting fields. |
| Neon (3) | Reads brightness and inverse speed; ignores low color nibble and RGB. Its generic high-nibble option is not advertised by Nia87. |
| Picture (13) | Reads brightness; maps the complete option byte through the generic selector table (0, 16, 32, etc.), with fallback to selector 1. Nia87 advertises three selectors only. |
| Screen Color (21) | Returns the host-screen mode with a fixed value of 4; ignores remaining response fields. |

The bounded correction should recognize the observed Neon color nibble 0
without inventing new Nia87 modes or options. Keep unknown values and the full
raw response lossless, and retain strict validation of fields actually exposed
by the board catalog. This evidence does not require changing setter bytes or
adding reads after ordinary lighting setters.

The screenshot's overlapping dropdown text is a separate rendering issue;
catalog parity alone does not verify its correction. Rendered layout and
control-message checks belong to the implementation acceptance record.

## Implementation verification

The correction accepts Neon low nibble 0 as well as the existing setter's
low nibble 7. It retains the established setter encoding and preserves
unsupported field combinations as opaque observations. The root investigation
reports a regression test over all 19 exact ordinary-mode prefixes in the
earlier physical capture, verifying editable projections and valid drafts.
A new read-only CLI read of the user's failing state now projects editable
Neon, brightness 4 and speed 2. No write or physical output test was needed
to establish this read-decoding correction.

The catalog comparison covered flags and options, not just names: brightness
0–4 on all 21 non-screen advertised effects; speed 0–4 except steady,
picture, music and screen; RGB/rainbow on all except Neon, picture and screen;
Wave's four directions, Snake's two patterns, Kaleidoscope's two directions,
Line Wave's two directions, Circle Wave's two directions, three picture
selectors, and three patterns on each music mode. These match the native
catalog. All 22 modes were statically checked, while the captured hardware
sequence covers 19 onboard modes; this is not new physical verification of
every host effect or lighting output.

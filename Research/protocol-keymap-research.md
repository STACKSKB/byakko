# Nia87 stock HID keymap protocol notes

These are static observations from the bundled Nia87 application, chiefly `Research/extracted/nia-app/resources/app/dist/static/js/main_ccea61a6.js`. Offsets below are JavaScript string (UTF-16 code-unit) offsets in that one-line file, useful with a substring extractor. Two read requests were also checked against a connected stock Nia87; that limited observation is identified separately below. The bundle does not expose the native `sendMsg` implementation.

## Device and implementation path

The registry entries around offset 1,967,739 identify `yc3121_nia87_soc` as Nia87, with vendor ID `0x3151`, product IDs `0x4011` and `0x4015`, usage page `0xffff`, usage `2`, and `featureReportByteLength:65`. Both variants select class `Pft` around offset 13,533,799. The inheritance chain is `Pft → CHe → PB → rB → UD → PD → sD → jC → tC`. `Pft` supplies the 512-byte default matrix, while the read and write methods below come from ancestors. The registry declares one Fn layer (`fnLayer:1`) and no `layer` or `profileLayer` value. The UI helpers around offset 11,718,930 therefore load one base layer (index 0) and one Fn layer (index 0) for this device. `support_onboard:2` is present but does not drive those layer counts.

The JavaScript transport around offsets 7,553,499–7,555,800 sends a 64-byte command buffer through `sendMsg`, then reads a 64-byte response through `readMsg`. `commomFeature` sends and reads as a pair. The `featureReportByteLength:65` registry value is consistent with a leading report-ID byte in a host HID API; the 64-byte protocol payload described here excludes that host byte. `CheckSumType` is defined around offset 1,420,518 as `BIT7=0`, `BIT8=1`, `NONE=2`. Calls passing numeric `0` request **BIT7**. Although the native checksum code is absent, live reads confirmed that BIT7 places `0xff - (sum of bytes 0…6 modulo 256)` at payload byte 7 for the identity requests. The response retained that checksum byte.

## Read-only identity and state

| Purpose | 64-byte request payload, with BIT7 checksum at byte 7 | Response interpretation | Evidence |
| --- | --- | --- | --- |
| Firmware version | Byte 0 = `0x80`; bytes 1–6 zero; byte 7 = `0x7f`; rest zero | Bytes 1–2, little-endian unsigned 16-bit version. The method does not validate response opcode. | `sD` command constants ~7,641,878; `PB.getFirmwareVersion` ~7,736,267 |
| Current profile | Byte 0 = `0x85`; bytes 1–6 zero; byte 7 = `0x7a`; rest zero | Byte 1 is profile index and is cached by the app. The method does not validate response opcode. | `sD` constants ~7,642,149; `PD.getCurrentProfile` ~7,672,150 |
| Base keymap page | Byte 0 = `0x89`, byte 1 = profile index (default is cached current profile or 0), byte 2 = page `0…7`, byte 7 = BIT7 checksum; rest zero | Entire 64-byte response is appended as map data, giving 512 bytes over eight requests. No header is stripped or checked in this method. | `sD` constants ~7,642,415; `PD._getKeyMatrix` ~7,677,521 |
| Fn keymap page | Byte 0 = `0x90`, byte 1 = Fn layer index (0 for Nia87), byte 2 = page `0…7`, byte 7 = BIT7 checksum; rest zero | Entire 64-byte response is appended as map data. | `jC._getFnKeyMatrix` ~7,613,341; `jC.getFnKeyConfig` ~7,589,141 |

`jC.getKeyConfig` (~7,588,645) calls `_getKeyMatrix`, then converts each 4-byte matrix entry into a UI binding. `jC.getFnKeyConfig` follows the same path using `_getFnKeyMatrix`. The current-profile read is separate from the keymap read; the app does not need to change the active profile to read a specified map index. A response to one of these reads should be associated with its request before interpreting it: the bundled methods do not reject a stale response whose first byte belongs to another command.

## Matrix slots and physical keys

`Pft` assigns a 512-byte default matrix near offset 10,717,018 and uses it near 10,719,888. It contains 128 consecutive 4-byte slots. A normal default keyboard key has form `[0, 0, HID-usage, 0]`; zero slots are unused. There are 88 nonzero HID-usage slots (including two unlabeled ISO positions) plus one special Fn slot at index 59, `[10, 1, 0, 0]`. For example, Esc is slot 0 (`[0,0,41,0]`) and A is slot 9 (`[0,0,4,0]`). The last two slots, 126 and 127, are empty. The Nia87 visual layout is associated with `nF.layout` near offset 6,874,067; UI coordinates are separate from matrix order.

The matrix walks the board in six-position columns. These labels are a derived index to the vendor default and layout, not a copy of its four-byte array. A dash means the default slot is empty. The two HID usages marked “unlabeled” occur in the matrix but not in `nF.layout`; they should remain addressable by physical slot number.

| Slot range | Keys in increasing slot order |
| --- | --- |
| 0–5 | Esc; grave; Tab; Caps Lock; left Shift; left Ctrl |
| 6–11 | —; 1; Q; A; usage 100 (unlabeled); — |
| 12–17 | F1; 2; W; S; Z; left Win |
| 18–23 | F2; 3; E; D; X; left Alt |
| 24–29 | F3; 4; R; F; C; — |
| 30–35 | F4; 5; T; G; V; — |
| 36–41 | F5; 6; Y; H; B; Space |
| 42–47 | F6; 7; U; J; N; — |
| 48–53 | F7; 8; I; K; M; right Alt |
| 54–59 | F8; 9; O; L; comma; Fn |
| 60–65 | F9; 0; P; semicolon; period; Application |
| 66–71 | F10; minus; left bracket; apostrophe; slash; right Ctrl |
| 72–77 | F11; equals; right bracket; usage 50 (unlabeled); right Shift; left arrow |
| 78–83 | F12; Backspace; backslash; Enter; up arrow; down arrow |
| 84–89 | Print Screen; Insert; Delete; Home; End; right arrow |
| 90–95 | Scroll Lock; Pause; Page Up; Page Down; —; — |
| 96–127 | All empty in the vendor default |

The initial base map captured from the connected keyboard in `Research/captures/keymaps-initial.json` matches all 128 vendor default slots byte for byte. This supports using the mapping above as the board's physical slot identity even after a future remap changes the device's current bindings. The captured Fn map has 32 nonzero slots; both captured maps have zeroes in slots 126 and 127.

Byakko offers normal keymap edits only for those 88 ordinary populated slots. The ANSI drawing exposes 86 of them; the two unlabeled ISO positions remain addressable by slot number. The special Fn slot and default-empty positions are retained verbatim in backups, but planned edits to them are rejected before device access. Recovery can still restore an unexpectedly changed raw slot.

For a slot `i`, its raw 4-byte value is at matrix offsets `4i…4i+3`. Read page `floor(i/16)` contains it at response offsets `4(i mod 16)…+3`. The bundled lookup around offset 7,599,894 locates a key by its original HID usage, with an occurrence index for duplicate usages; special composite entries are matched as full four-byte values. The simple setter uses this physical slot index. A product implementation should preserve the four-byte entries and the two trailing zero slots when reading; the default matrix is evidence for locating physical slots, not a substitute for the device's current map.

## Writes and completion behavior

The full base-map setter selected for Nia87 is `PB._setKeyConfig` near offset 7,737,942. It writes **nine** 64-byte feature payloads, for pages `0…8`, with this structure:

| Payload offset | Meaning |
| --- | --- |
| 0 | `0x09` (`FEA_CMD_SET_KEYMATRIX`) |
| 1 | Explicit profile index, or cached current profile/default 0 |
| 2–3 | `0xf8,0x01`, the value 504 as little-endian bytes |
| 4 | Page number `0…8` |
| 5–6 | Zero |
| 7 | `255 - ((sum of bytes 0…6) & 255)` |
| 8–63 | 56 bytes from matrix offsets `56×page…56×page+55`, zero-padded if short |

Each page uses `writeFeatureCmd` with its default checksum mode, `NONE` (enum value 2), because the eight-byte header already includes an explicit checksum. It returns failure if any page send fails, then waits `BIGCMDDELAY` (1,000 ms) after all nine sends. There is no separate commit command in this method. Nine pages carry 504 matrix bytes, corresponding to slots 0–125; Nia87's remaining two slots are empty in the vendor default. For slot `i≤125`, its write page is `floor(i/14)`, and its four bytes begin at payload offset `8 + 4(i mod 14)`.

The inherited full Fn-map setter (`jC._setFnKeyConfig`, ~7,587,950) uses command `0x10`, Fn layer in byte 1, the same 504-byte nine-page layout, and the same explicit header checksum; it waits 50 ms after the pages. The higher-level `setKeyConfig` and `setFnKeyConfig` methods (~7,585,931 and ~7,586,587) first convert UI bindings into a whole matrix, then call these full-map writers. The base setter can also write referenced macro data before the map pages.

`CHe` adds a single-key path near offset 9,984,500. `setKeyConfigSimple` sends command `0x13`, profile byte 1, physical slot byte 2, and a four-byte binding at bytes 8–11; it passes BIT7 to the native transport and then calls a short post-send helper. `setFnKeyConfigSimple` near 9,985,250 has the same layout with command `0x15` and Fn layer byte 1. Neither method contains a separate commit request. The visible bundle does not prove whether firmware persists these changes immediately or on its own later.

The profile-selection write is distinct: `PD.setCurrentProfile` around 7,671,860 sends command `0x05`, byte 1 = selected profile, with BIT7, and updates its cached profile after success. That selection is not required for a read using an explicit map profile index.

## Confirmed read checksum and limits

The explicit full-write header sums bytes 0–6 and places the complement in byte 7, making the eight header bytes total `0xff` modulo 256. A connected stock Nia87 accepted the same BIT7 framing for the read-only `0x80` and `0x85` requests: the requests ended in `0x7f` and `0x7a` respectively; their responses retained those bytes. The version reply began `80 00 01` (raw version `0x0100`), and the profile reply began `85 00` (active index 0). These observations verify the identity-request framing, not the content or persistence of any keymap write.

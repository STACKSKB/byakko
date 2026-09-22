# Nia87 host stream opcode correction

This is a protocol-fact trace of the supplied `Research/extracted/web-current/main_68eaf5ce.js` bundle, checked against `Research/captures/host-lighting-final-2.log`. The bundle was inspected as data; no source, tables, or assets were copied into the product.

## Why the first trace was wrong

`Research/host-frame-protocol.md` followed the selected Nia87 class (`Emt`), its parent (`YHe`), and then jumped to the earliest generic feature class. That generic class defines audio as decimal 14 (`0x0e`) and screen as decimal 15 (`0x0f`). It is **not** the effective pair for Nia87. The actual ancestry is `Emt → YHe → EG → lG → JC → RC → AC → $Y → YY`. `AC` replaces the inherited audio and screen command fields with decimal 13 (`0x0d`) and decimal 14 (`0x0e`). Neither `YHe` nor `Emt` replaces those fields. Thus inherited method bodies that access the instance's command fields use `0x0d` and `0x0e`.

`YHe` implements `setMusicFollow`, overriding the implementation below it. `EG` implements `setScreen`, overriding the earlier normal feature method. The first trace also chose the wrong screen transport. Nia87's effective screen path uses raw feature writes, not the older generic normal feature method.

## Effective USB frames on this keyboard

For ordinary USB (not the `is24`/`isblue` branches), `YHe.setMusicFollow` selects up to 32 band values. It constructs a 64-byte payload with `0x0d` at byte 0, bytes 1–7 reserved (zero before the helper's checksum insertion), and band bytes at 8–39, zero padded. It calls the raw feature writer with BIT7 checksum mode. The official live capture in music mode shows a silent frame headed `0d 00 00 00 00 00 00 f2`, consistent with complement of the seven header bytes. It does not yet prove the values or physical effect of nonzero bands.

`EG.setScreen` constructs a zero-padded 64-byte payload starting with the instance's screen opcode `0x0e`, followed immediately by the supplied color bytes, and sends it via the raw feature writer with BIT7 checksum mode. The captured screen mode frame starts `0e df df df 00 00 00 54`, whose byte 7 is the checksum complement of bytes 0–6. Captured bytes 1–3 are RGB; byte 4 was zero in this capture. The static method accepts and appends its caller's array, so byte 4 should be treated as caller-dependent until a varied live capture establishes the alpha policy. The official screen effect visibly lit the keyboard in the webcam check.

The ordinary USB branch of `YHe.setMusicFollow` uses 8 header bytes; the wireless branches use nibble packing and different framing. `EG.setScreen` also has alternate `is24`/`isblue` behavior. Those branches should be researched separately for 2.4 GHz support.

The parent app's host reactions still sample screen data at 40 ms and music at 30 ms, but these are renderer timers, not measured HID cadence. Frame values, pacing, and cleanup on leaving these effects need direct capture before shipping live streaming.

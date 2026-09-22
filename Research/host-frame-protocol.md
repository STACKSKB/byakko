# Nia87 host frame protocol (static bundle trace)

**Superseded:** the opcode and screen-transport conclusions below were disproved by live capture. See [the corrected inheritance trace](host-frame-correction.md) and [native visual verification](host-stream-live-verification.md). This historical record is retained to explain the failed first experiment; do not use its 0x0f/0x0e opcode pair for Nia87.

Source: supplied current bundle `Research/extracted/web-current/main_68eaf5ce.js`, read as text only. These are wire-layout facts from the selected Nia87 inheritance path (`Emt` → `YHe` → base feature class); no device I/O or vendor code was copied into the product.

## Music follow on this selected path

`YHe` overrides `setMusicFollow`. It takes up to 32 entries starting at the caller's offset (0 for music effects 20/22). If fewer than five entries are nonzero and the offset is positive, it shifts the slice backward until that condition clears or offset reaches zero. Its audio opcode is inherited as decimal 14 (`0x0e`). For a device without `is24` or `isblue`, the report is 64 bytes: opcode at byte 0, seven zero header bytes at 1–7, then the selected entries from byte 8, padded with zeros. It calls `writeRawFeatureCmd(report, 0, 0)`: checksum type BIT7 (enum 0), no wrapper delay, raw-feature transport. This is the relevant ordinary Nia87 branch if the selected device flags are false; verify the flags for a particular SKU before assuming it.

The override also has explicit alternate branches. For `is24` or `isblue`, it packs pairs of entries into one byte at offsets 1 onward (low nibble first). An `is24` call with the third argument true first sends a next-packet-length command with half the selected entry count. Both branches use raw-feature writes with checksum type NONE (enum 2) and zero delay. The `isblue` branch additionally prepends bytes `0x56, 0x0c`. These branches are recorded to keep their layouts distinct from the ordinary Nia87 report.

The renderer updates `music3Delt` on a 30 ms sampling interval and invokes this method on observed changes. The method itself has no loop or fixed device interval. Its raw-feature wrapper uses the helper's raw-feature RPC, so the 30 ms interval is an upper-level sampling target, not a measured HID cadence.

## Screen color on this selected path

`YHe` does not override `setScreen`; the base class implementation is inherited through `YHe` and `Emt`. Its inherited opcode is decimal 15 (`0x0f`). The method makes a 64-byte zero-padded report with opcode at byte 0 and the supplied pixel entries starting at byte 1. The caller supplies a one-pixel RGBA array from `screenDataArray`; thus bytes 1–4 carry R, G, B, A in that order, subject to the bundle's array-to-byte conversion. It calls `writeFeatureCmd(report, 0)`, selecting BIT7 checksum and the normal feature path. The wrapper has a default 10 ms pre-write delay; the method does not explicitly request a readback.

The screen sampler updates `screenDataArray` every 40 ms and the native reaction invokes `setScreen` on changes. There is no device timer in `setScreen`; actual HID cadence can vary with reaction coalescing and helper latency.

## Transport and mixed writes

The shared `writeFeatureCmd` wrapper waits its configured delay and calls the helper send function with the path, payload, checksum mode, and dongle type. The raw variant calls the helper raw-feature send function. Neither wrapper does a GetFeature/read automatically after a SET. A separate `commomFeature` helper explicitly performs write followed by `readFeatureCmd`, and callers can also invoke a read separately. Thus an observed SET/GET pair is a caller choice, not a property of every `writeFeatureCmd` or `sendMsg` SET request.

This trace resolves the encoder layouts and selected transport, but does not establish observed USB report bytes or a measured update cadence. The helper applies the requested BIT7 checksum before HID delivery; a capture remains useful to validate the exact physical report framing and inter-report timing.

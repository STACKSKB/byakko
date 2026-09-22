# Nia87 Fn single-key write correction

## Observed conflict

A guarded F24 (`00 00 73 00`) Fn single-key attempt at physical Pause slot 91
using command `0x15` failed complete-map verification. The exact first
post-write mismatch was not retained. During rollback, a base `0x13` Pause
restore followed by a `0x15` Fn-zero report cleared **base** slot 91; the Fn
slot remained zero. A subsequent base `0x13` restore recovered both maps,
verified against the original backup. This directly shows that `0x15` can
affect the base map on the tested firmware. It does not identify every effect
of the first write or establish behavior for other indices or states.

## Static call-chain check

The supplied official bundle is
`Research/extracted/nia-app/resources/app/dist/static/js/main_ccea61a6.js`;
offsets below are character offsets in its minified single line. This record
extracts protocol facts without reusing its implementation.

1. The device factory at approximately `13,533,000` selects `Pft` for
   `yc3121_nia87_soc`. The class declaration at approximately `10,715,800`
   shows `Pft` extending `CHe`, adding the Nia87 default physical matrix but
   no Fn single-key writer override.
2. The `CHe` constructor at approximately `9,981,200` assigns the symbolic
   Fn simple set command the decimal value `21`, or hexadecimal `0x15`.
   Its Fn simple method at approximately `9,985,250` finds the physical slot
   in the default matrix, converts the desired four-byte binding, and sends a
   64-byte feature payload. Byte 0 is `0x15`, byte 1 is its Fn-index argument,
   byte 2 is the physical slot, and bytes 8–11 are the binding. The call to
   `writeFeatureCmd` requests checksum type `0` (BIT7). There is no preceding
   Fn-mode selection or separate commit call in this method. Its optional
   default-Fn reset branch changes only the chosen binding bytes.
3. The application single-key caller at approximately `15,742,000` invokes
   this method when its UI state is Fn mode, with arguments `(config,
   fnIndex, fnSysType)`. `CHe` consumes only the first two arguments; the
   operating-system selector has no effect on its report fields. The base
   branch calls `setKeyConfigSimple` instead, which uses command `0x13` and
   the same physical-slot and binding positions.
4. A different application branch at approximately `15,736,600` calls
   `setFnKeyConfig` for a full-map save. The inherited full Fn writer at
   approximately `7,587,950` sends nine 64-byte payloads with command
   `0x10`, Fn index in byte 1, `0xf8` in byte 2, `0x01` in byte 3, page
   `0…8` in byte 4, BIT7 checksum in byte 7, and 56 matrix bytes in bytes
   8–63. This is distinct from command `0x15`, but a later guarded live
   attempt also failed complete-map verification.

For the attempted index 0, slot 91 simple report, the header should be
`15 00 5b 00 00 00 00 8f` under BIT7, followed by the four binding bytes at
offset 8. The source trace therefore supports the original packet layout but
conflicts with the observed base-map side effect. Changing the native packet
to another guessed index or checksum would be unsupported.

## Product implication

Keep Fn writes disabled while retaining Fn reads and backups. Neither `0x15`
nor `0x10` passed a verified Fn-only write. The first `0x15` and `0x10`
post-write mismatch details were not saved, so their exact effects must not
be inferred. A future probe would need to preserve both complete post-write
maps before rollback and use the established recovery path.

## Follow-up: UI index and full-map construction

A second guarded live attempt using the nine-page `0x10` full Fn writer sent
all pages, then failed complete-map verification. The post-write mismatch was
not saved, so this does not distinguish a no-op from a wrong-layer write or
another effect. The original base and Fn maps were restored and verified.

The UI index is zero-based in the actual wire call. The store constructor near
offset `15,684,043` initializes `fnIndex` to `0` and `fnSysType` to `"win"`.
The Nia87 registry entries near `1,967,519` specify `fnLayer:1` and no
`layer`, `profileLayer`, or `fnSysLayer`. The helper near `11,718,930`
therefore computes one normal local configuration and one Fn configuration:
its *local list* position for Fn is 1, while the Fn wire index is still 0.
Initial loading near `15,662,182` loops from Fn index 0 to less than
`fnLayer` and calls `getFnKeyConfig(0)`. A UI label near `17,289,594`
displays `fnIndex + 1` for people, but its selector passes the zero-based
index unchanged. The full and simple save callers near `15,736,855` and
`15,742,647` both pass `this.fnIndex` directly to their device methods.
Thus an off-by-one between the UI's displayed “Fn 1” and the wire header
does not explain either live result.

The full-map payload requires a second distinction. The inherited
`setFnKeyConfig` near `7,586,600` sets its internal `isFn` flag, then converts
the UI configuration list through `configsToMatrix`. That conversion near
`7,584,886` starts from the Nia87 512-byte `defaultMatrix` (the normal stock
layout) and applies each configured key change. It does **not** start from a
raw `0x90` Fn-map readback. The full writer then slices the first 504 bytes
into nine 56-byte pages. The native full-map probe instead used the current
raw Fn-map image with one changed slot. Its command, index, length fields,
page order, checksum, and payload segmentation match the source writer, but
its untouched slot values may differ from a stock UI full save. This payload
difference cannot by itself establish which firmware partition `0x10`
addresses; no alternate field values are supported by the source.

The full writer computes byte 7 explicitly and calls `writeFeatureCmd` with
no checksum argument. The inherited transport at approximately `7,553,508`
defaults that argument to `NONE`, so it leaves the writer's explicit checksum
alone. The simple writer passes checksum type `0` (`BIT7`) and lets the
transport generate it. Neither path adds a hidden Fn-mode command in these
methods. The Fn reader at approximately `7,613,341` sends eight `0x90`
page requests with Fn index 0, consistent with the captured read behavior.

The remaining explanation is unresolved. The two write paths may be
unsupported or behave differently on the tested firmware, or the full writer
may require a stock-style content image or other state not visible in this
trace. Keep Fn editing blocked; further work needs exact before/after bytes
for **both** maps and a separately designed safe probe, not a guessed index.

## Keyboard-option flag lead

A read-only `GET_KBOPTION` response was captured as
`86 00 10 00 01 00 00 79` in its first eight bytes. In the inherited
Nia87 keyboard-option reader at approximately `7,731,298`, command `0x86`
returns `keyboardFnKeyMatrix` from response byte 3 bit 0 and
`powerSaveMode` from byte 4. Thus this capture decodes the named Fn-matrix
flag as false and power-save as true. The setter at approximately
`7,730,602` uses command `0x06`, profile byte 1, the ordinary option bits
in byte 2, the supplied `keyboardFnKeyMatrix` value in byte 3, and
`powerSaveMode` in byte 4, with BIT7 checksum. The name and byte position
alone do not establish that the flag enables Fn-map writes.

The frontend calls the getter during device loading near `15,663,337` and
stores the result for settings and lighting behavior. A full-bundle search
found the `keyboardFnKeyMatrix` property only in two generic keyboard-option
getter/setter pairs, and found no frontend call to generic
`setKeyboardOption(...)`. In particular, `setIsFnMode` near `15,696,900`
changes local UI state and reloads a configuration; `setFnIndex` near
`15,697,100` changes its local zero-based index and reloads. The latter
sends a different keyboard-option command only for the separate
`keyboard3123` device class, which is not Nia87. The Nia87 Fn-settings UI
therefore provides no source evidence for setting byte 3 before a Fn edit.
No write to this flag is justified by the trace or the captured value.

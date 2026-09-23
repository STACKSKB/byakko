# Nia87 visible action coverage

This note records action entries visible in the supplied official Nia87
configurator catalog. All twelve entries in the table are now represented in
`crates/byakko-devices/src/nia87/actions.rs`. This is a static research record;
no vendor source code, table asset, or UI implementation is copied into Byakko.

## Scope and selected path

The source is the supplied extracted official bundle
`Research/extracted/nia-app/resources/app/dist/static/js/main_ccea61a6.js`.
The action catalog is at approximately UTF-16 offset 7,542,000. The keyboard
selection is the merged function map (`FF = Object.assign(AF, RF)`), where the
media/system entries (`AF`) and the general keyboard entries (`RF`) are made
available together. This is the relevant selection for Nia87's keyboard
configuration; mouse/gamepad maps are outside this note.

The shared action encoder accepts a function action and writes its four-byte
catalog value as the binding. Nia87's selected write path is `Pft → CHe`:
`setKeyConfigSimple` uses command `0x13` for the base layer and places the
binding at payload bytes 8–11; `setFnKeyConfigSimple` uses command `0x15` and
the same binding position for the Fn layer. Both methods pass the report
through the BIT7 checksum transport. The reports are 64-byte protocol payloads
(the Windows HID API may add a leading report-ID byte). There is no separate
action-specific command for these catalog entries.

The table below gives the exact four bytes selected by the catalog. Values are
shown in hexadecimal and are the binding bytes, not a complete 64-byte HID
report.

| Visible action | Catalog entry | Four-byte binding | Static confidence |
| --- | --- | --- | --- |
| Player | `播放器` | `03 00 83 01` | High |
| Calculator | `计算器` | `03 00 92 01` | High |
| E-Mail | `邮件` | `03 00 8A 01` | High |
| My Computer | `我的电脑` | `03 00 94 01` | High |
| Search | `搜索` | `03 00 21 02` | High |
| Homepage | `主页` | `03 00 23 02` | High |
| Brightness+ | `亮度加` | `03 00 6F 00` | High |
| Brightness- | `亮度减` | `03 00 70 00` | High |
| Refresh | `刷新` | `03 00 27 02` | High |
| Zoom out | `缩小` | `00 00 E3 2D` | High |
| Zoom in | `放大` | `00 00 E3 2E` | High |
| Microphone switch | `麦克风开关` | `06 80 00 00` | High |

The decimal source arrays are, respectively, `[3,0,131,1]`, `[3,0,146,1]`,
`[3,0,138,1]`, `[3,0,148,1]`, `[3,0,33,2]`, `[3,0,35,2]`,
`[3,0,111,0]`, `[3,0,112,0]`, `[3,0,39,2]`, `[0,0,227,45]`,
`[0,0,227,46]`, and `[6,128,0,0]`.

## Interpretation and limits

The first byte distinguishes the catalog's action families. The `03` records
are predefined function actions, while Zoom uses the ordinary keyboard-style
record with a special usage prefix in bytes 2–3. Microphone switch is a
separate function family (`06`) with byte 1 set to `0x80`. These distinctions
come from the catalog values and the shared decoder boundary described in
`docs/feature-inventory.md`; they should not be normalized into ordinary HID
usages.

These values establish what the official UI selects and what the selected
Nia87 encoder would place in a keymap binding. They do not prove that every
firmware revision performs the corresponding host operation, nor do they prove
physical output. In particular, the supplied live evidence verifies keymap
readback and selected media bindings, while application launch, browser
navigation, brightness, zoom, refresh, and microphone behavior remain
untested on the attached keyboard. The native implementation should preserve
unknown four-byte values and should not label these actions end-to-end
validated until reversible device tests and host-side behavior checks exist.

## Fixture audit, 2026-09-23

The saved official Fn HID trace (`Research/captures/official-fn-hid-20260922.log`)
and its before/after map captures contain a single newly selected binding,
Play/Pause (`03 00 CD 00`) at Fn slot 91. The replay and mixed-map captures add
ordinary F23/F24 key usages; the other non-ordinary records already occur in
the stock Fn baseline. Play/Pause, Fn, the observed media actions, and the
twelve catalog entries above already have matching presets or codecs. The
stock Fn map also contains other `0A` records and `0D`/`0E` records whose exact
Nia87 behavior is not established by these captures. A stored raw record alone
does not identify a safe, named action to offer for editing. There is therefore
no additional verified preset to add from the current replay fixtures.
Preserve those records as opaque bindings until an official selected-action
trace or physical behavior check establishes their meaning.

## Trace references

- Function catalog (`AF`, `RF`, merged `FF`): official bundle near offset
  7,542,000.
- Shared function/action encoding and decode classification: official bundle
  near offsets 7,563,446 and 15,737,075; summarized in
  `docs/feature-inventory.md`.
- Nia87 simple base/Fn binding writes: official bundle near offsets 9,984,500
  and 9,985,250; summarized in `Research/protocol-keymap-research.md`.

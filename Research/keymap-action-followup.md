# Nia87 keymap action follow-up (2026-09-23)

This is a read-only comparison of the current Nia87 action catalog with the
supplied official configurator bundle and saved keymaps. It records wire values
and gaps; it does not import vendor implementation or establish physical output.

## System controls added from the selected catalog

The selected official keyboard function map is the merge `FF =
Object.assign(AF, RF)` near UTF-16 offset 7,542,000 of
`Research/extracted/nia-app/resources/app/dist/static/js/main_ccea61a6.js`.
`AF` includes three distinct system controls:

| Official function | Four-byte binding | Byakko named action |
| --- | --- | --- |
| System power | `02 81 00 00` | System Power |
| System sleep | `02 82 00 00` | System Sleep |
| Computer wake | `02 83 00 00` | Computer Wake |

The selected `Pft → CHe` simple key setter places any such catalog binding in
report payload bytes 8–11 under command `0x13` for base or `0x15` for Fn;
`Research/protocol-keymap-research.md` records the report shape. Byakko now
advertises these exact records through
`crates/byakko-devices/src/nia87/actions.rs::presets`; the focused regression
checks that `adapter.rs::descriptor` offers each named action and that the
action encoder returns the catalog bytes. Existing raw `Opaque` records remain
lossless in readback and native archives.

These are **catalog and encoder facts**, not a physical acceptance result.
The saved official action trace
`Research/captures/official-fn-hid-20260922.log` selected Play/Pause, and the
before/after keymap captures do not show a new `02` binding. Power and sleep
could disrupt the host during a physical test; wake might need a sleeping host
to validate. Any product addition should distinguish exact encoding from
observed host behavior and avoid promising working power management from the
catalog alone.

## Lower-confidence or already expressible entries

The same merged map includes `r_FN = 0A 01 01 00`, app-launch slots
`06 80 00 01` through `06 80 00 03`, profile controls beginning with
`08 00`, and `Open website = 12 00 00 00`. No saved selected-action write
establishes those as useful standalone Nia87 features. Nia87 advertises one
base profile, while app and web launch may depend on the official host helper.
Do not infer stock firmware support from the shared catalog.

The stock Fn keymap at `Research/captures/keymaps-initial.json` has
`0A 0D 00 00` at slot 0. The official map gives that same value to two
different labels (`fn锁屏` and one volume/keyboard-brightness toggle). The raw
value cannot identify which label is intended, so the current opaque readback
is more accurate than assigning a name. Braces and input-method switching
are expressible as the existing Shift+[ / Shift+] / Ctrl+Space shortcut values;
adding identical presets would change labels rather than capability.

`Research/action-coverage.md` remains correct for its nineteen explicitly
audited visible entries, but those entries are not the complete merged
function map. The three distinct `02` records above now have catalog-level
support; firmware and host behavior remain unverified.

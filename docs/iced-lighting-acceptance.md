# Iced global lighting slice, 2026-09-23

The native Iced application now supplies a core lighting catalog and draft to
the same serialized device worker as keymaps and macros. The desktop renders
backend-advertised effects, ranges, options and color choices; it does not
contain Nia87 effect numbers or USB reports. `--demo` uses a memory keyboard
with different effects and ranges. The stock-firmware Nia87 adapter advertises
built-in effects 0–19. Music and screen-following effects 20–22 use a
separate host stream lifecycle and await physical Iced acceptance.

Opening the Lighting page now reads an unloaded or invalidated snapshot once
the keymap is ready. A pending keymap read finishes first. Revisiting a ready
page does not repeat the request, and a failed read needs an explicit retry.
This behavior has a memory-backend test; live window interaction is unverified.

Reading preserves the full 64-byte response as a revision. Unknown or
unrepresentable responses remain opaque. Every stage validates the complete
setting. Applying checks the exact baseline, writes a durable backup before
the setter, reads back the setting and reserved bytes, and returns a typed
recovery result if it fails. A changed keymap or macro operation invalidates
lighting trust while retaining its draft; a lighting write invalidates those
other surfaces. Conflicts and failed reads preserve staged edits. The session
rejects stale or wrong-kind completions and keeps only one device operation
active. Commands and completions use owned serializable values for a future
browser or service adapter.

The 2026-09-23 read-only probe on the attached Nia87 traversed the core
session, executor and native adapter. It read effect 5 (Ripple), brightness 4,
speed 0 and RGB `[8, 8, 8]`, matching the earlier full configuration capture.
The new completion was saved to the ignored
`Research/captures/lighting-core-executor-20260923.json` without overwriting
the baseline. No setter was sent. Pure codec tests cover every advertised
effect's option, color and range limits, raw revision retention, forged
baselines, and the native near-white alias before any write. Memory workflow
tests exercise staged edits, saves, rereads, other-draft preservation and
failed/stale result handling. Workspace tests pass. Windows
and Linux Clippy pass with warnings denied, the core checks for WASM, and the
Windows desktop release builds. The UI uses shared responsive panes and typed
style data; the lighting renderer consumes a pure core control projection.

A later read-only CLI result reported effect 1, with all other lighting bytes
unchanged. A complete archive confirmed that settings, keys, macros and
per-key colors still matched the earlier baseline. The cause of the effect
change is unknown; see `live-evidence.md`. No restore setter was attempted.

Actual Iced lighting writes, rendered GUI interaction, visual behavior on
this keyboard, power-cycle persistence and Linux runtime remain unverified.
The earlier injected-failure recovery anomaly remains an open acceptance gate.

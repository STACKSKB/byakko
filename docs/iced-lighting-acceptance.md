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

If a previous process left a recognized host-driven mode stored, the read is
shown as an active host mode rather than an editable onboard effect. The user
can explicitly choose an advertised onboard effect and apply it through the
same revision check, backup, readback and recovery transaction. Unknown host
responses stay opaque. No effect is written automatically on startup. Core,
Nia87 preflight and memory-backend CLI tests cover this path without hardware;
physical Iced exit from a leftover host mode remains unverified.

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

## Display source and point sampling (2026-09-23)

Host screen color now offers native display discovery and selection, retaining
average as the default and adding an optional normalized point (0–1000 per axis).
Capture options stay local to the OS sampler; the core/HID RGB frame contract is
unchanged. A missing explicit display fails without substituting another one.
The Windows sampler uses monitor bounds; X11 uses RandR 1.5 monitors with a root
screen fallback on older servers. Wayland capture remains unsupported.

Windows read-only `sample_displays` smoke check found DISPLAY1, returned average
[175,176,178] and center-point [246,246,246], and rejected a nonexistent display.
The sandbox denied GDI transfer; the same bounded executable succeeded outside
it. No images were saved and no HID access occurred. Native package tests and
Clippy passed; Linux devices cross-check passed. Multi-monitor hardware, Iced
interaction, physical streaming/restoration and Linux runtime remain pending.
The retained official evidence establishes selected-display 1x1 downsampling,
not an exact chosen-coordinate algorithm; point sampling is a Byakko option.

## Persistent keyboard workspace (2026-09-23)

The keyboard remains above all configuration pages. Lighting separates onboard
effects from host modes, with a dedicated scrollable parameter pane and visible
fixed-color presets/RGB controls. Host controls no longer push effect parameters
offscreen. Per-key colors preview the staged RGB map on the same keyboard and
explain that displaying stored colors requires an onboard per-key lighting effect.
All scalar settings appear together with toggles and bounded sliders.

Switching pages issues no device command. Connection refresh loads lighting,
settings and colors once, followed by passive macro discovery. Explicit Refresh/
Read controls remain for invalidated or failed snapshots. Writes still use the
existing backend backup, pacing and readback; transaction duration is unchanged.
The rebuilt Windows release was opened for user validation. Rendering and new
physical acceptance are not inferred from headless tests. Native tests and
Clippy passed; core WebAssembly and Linux devices cross-checks passed.

The user confirmed the rebuilt workspace was "Visible and usable": keyboard
persistence across tabs and onboard effect color controls were confirmed in the
running Windows release. They had not yet applied a color in that build, so
this is rendered UX acceptance, not new physical lighting-write acceptance.

## 2026-09-23 live-control UX revision (awaiting user review)

The keyboard remains centered above a bounded workspace. Key actions wrap into
compact buttons; numeric controls and settings cards no longer stretch across the
window. Global and per-key lighting use a native HSV color field, hue strip,
preview and palette swatches. Repeated key labels, hex readouts and RGB sliders
are removed. Onboard effects apply on selection; color/slider gestures coalesce
for 120 ms and retain the newest input during serialized device work. Normal
lighting interaction needs no Read or Apply click. Per-key edits select the
backend-advertised picture effect automatically before reading its color context.

Verified scalar/picture saves retain unrelated caches; backend expected-state,
backup and readback checks remain. Failed writes retain intent and stop automatic
retries. The discard confirmation is a centered modal and blocks background
edits. Macro editing can start through a foreground slot read while the library
scan continues independently. No new physical LED-write acceptance is claimed;
USB transaction latency has not been measured or reduced in this revision.

Scope ends with this UX build and tests. Further UX design and hardware acceptance
require the user's next review; do not resume unrelated parity work autonomously.

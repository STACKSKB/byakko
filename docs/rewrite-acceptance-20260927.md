# Rewrite acceptance — Windows, 2026-09-27

Source checkpoint: `8045e2c` on `codex/application-rewrite`.
The user positioned the webcam at the keyboard and requested autonomous
rendered-UI and physical checks, leaving checks requiring physical input for later.

## Baseline and tools

The native CLI selected the observed USB collection, VID `3151`, PID `4015`,
interface 2, usage page `FFFF`, usage 2. Firmware is `0100`, profile 0.
The complete diagnostic archive contains both 128-entry keymaps, fifty 256-byte
macro slots, 64 lighting bytes, 128 picture colors under the current selector,
and all four settings replies. The initial effect is Wave, brightness 4, speed 2,
right direction, rainbow. Backlight is enabled and debounce is 1.

Ignored local evidence: `Research/captures/rewrite-acceptance-20260927/`.
The initial `before.json` SHA-256 is
`63c00318f760b90b4918d4639c7d962fc4f24e836280841ad73e788703ea901c`.
Separate keymap, lighting, settings and picture snapshots and the advertised
descriptor are saved alongside it. The baseline webcam image is
`Research/captures/keyboard-camera-1790448330687106700.png`; the keyboard is
fully visible with rainbow illumination. This is qualitative color evidence,
not a calibrated brightness measurement.

The actual desktop executable launched. Windows Computer Use could list and
activate its window, but screenshot capture failed twice, including after fresh
window selection, with `SetIsBorderRequired failed: No such interface supported
(0x80004002)`. Accessibility exposed only the window/title bar, not Iced controls.
No blind coordinate actions were used. Alt+F4 closed the idle application,
confirmed by a subsequent window inventory.

The existing webcam script worked using the bundled Python runtime and access
to its existing OpenCV dependencies. The read-only WASAPI playback probe opened
at 48 kHz and sampled 18,336 frames with peak 0.000: successful capture of silence,
not evidence of a nonzero music response. No microphone recording was performed.

## Rendered review and corrections

The ignored `target/visual-review` harness imports the actual desktop view,
form and widget modules. It renders Nia87 cached observations through
`iced_runtime::UserInterface` and the existing `iced_tiny_skia` headless renderer.
It sends the real redraw event before drawing; no duplicated UI markup, product
flags or dependency changes were introduced. The macro example stages a local
four-event Ctrl+C draft with repeat count 2; it is never written to the keyboard.

Reviewed assignments/catalog/shortcuts, macros/manual composer, lighting/host
parameters, picture/native color picker, settings, archive and discard modal at
1360×800 and 1024×768, plus a 2× assignment render and tall comparison frames.
The renders identified and corrected three defects:

- Macro file controls consumed the fixed space below the keyboard, clipping the
  repeat/event/composer/assignment controls. Files and the complete editor now
  share a bounded scrolling detail pane; the library scrolls independently.
- Music-host parameters pushed Start/Stop offscreen and collapsed the onboard
  pane. Onboard and host controls now share one bounded scroll below the keyboard.
- Settings used a narrow shrinking viewport which clipped its explanatory text.
  The viewport now fills the available width. Embedded scrollbar spacing keeps
  the corrected panes' controls clear of the scrollbar.

The keyboard stays visible in these panes. Offscreen wheel events reach the
macro composer, assignment/file actions and host Start/Stop at both sizes.
Synthetic widget clicks produce `Macros(Assign("hold"))`,
`Files(Begin(ExportMacro))` and `Host(Start)`; the harness does not dispatch those
messages to effects. Before renders are in `target/visual-review/before-fixes`,
corrected renders in `target/visual-review/png-after`.

Run the local harness with:

```powershell
cargo run --manifest-path target/visual-review/Cargo.toml --target-dir target --offline
```

All 76 desktop tests, strict desktop all-target Clippy, formatting and the native
desktop build pass. This establishes layout and widget intent at the inspected
sizes, not native window gestures, physical output or the user's visual approval.

## Hardware checks and write boundary

The CLI's read-only planners accepted a proposed debounce change from 1 to 2
and a proposed base Pause binding to F13. Neither was applied. The complete
`after-readonly.json` archive compares equal to `before.json`; no captured
keymap, macro, lighting, picture or settings data changed during these checks.

Automatic approval review rejected the prepared red/green lighting test before
execution because it did not treat the broad instruction as explicit permission
for hardware writes. Explicit approval for backed-up temporary writes is pending.
No lighting, picture, settings, keymap or macro writes have run in this check.

The ignored `target/host-acceptance` harness is compiled but unexecuted. It uses
the real desktop host controller, OS sampler, core and selected-device executor
for an eight-second stream followed by explicit Stop and restoration readback.
It also supports a local webcam still during streaming. It is ready for the
pending write approval; it is not evidence of completed streaming acceptance.

## Remaining physical checks

Physical key output, macro playback, reactive effects, cable/reconnect behavior,
and power-cycle persistence need later hands-on coordination. Linux runtime and
the historical recovery/fault failures remain separate acceptance gates.

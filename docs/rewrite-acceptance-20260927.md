# Rewrite acceptance — Windows, 2026-09-27

Source checkpoint: `8045e2c`, with rendered-layout fixes at `e9bb46f`, on
`codex/application-rewrite`.
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

## Hardware checks and restoration

The initial read-only phase ended with `after-readonly.json` equal to
`before.json`. Automatic approval review initially rejected the lighting test
because it required explicit hardware-write permission. The user then answered
"Go ahead" to the request for backed-up temporary writes and restoration.
The following checks ran after that approval, with one hardware owner at a time.
No firmware flashing, fault injection or cable/power manipulation was performed.

| Check | Observation and restoration |
| --- | --- |
| Onboard lighting | Red and green settings matched diagnostic readbacks and visibly changed the keyboard. Wave/rainbow was restored, with the raw-byte exception below. |
| Scalar settings | Debounce 1 → 2 → 1 passed normal apply verification and restored the complete settings revision. |
| Keymap | Base Pause (`slot-091`, usage 72) → F13 (104) → Pause passed normal apply verification; both maps restored exactly. No physical key press was tested. |
| Macro | Slot 0 was read as all 256 bytes zero and unbound in both maps. A count-1 F13 press/release program passed normal save/readback. Developer native restoration recovered all 256 original bytes, including raw repeat zero; both maps and picture/context remained unchanged. It was never assigned or played. |
| Screen average | Actual desktop host controller, screen sampler, core and selected-device executor streamed for eight seconds. Webcam showed changed pink/white illumination. Explicit Stop restored the complete saved lighting baseline with Readback evidence. |
| Playback music | The same path streamed `music-follow-2` for eight seconds while a six-second, 480 Hz, 4%-peak local test tone played. Webcam showed nonzero green output. Explicit Stop restored the complete saved lighting baseline. No microphone recording or system volume change was used. |
| Per-key picture | Under effect 13 / option 1, F1–F3 were set green. All three diagnostic colors matched and the webcam clearly showed green under those keys, retaining the existing red WASD/arrows. All 384 saved picture bytes under that selector were restored, followed by the complete saved lighting baseline. |

Host checks use the ignored `target/host-acceptance` harness, which imports the
actual desktop controller and sampler modules. `host-screen.log` and
`host-music.log` record completion. The macro harness is in
`target/macro-acceptance`; `macro-roundtrip.log` records its result. These checks
exercise real native effects and desktop host orchestration, but do not prove
native window Start/Stop clicks, focus-loss gestures or close behavior.

Qualitative webcam evidence under `Research/captures/`:

- Red: `keyboard-camera-1790475521972805900.png`.
- Green: `keyboard-camera-1790475526500062400.png`.
- Screen average: `keyboard-camera-1790475742055053200.png`.
- Playback music: `keyboard-camera-1790475834372040600.png`.
- F1–F3 green: `keyboard-camera-1790476076767047500.png`.

### Intentional Nia87 white encoding and raw archive limits

The first ordinary lighting restoration recovered the original Wave/rainbow
setting, brightness, speed and direction, but lighting raw bytes 5 and 7 changed
from 255 to 250. The existing normal encoder deliberately maps `FF FF FF` to
`FA FF FA`; the recognized setting maps this back to semantic white.

One bounded developer archive restoration attempted the original exact lighting
bytes, with a plan changing lighting only. Its full readback mismatched; automatic
recovery verified the canonicalized state immediately preceding that attempt.
Evidence is in `exact-lighting-restore-trace.json` and
`Research/captures/backups/configuration-apply-mismatch-1790475639791181600.json`.
This records an exact-byte restoration limitation on this Windows unit, rather
than a defect in the required semantic white behavior. It is not a successful
exact restore of the original archive.
Later checks restored their saved canonicalized lighting baseline exactly.
No production workaround, guard or protocol change was added.

The subsequent [RGB boundary investigation](../Research/rgb-white-boundary-20260927.md)
separates this behavior: native literal `255,255,255` becomes `180,180,180`, while
253/254 and channel permutations are preserved. The official software writes
`250,255,250` for white and displays that tuple as white on reload. This supports
an exact-white device special case, not a general cap at 250. The follow-up
restored its complete starting archive exactly without changing production code.
The user then directed that the backend mirror the official behavior and treat
it as intentional. White encoding is therefore not an unresolved bug or release
gate. Raw archive differences remain documented; unrelated recovery failures
remain open.

### Picture diagnostic timing

The first immediate picture getter returned the old color after a successful
setter, and an immediate restoration getter still returned the test color.
After allowing two seconds to settle, the full original 384-byte picture was
confirmed restored before further writes. Subsequent checks used two- or
three-second diagnostic settling waits, verified the requested colors and then
verified complete restoration. The three-key webcam comparison resolved the
ambiguous single-key image. These are investigation waits, not a measured minimum
firmware delay or a new automatic runtime getter. Normal lighting/picture writes
continue to report transport acceptance using the existing pacing.

### Final state

One final complete sweep, `after-final.json`, matches the original keymaps, all
fifty macro slots, settings and picture under the original Wave selector exactly.
Only lighting raw bytes 5 and 7 differ as described above. The separately visited
picture selector also matched its complete saved before-image before returning
to Wave. A single archive does not cover every picture selector.

Final archive SHA-256:
`ca4cc4ca7d7f2b3b920bb488da57a91ad074784aeab12ce20a67d4a82dedc1e1`.
It also equals `after-hardware.json`, captured before the final three-key test.

## Remaining physical checks

Physical key output, macro playback, reactive effects, cable/reconnect behavior,
and power-cycle persistence need later hands-on coordination. Linux runtime and
the historical recovery/fault failures remain separate acceptance gates.
Native window interaction and the user's final rendered-layout review remain
open; offscreen rendering and successful controller execution do not replace them.

## User feedback and corrective build

The user reports that macro writes, keymap reassignment and lighting-mode changes
work. This is bounded user evidence for those operations; it does not establish
every playback mode, persistence, Linux behavior or fault recovery.

The reported UI problems were zero-based macro assignment labels, scrollbars
overlapping controls, technical connection diagnostics and a busy transaction
message after disconnect/re-read. The running release executable at inspection
was dated 2026-09-26 17:17:11 and preceded the source rewrite. The fixes below
apply to the current source, and the release executable was rebuilt in place.

- Assigned macro labels resolve local names through advertised binding metadata
  on both the Assignments and Macros pages. Unnamed slots use advertised one-based
  labels; long names retain full tooltips. Local labels load on the initial
  workspace, without requiring a visit to Macros.
- One shared vertical scroll container reserves a content gutter for the track
  and hit area, including macro events and host display/lighting controls.
- Connection errors refer to the keyboard. Save and host errors explain typed
  restoration outcomes in ordinary language instead of exposing enum/debug text.
- A replacement executor waits asynchronously for the previous worker to finish
  releasing its resources. Failed native opens release their OS lock; lock I/O
  errors no longer masquerade as another active transaction. No lock is forcibly
  released while an operation or restoration is still running.

All-feature/all-target workspace tests pass (89 core, 80 desktop, 228 devices,
seven CLI, external target and research tests), as does strict workspace Clippy.
Regression checks exercise real isolated OS locks, queued stale-command rejection,
failed reads/opens, repeated reconnect intent, retained drafts and close during
cleanup. Actual Iced offscreen renders and relevant click checks passed at
1360×800 and 1024×768; images remain under `target/visual-review/png-after`.
These checks performed no keyboard writes and did not physically disconnect it.
The older executable's exact lock failure is not conclusively attributed to the
current source race. A physical reconnect check of the rebuilt release remains
for the user.

## Approved workflow restoration

The user rejected the replacement executable's changed workflow. The source
rewrite did not authorize a UX redesign; its earlier rendered checks established
reachability, not equivalence to the approved UI. The rewritten views have now
been corrected against the supplied screenshots and interaction requirements:

- Keys, Macros, Lighting, Settings and Diagnostic capture remain the five tabs.
  The keyboard persists above the editor and beside the appropriate sidebar.
  Connection actions appear when needed, and assignment saves remain with Keys.
- Macros use the left editor and right Library/Playback arrangement. Name,
  recording/fixed wait, event count, manual editing and compact event actions
  retain their placement. Only configured, bound and selected candidate slots
  appear. Playback selection, repeat, assign, save/revert and collapsed file
  options retain their separate roles. Scrollbar gutters remain reserved.
- One Lighting dropdown selects onboard, per-key and host modes. Per-key mode
  paints the same keyboard with the compact native picker. Effect parameters
  sit beside the picker; there are no extra RGB channel sliders or separate
  per-key navigation tab. Onboard selection derives from the core draft.
- Settings again use compact labeled sliders/toggles and automatic application.
  The rewrite's numeric text-entry form and explicit apply toolbar are removed.

The disabled Record regression came from omitting the explicit New macro
initialization. After its foreground read confirms an empty slot, a stored count
outside editor limits is changed only in the local draft to the advertised
minimum. The original snapshot remains untouched. Ordinary reads, nonempty
programs and opaque slots are not normalized. The controller regression exercises
New macro → Record → Stop without a manual count edit or hardware writes.

These views are self-contained implementations over the current shared models,
projections and controllers. They have no build/runtime dependency on retired
application views. The named macro, scrollbar and reconnect fixes remain.

All-feature workspace tests pass (89 core, 83 desktop, 228 devices, seven CLI,
external target and research tests). Strict all-target/all-feature Clippy and
formatting pass. Offline Iced renders at 1360×800 and 1024×768 exercise a 42-event
macro, two named/bound macros, recording, lighting modes and settings. Mouse-event
checks exercise Record, event deletion beside the gutter, non-destructive gutter
clicks and selection from the lighting dropdown. A controller regression covers
explicit revert/read after a conflicting lighting or picture result. Images are under
`target/visual-review/approved-restored`. These are software/rendered checks;
the user's final native UI and physical reconnect/playback review remain open.

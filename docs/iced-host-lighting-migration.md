# Iced host lighting migration boundary

The egui baseline runs screen color and playback lighting through separate
threads (`src/screen_stream.rs` and `src/audio_stream.rs`). Those loops open a
`HostLightingSession`, send frames, and explicitly restore the saved effect.
That session now captures the selected Nia87 HID target and checks it on both
setup and restoration, including the legacy unique-device entry point.
The Iced desktop currently exposes only finite lighting read/apply commands.
The Nia87 adapter intentionally excludes host effects 20–22 from its editable
catalog. Iced must not present Start until its executor can own the complete
stream and restoration transaction.

## Required ownership change

1. Add a host activity to the portable session contract with explicit
   `Starting`, `Streaming`, `Stopping`, and `Unverified` outcomes. Start requires
   a verified lighting baseline and a backend-advertised host mode. A Stop
   request during Starting must be remembered and run restoration as soon as
   setup finishes. Every completion carries the connection generation and
   stream operation ID; stale UI completions cannot replace current state.
2. Extend the existing bounded device executor with a control path for Stop.
   The same worker that owns the selected device executes setup, frames, and
   restoration in order. It must accept Stop while the stream loop runs, and
   reject other device commands until restoration completes. Do not create a
   second worker or let a frame loop reopen an arbitrary unique device.
   The screen/audio sampler stays above `devices` and sends owned RGB or band
   frames through a bounded input slot. Band count is a backend capability;
   only the Nia87 adapter assumes 32 bands. A full slot may drop a frame; it must
   not delay Stop. The device-side host session only sends frames and restores
   its verified baseline. Do not make a device backend pull screen or audio
   samples in a `step` callback.
3. Route host activity through `Access::start_host_lighting` using the
   executor's immutable target. The retained stream's setup and restore are
   now target-bound, but the Iced executor does not yet own that stream and its
   Stop/recovery control path. Complete that ownership change before exposing
   the action.
4. Return a typed restoration result. A verified restored snapshot may refresh
   the lighting baseline. Failed or uncertain restoration leaves lighting
   unverified, keeps the backup path visible, and blocks another write until a
   deliberate read. A capture or stream failure after setup still runs restore.

## Desktop behavior

The host controls should be projected from backend capabilities and a verified
lighting baseline. Screen color and system playback are opt-in Starts. The
active view has Stop/Restore. A close request, focus loss, or selected-target
disconnect requests Stop and waits for restoration; it does not discard the
worker or accept a new connection while restoration is unresolved. Sampling is
local and no captured image or playback sample is stored. The existing
platform capture limits remain visible where a mode is offered.

## Acceptance gates

- Memory-executor tests: Start/Stop ordering; Stop during Starting; duplicate
  Stop; other commands rejected while streaming; stale generation and operation
  completions ignored; close waits for restoration; failed restoration remains
  unverified and retains the backup reference.
- Nia87 transport tests: selected-target change fails before opening any other
  collection, both on setup and on recovery; exact frame encodings remain the
  existing protocol fixtures. Tests use fakes and never drive the keyboard.
- Physical acceptance: with a backup and the selected USB collection, verify
  start, frames, Stop, close, focus loss, unplug/replug, and restoration
  readback. The earlier failed automatic recovery remains an open gate. Linux
  capture/runtime behavior needs separate acceptance.

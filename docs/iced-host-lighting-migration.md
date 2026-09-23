# Iced host lighting migration boundary

The egui baseline runs screen color and playback lighting through separate
threads (`src/screen_stream.rs` and `src/audio_stream.rs`). Those loops open a
`HostLightingSession`, send frames, and explicitly restore the saved effect.
That session now captures the selected Nia87 HID target and checks it on both
setup and restoration, including the legacy unique-device entry point.
Iced now exposes screen-average and playback music streaming as host modes
advertised separately from editable firmware effects. Its pure session owns
the baseline and operation ticket; the selected-device executor owns setup,
frames and restoration; OS samplers own capture and never open HID. This is an implementation
record, not physical acceptance of the Iced path.

## Required ownership change

1. The portable session contract has explicit
   `Starting`, `Streaming`, `Stopping`, and `Unverified` outcomes. Start requires
   a verified lighting baseline and a backend-advertised host mode. A Stop
   request during Starting must be remembered and run restoration as soon as
   setup finishes. Every completion carries the connection generation and
   stream operation ID; stale UI completions cannot replace current state.
2. The bounded device executor has a control path for Stop.
   The same worker that owns the selected device executes setup, frames, and
   restoration in order. It must accept Stop while the stream loop runs, and
   reject other device commands until restoration completes. Do not create a
   second worker or let a frame loop reopen an arbitrary unique device.
   The screen/audio sampler stays outside the keyboard backend and sends owned
   RGB or band frames through a bounded input slot. OS sampler adapters live in
   the devices package, separate from HID drivers and controlled by the desktop.
   Band count is a backend capability; the current desktop analyzer produces 32
   bands and the Nia87 adapter requires exactly 32. A full slot may drop a
   frame; it must not delay Stop. The device-side host session only sends frames and restores
   its verified baseline. Do not make a device backend pull screen or audio
   samples in a `step` callback.
3. The Nia87 adapter routes host activity through
   `Access::start_host_lighting_detailed` using the executor's immutable target.
   The adapter maps the portable screen mode to Nia87 effect 21 and the two
   music modes to effects 22 and 20. The core owns a transient host-mode draft
   using the shared lighting parameter schema and control projection. Music
   defaults to value 4, green RGB and upright; brightness 0–4, three options,
   fixed RGB and rainbow are editable before Start. The validated setting
   travels through the portable executor contract; the Nia87 adapter verifies
   the exact advertised mode and setting before any device access.
   The same worker owns Stop and recovery; it never opens an arbitrary matching
   collection. Each activity retains its selected host source, and the shared
   frame contract checks RGB versus audio-band input and the advertised band
   count before the Nia87 session sends a frame. An incompatible frame fails
   the stream and enters the normal restoration path.
4. Restoration returns a typed result. A verified restored snapshot may refresh
   the lighting baseline. Failed or uncertain restoration leaves lighting
   unverified, keeps the backup path visible, and blocks another write until a
   deliberate read. A capture or stream failure after setup still runs restore.

## Desktop behavior

Iced offers screen and playback Start from backend capabilities only with a
verified, editable, clean lighting baseline. The active view has Stop & restore.
A mode selection resets its temporary parameters to that mode's defaults;
editing them does not stage a persistent device change. The view renders the
same projected parameter controls used for ordinary lighting settings.
A close request or window focus loss asks for Stop and waits for a verified
restoration result. The stream sampler is local, drops frames if its slot is
full, and stores no captured image
or audio sample.
Focus-loss Stop and selected-target disconnect acceptance remain open; the
music sampler uses WASAPI loopback on Windows and the default output monitor
through PulseAudio on Linux. Platform capture limits remain to be validated on
Linux, including X11 disconnect, Wayland and audio routing.

## Acceptance gates

- Headless tests cover Start/Stop ordering, Stop during Starting, duplicate
  Stop, finite commands rejected while streaming, generation-change restore,
  bounded frames, and failed restoration. Pure session tests reject stale
  tickets and invalidate lighting on uncertain restore. The Iced close path
  waits for restoration, but needs GUI interaction acceptance.
- Nia87 transport tests: selected-target change fails before opening any other
  collection, both on setup and on recovery; exact frame encodings remain the
  existing protocol fixtures. Pure adapter tests cover the music brightness
  bounds, each option and fixed/rainbow report encodings, plus rejection of a
  forged mode or invalid setting before I/O. Tests use fakes and never drive
  the keyboard. Shared frame tests cover both source types, mismatched frame
  types and the 32-band length requirement.
- Physical acceptance: with a backup and the selected USB collection, verify
  start, frames, Stop, close, focus loss, unplug/replug, and restoration
  readback. The earlier failed automatic recovery remains an open gate. Linux
  capture/runtime behavior needs separate acceptance.

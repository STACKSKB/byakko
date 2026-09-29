# Browser/native parity

The target is the current native Iced configurator on the same branch. Existing
native hardware acceptance gaps remain gaps. The implementation is shared where
behavior is portable; only browser effects and presentation live in `web/`.

| Area | Implemented behavior | Evidence / remaining acceptance |
| --- | --- | --- |
| Keymap | Base/Fn edits, protected positions, macro assignment, shared validation, before-image, paced writes, one readback and recovery | Limited physical remap and restoration; simulated controls and exact-byte tests |
| Onboard RGB / picture | All advertised effects/options, semantic white, layer-dependent picture cache, autosave and per-key painting | Limited physical RGB/per-key changes and restoration; setters retain transport-acceptance evidence |
| Settings | Advertised toggles, debounce and sleep fields; one pending field; backup, 500 ms pacing, one four-reply readback and recovery | Simulated autosave control and Rust effect tests; physical settings writes remain open |
| Onboard changes | Selected notification interface, shared decoding and observation queue, 500 ms / two-second coalescing, affected loaded features, queued edits/recording/host work, retained conflicts | Limited physical notification evidence, WASM replay and simulated control tests |
| Macros | Passive catalog, bound/configured library, local names, fixed/measured recording, local pointer input, playback choices, native macro files, save-and-assign | Limited physical macro save-and-assign and restoration; storage, input, session and control tests |
| Host screen/music | Shared lifecycle, screen average/point, two music modes, browser capture, shared audio projection, bounded latest frame, live parameters, original-baseline restoration | Synthetic browser controls plus startup/frame/stop/failure/race tests; real screen/audio capture and physical host output remain unaccepted |
| Diagnostics | Full lossless native-format capture/export, retained offline export, durable IndexedDB before-images | Exact-format tests and simulated export after disconnect; no public archive restore/import |
| Native delivery | Same workspace/branch; shared pure settings/archive/host codecs, audio projection and observation scheduler; native effects unchanged | Native build, workspace tests, formatting and strict Clippy pass; no native JavaScript/runtime/server dependency |

The CLI's developer-only picture snapshot staging and raw recovery tools remain
native developer tools. Neither native desktop nor browser exposes public raw
archive restoration. This is desktop feature parity, not a browser copy of every
research command.

## Browser limits and open acceptance

- WebHID needs a capable desktop browser, secure context and user-selected HID
  permission. The broad VID/PID chooser returns the keyboard's interfaces; exact
  configuration and notification report validation still follows selection.
- Screen/shared-audio capture requires a fresh user gesture/permission each time.
  Shared audio depends on browser, OS and selected source. There is no microphone
  substitute. See [getDisplayMedia](https://developer.mozilla.org/en-US/docs/Web/API/MediaDevices/getDisplayMedia).
- Focus loss does not deliberately stop host lighting. Browser freezing/discard
  can suspend timers or terminate execution. Stop and restore before closing;
  a force-closed page cannot guarantee asynchronous HID restoration. See
  [Chrome page lifecycle](https://developer.chrome.com/docs/web-platform/page-lifecycle-api).
- Browser recording sees focused page input rather than a global OS hook; reserved
  browser/OS shortcuts may not reach the page. Files use browser file selection
  and downloads; backups/names are origin-local IndexedDB, not native storage.
- Standalone Chrome/Edge, Linux, PID 4011, unplug/reconnect, prolonged capture,
  physical settings/host output, power-cycle persistence and hardware fault
  recovery remain unverified. Synthetic fault tests do not establish those.

The physical checks above were limited to the listed operations. Other behavior
is supported by software tests until separate hardware acceptance is completed.

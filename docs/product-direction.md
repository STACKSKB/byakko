# Product direction

The stretch goal is the useful scope of an older native peripheral suite:
simple everyday interaction, powerful configuration, reliable plug-and-play
behavior and low resource use. This does not expand the current task into
flashing replacement firmware. The first supported device remains the stock
Menel Nia87 over USB.

## Required experience

- Launch the native workbench and discover supported devices automatically.
  Load their physical layout and capabilities without repeated model setup.
- Configure devices without a browser, web authorization flow, Electron,
  JavaScript runtime, webview or vendor helper. OS-level device permissions may
  still require platform installation setup; do not disguise that limitation.
- Stored device settings work with the GUI closed. Host-dependent features
  have explicit activation, visible status and a controlled stop/restoration path.
- Reconnect automatically and handle multiple devices explicitly, without
  guessing which keyboard should receive a write.
- Keep common operations direct and advanced controls available. Use the
  original dense native workbench design, not vendor or Sharkfin UI copies.
- Preserve staged work across recoverable errors and report exactly whether
  device writes and restoration were verified.
- Reuse the frontend across independent backend adapters. Nia87 firmware
  packets, storage limits and layer conventions must not define shared UI models.

## Acceptance beyond feature counts

Measure release-build launch time, idle working set, idle CPU, discovery and
reconnect latency, and resource use while host lighting is active. Record the
platform and workload so regressions are comparable. Do not claim low memory
use from the language choice or development-build behavior alone.

Exercise clean-machine installation, no-device startup, hotplug, reconnect,
multiple devices, unsupported capabilities, interrupted writes and recovery.
Physical output, timing and power-cycle persistence need hardware validation.
Backend-neutral UI tests must include a different layout and more than two
layers. Future QMK/VIA adapters should not need to emulate Nia87's raw format.

See [backend architecture](backend-architecture.md) for the migration plan and
[acceptance ledger](parity-status.md) for the current evidence and gaps.

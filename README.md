# Byakko

A native, USB-first Menel Nia87 configurator under investigation. The intended application uses Rust and a native GUI, direct HID access, and the keyboard's stock firmware. No JavaScript, Electron, webview, or vendor helper dependency.

## Current status

Only the read-only HID enumeration command is implemented. There is no configurator GUI or configuration read/write support yet. See [PLAN.md](PLAN.md) for milestones, plug-and-play layout requirements, evidence and licensing boundaries.

```console
cargo run --locked -- devices
```

This lists collections matching candidate VID/PIDs from the supplied vendor package. It does not open an input stream, capture typing, send feature reports, change settings or flash firmware. A candidate match does not establish the model uniquely.

Validated on the attached Windows keyboard: `3151:4015`, configuration collection on interface 2, usage `FFFF:0002`. The Windows build and Clippy checks pass. Linux and macOS execution have not been tested; Linux hidraw permissions will need configuration for later device access.

The target product will include a verified Nia87 TKL profile so no repeated layout setup is necessary. 2.4 GHz configuration is deferred until USB works and receiver capability is measured.

## Research and provenance

The probe is original code using the MIT-licensed `hidapi` Rust binding. Windows uses its native backend; Linux selects `linux-native-basic-udev` rather than linking libudev. The macOS backend uses the upstream C HIDAPI implementation, for which the BSD-style license option is intended. Target-specific dependency and distribution notice audits remain necessary before releases.

No Sharkfin source, tables, tests or assets have been incorporated. User-reported Sharkfin compatibility is behavioral evidence only. Vendor installers and extracted research material are ignored by Git and are not product assets.

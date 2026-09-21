# Byakko

A native, USB-first Menel Nia87 configurator under investigation. The intended application uses Rust and a native GUI, direct HID access, and the keyboard's stock firmware. No JavaScript, Electron, webview, or vendor helper dependency.

## Current status

A native keymap workbench is implemented, with automatic Nia87 TKL layout, base/Fn editing, staged changes, automatic backups and complete readback verification. Media, pointer and modifier combinations are available. The native macro editor supports event editing, saved-slot bindings and JSON import/export. Long macro storage, shorter replacement and empty restoration passed live readback checks. Lighting reads work; lighting controls remain in progress. This is a development build, not yet feature complete. See [docs/live-evidence.md](docs/live-evidence.md) for the exact verification limits.

```console
cargo run --locked -- devices
cargo run --locked -- gui
```

The `devices` command only enumerates. `descriptor`, `inspect`, and `export <new-file.json>` inspect the configuration interface and keymaps. The GUI reads automatically, stages edits locally, and writes only on explicit Apply. A candidate USB match alone does not establish the model uniquely. Close the official configurator/helper before connecting to avoid competing transactions.

Validated on the attached Windows keyboard: `3151:4015`, configuration collection on interface 2, usage `FFFF:0002`, raw firmware version `0100`. A Pause-to-F24 remap and an unbound macro storage test were both read back and restored. Windows builds, protocol tests and Clippy pass. Physical output and power-cycle persistence remain untested while the user is AFK. Linux and macOS execution have not been tested; Linux hidraw access needs suitable permissions.

The built-in Nia87 slot profile comes from our observed default map, independently checked against the supplied vendor fixture. It does not depend on current key assignments or require repeated layout setup. 2.4 GHz configuration is deferred until USB works and receiver capability is measured.

## Research and provenance

The probe is original code using the MIT-licensed `hidapi` Rust binding. Windows uses its native backend; Linux selects `linux-native-basic-udev` rather than linking libudev. The macOS backend uses the upstream C HIDAPI implementation, for which the BSD-style license option is intended. Target-specific dependency and distribution notice audits remain necessary before releases.

No Sharkfin source, tables, tests or assets have been incorporated. User-reported Sharkfin compatibility is behavioral evidence only. Vendor installers and extracted research material are ignored by Git and are not product assets.

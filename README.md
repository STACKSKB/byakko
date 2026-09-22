# Byakko

A native, USB-first Menel Nia87 configurator under investigation. The intended application uses Rust and a native GUI, direct HID access, and the keyboard's stock firmware. No JavaScript, Electron, webview, or vendor helper dependency.

## Current status

A native keymap workbench is implemented, with automatic Nia87 TKL layout, base and Fn editing, staged changes, automatic backups and complete readback verification. Media, pointer and modifier combinations are available. The native macro editor supports event editing, saved-slot bindings and JSON import/export. Long macro storage, shorter replacement and empty restoration passed live readback checks. Global effect and per-key color panels are implemented; brightness and one per-key color passed reversible readback tests. Fn F24 and media writes now pass application readback and restoration. Mixed base/Fn changes now pass complete verification with one-second spacing between key writes. This is a development build, not yet feature complete. See [docs/live-evidence.md](docs/live-evidence.md) for the exact verification limits.

```console
cargo run --locked -- devices
cargo run --locked -- gui
```

The `devices` command only enumerates. `descriptor`, `inspect`, and `export <new-file.json>` inspect the configuration interface and keymaps. The GUI reads automatically, stages edits locally, and writes only on explicit Apply. A candidate USB match alone does not establish the model uniquely. Close the official configurator/helper before connecting to avoid competing transactions.

Validated on the attached Windows keyboard: `3151:4015`, configuration collection on interface 2, usage `FFFF:0002`, raw firmware version `0100`. A Pause-to-F24 remap and an unbound macro storage test were both read back and restored. Windows builds, protocol tests and Clippy pass. Physical output and power-cycle persistence remain untested while the user is AFK. Linux core and GUI cross-target checks pass, but Linux linking/execution and hardware remain untested; hidraw access needs suitable permissions. The original adapters currently support Windows and Linux; macOS is not implemented.

The built-in Nia87 slot profile comes from our observed default map, independently checked against the supplied vendor fixture. It does not depend on current key assignments or require repeated layout setup. 2.4 GHz configuration is deferred until USB works and receiver capability is measured.

The workbench has keyboard shortcuts for its five pages (Ctrl+1–5), local keymap import/export, and a focused keyboard macro recorder. Debounce and automatic OS detection have native controls and passed reversible device readback tests. Four wireless sleep timers have native controls and passed complete write/readback/restoration tests. Keyboard options remain read-only. A captured official Fn media binding now passes native write/readback/restoration. USB access recovered after a replug; the pending test binding was restored, and Fn editing is enabled. Mixed edits to both layers of the same slot are now supported and verified by readback/restoration.

## Research and provenance

Complete device archives can now be captured and inspected from the native Keys page or with `capture-configuration NEW_PATH.json` and `inspect-configuration PATH.json`. Capture includes all 50 macros, both keymaps, current picture, lighting and settings, with two matching complete reads. Whole-archive restoration remains pending. See [archive format and verification](docs/configuration-archives.md) and the [current acceptance ledger](docs/parity-status.md) for remaining work.

The product uses original Windows HID and Linux hidraw adapters with permissively licensed OS bindings. HIDAPI was removed after an audit found a GPL header in its build script despite MIT package metadata. Target-specific dependency and distribution notice audits remain necessary before releases.

No Sharkfin source, tables, tests or assets have been incorporated. User-reported Sharkfin compatibility is behavioral evidence only. Vendor installers and extracted research material are ignored by Git and are not product assets.

# Byakko

A native, USB-first Menel Nia87 configurator under investigation. The intended application uses Rust and a native GUI, direct HID access, and the keyboard's stock firmware. No JavaScript, Electron, webview, or vendor helper dependency.

## Current status

A native keymap workbench is implemented, with automatic Nia87 TKL layout, base and Fn editing, staged changes, automatic backups and complete readback verification. Media, pointer and modifier combinations are available. The native macro editor supports event editing, saved-slot bindings and JSON import/export. Long macro storage, shorter replacement and empty restoration passed live readback checks. Global effect and per-key color panels are implemented; brightness and one per-key color passed reversible readback tests. Fn F24 and media writes now pass application readback and restoration. Mixed base/Fn changes now pass complete verification with one-second spacing between key writes. This is a development build, not yet feature complete. See [docs/live-evidence.md](docs/live-evidence.md) for the exact verification limits.

```console
cargo run --locked -- devices
cargo run --locked -- gui
```

The `devices` command only enumerates. `descriptor`, `inspect`, and `export <new-file.json>` inspect the configuration interface and keymaps. The GUI reads automatically, stages edits locally, and writes only on explicit Apply. A candidate USB match alone does not establish the model uniquely. Close the official configurator/helper before connecting to avoid competing transactions.

Validated on the attached Windows keyboard: `3151:4015`, configuration collection on interface 2, usage `FFFF:0002`, raw firmware version `0100`. A Pause-to-F24 remap and an unbound macro storage test were both read back and restored. Windows builds, protocol tests and Clippy pass. Physical output and power-cycle persistence remain untested while the user is AFK. Linux core and GUI cross-target linking checks pass; native Linux runtime behavior, hidraw permissions, and hardware transactions remain untested. The original adapters currently support Windows and Linux; macOS is not implemented.

The built-in Nia87 slot profile comes from our observed default map, independently checked against the supplied vendor fixture. It does not depend on current key assignments or require repeated layout setup. 2.4 GHz configuration is deferred until USB works and receiver capability is measured.

The workbench has keyboard shortcuts for its five pages (Ctrl+1–5), local keymap import/export, and a focused keyboard macro recorder. The shared Keys editor now uses a backend boundary with dynamic layouts, layers, typed actions, and lossless opaque bindings; macros, lighting, picture, settings, and archive panels remain Nia87-specific. Debounce and automatic OS detection have native controls and passed reversible device readback tests. Four wireless sleep timers have native controls and passed complete write/readback/restoration tests. The backlight option has a native control; other option semantics remain limited to the verified subset. A captured official Fn media binding now passes native write/readback/restoration. USB access recovered after a replug; the pending test binding was restored, and Fn editing is enabled. Mixed edits to both layers of the same slot are now supported and verified by readback/restoration.

## Research and provenance

Complete device archives can be captured, inspected, reviewed and applied from the native Keys page. CLI capture and inspection use `capture-configuration NEW_PATH.json` and `inspect-configuration PATH.json`. Archives include all 50 macros, both keymaps, current picture, lighting and settings. Multi-section application and restoration passed complete hardware readback. The first injected-failure run failed automatic recovery after a lighting write; a later explicit restore returned the original archive and verified it; unexplained state changes remain unresolved, so repeat failure-injection acceptance is pending. See [archive format and verification](docs/configuration-archives.md) and the [current acceptance ledger](docs/parity-status.md) for remaining work.

The product uses original Windows HID and Linux hidraw adapters with permissively licensed OS bindings. HIDAPI was removed after an audit found a GPL header in its build script despite MIT package metadata. Target-specific dependency and distribution notice audits remain necessary before releases.

No Sharkfin source, tables, tests or assets have been incorporated. User-reported Sharkfin compatibility is behavioral evidence only. Vendor installers and extracted research material are ignored by Git and are not product assets.

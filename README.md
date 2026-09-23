# Byakko

A native, USB-first Menel Nia87 configurator under investigation. The intended application uses Rust and a native GUI, direct HID access, and the keyboard's stock firmware. No JavaScript, Electron, webview, or vendor helper dependency.

## Current status

The approved architecture migration uses **Iced**. Its keymap, macro, global lighting, per-key picture, scalar settings and native archive capture/review slices are
available separately from the retained research GUI:

```console
cargo run -p byakko-desktop --release --locked
cargo run -p byakko-desktop --release --locked -- --demo
```

Normal startup discovers and reads the Nia87 automatically. Idle USB scans
notice removal and retry reads after reconnection without choosing between
multiple matching interfaces; scans send no HID reports. Each accepted
connection gets an executor bound to the selected HID collection, and later
opens reject a different collection. The demo uses an
in-memory, three-layer keyboard and performs no hardware or file writes. Both use the
same portable session model and serialized executor. The Iced screen supports
key/layer selection, action search, staged-change review, apply with readback,
revert and explicit read/reconnect. The Macros page supports capability-driven
slot reads, event editing/reordering, stored repeat counts, revert and verified
save. Saved macros can be staged onto the selected key/layer using advertised
playback modes; Nia87 toggle/hold require an explicitly saved count of 1.
Focused keyboard/mouse recording appends to the draft with measured or fixed
waits and reserved release events. The demo includes editable and opaque macro
slots. The Macro file panel imports into the current draft and exports a new
JSON file; native version-1 files remain importable. Lighting offers the
backend's built-in effects, option choices, brightness, speed and color when
supported; edits use the same staged and verified save workflow. Per-key RGB
picture storage has a separate capability-driven editor and staged device
workflow; storing RGB values is separate from selecting the built-in global
picture effect. This slice has only read-only USB baseline verification, with
no live picture writes. Settings offer capability-defined toggles and bounded
values. The Iced editor stages one setting at a time to match the Nia87's
guarded one-setting transaction; its attached-device check is read-only.
The local configurations page captures, exports a new native backup and
reviews a chosen archive against a fresh device capture. A changed review can
be applied with a durable backup, complete readback and typed recovery result;
this Iced path has not had a live archive write. Iced screen-average lighting
now streams through the same selected-device executor with a bounded frame
queue and verified restore on Stop or close. Iced playback music uses the same
path with separate OS audio sampling and capability-driven temporary controls
for brightness, direction and fixed RGB/rainbow color.
These Iced lifecycles have headless tests but have not had a live stream or
music parameter editing. The following feature inventory and
`gui` command describe the retained egui research application. See the
[approved architecture](docs/pre-alpha-proposal.md) and
[current boundary](docs/backend-architecture.md). The
[lighting acceptance notes](docs/iced-lighting-acceptance.md) distinguish
headless verification from writes on the attached keyboard. The
[per-key picture acceptance notes](docs/iced-picture-acceptance.md) record its
storage model and physical verification limits. The
[settings acceptance notes](docs/iced-settings-acceptance.md) record the
one-setting transaction and read-only comparison. The
[archive acceptance notes](docs/iced-archive-acceptance.md) record its
read-only capture and review limits. The
[discovery acceptance notes](docs/iced-discovery-acceptance.md) record automatic
reconnect behavior and limits. The
[desktop UI boundary](docs/desktop-ui-architecture.md) describes the shared
style data and responsive panels.

A native keymap workbench is implemented, with automatic Nia87 TKL layout, base and Fn editing, staged changes, automatic backups and complete readback verification. Media, pointer and modifier combinations are available. The native macro editor supports event editing, saved-slot bindings and JSON import/export. Long macro storage, shorter replacement and empty restoration passed live readback checks. Global effect and per-key color panels are implemented; brightness and one per-key color passed reversible readback tests. Fn F24 and media writes now pass application readback and restoration. Mixed base/Fn changes now pass complete verification with one-second spacing between key writes. This is a development build, not yet feature complete. See [docs/live-evidence.md](docs/live-evidence.md) for the exact verification limits.

```console
cargo run --locked -- devices
cargo run --locked -- gui
```

GUI backups and default exports use a stable per-user directory; see
[local file locations](docs/local-storage.md). Existing files are retained.

The `devices` command only enumerates. `descriptor`, `inspect`, and `export <new-file.json>` inspect the configuration interface and keymaps. The GUI reads automatically, stages edits locally, and writes only on explicit Apply. A candidate USB match alone does not establish the model uniquely. Close the official configurator/helper before connecting to avoid competing transactions.

Validated on the attached Windows keyboard: `3151:4015`, configuration collection on interface 2, usage `FFFF:0002`, raw firmware version `0100`. A Pause-to-F24 remap and an unbound macro storage test were both read back and restored. Windows builds, protocol tests and Clippy pass. Physical output and power-cycle persistence remain untested while the user is AFK. The Iced desktop and narrow hidraw permission helper cross-link as Linux ELF binaries; native Linux runtime behavior, hidraw permissions, and hardware transactions remain untested. The original adapters currently support Windows and Linux; macOS is not implemented.

The built-in Nia87 slot profile comes from our observed default map, independently checked against the supplied vendor fixture. It does not depend on current key assignments or require repeated layout setup. 2.4 GHz configuration is deferred until USB works and receiver capability is measured.

The workbench has keyboard shortcuts for its five pages (Ctrl+1–5), local keymap import/export, and a [focused keyboard/mouse-button macro recorder](docs/macro-recording.md). The Iced pages use backend capabilities and portable state; the Nia87 report details remain in its device adapter. Debounce and automatic OS detection have native controls and passed reversible device readback tests. Four wireless sleep timers have native controls and passed complete write/readback/restoration tests. The backlight option has a native control; other option semantics remain limited to the verified subset. A captured official Fn media binding now passes native write/readback/restoration. USB access recovered after a replug; the pending test binding was restored, and Fn editing is enabled. Mixed edits to both layers of the same slot are now supported and verified by readback/restoration.

Linux desktop permission setup is documented in [Linux installation](docs/linux-install.md). The native helper and narrow udev rule are included; their real Linux device behavior remains unverified.

## Research and provenance

Complete device archives can be captured, inspected, reviewed and applied from the native Keys page. CLI capture and inspection use `capture-configuration NEW_PATH.json` and `inspect-configuration PATH.json`. Archives include all 50 macros, both keymaps, current picture, lighting and settings. Multi-section application and restoration passed complete hardware readback. The first injected-failure run failed automatic recovery after a lighting write; a later explicit restore returned the original archive and verified it; unexplained state changes remain unresolved, so repeat failure-injection acceptance is pending. See [archive format and verification](docs/configuration-archives.md) and the [current acceptance ledger](docs/parity-status.md) for remaining work.

The product uses original Windows HID and Linux hidraw adapters with permissively licensed OS bindings. HIDAPI was removed after an audit found a GPL header in its build script despite MIT package metadata. Target-specific dependency and distribution notice audits remain necessary before releases.

No Sharkfin source, tables, tests or assets have been incorporated. User-reported Sharkfin compatibility is behavioral evidence only. Vendor installers and extracted research material are ignored by Git and are not product assets.

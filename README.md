# Byakko

A native, USB-first Menel Nia87 configurator using Rust and Iced, direct HID
access and stock firmware. No JavaScript, Electron, webview or vendor helper
is needed by the product.

This is a **source-only pre-alpha** for Windows and Linux. It is not feature
complete. Read the [support and recovery notes](docs/pre-alpha-support.md) for
current behavior, physical acceptance and known limits.

## Build and run

Use the [source-build instructions](docs/source-build.md) for prerequisites,
Windows/Linux commands and local checks. The tested toolchain is Rust 1.98.0.

```sh
cargo build --release --locked -p byakko-desktop -p byakko-cli
cargo run --release --locked -p byakko-desktop -- --demo
```

Omit `--demo` to discover and read the connected keyboard. The demo uses an
in-memory keyboard. Linux hardware access needs the narrow permission setup in
[Linux installation](docs/linux-install.md); run as your ordinary desktop user.

The keyboard stays visible while key, macro, lighting and settings controls
change around it. Keys/macros use staged saves; lighting and settings send
queued choices automatically. Color drags commit after release and the idle
delay. Ordinary lighting/picture uploads report transport acceptance; they do
not claim an immediate verified readback. See [autosave behavior](docs/desktop-configuration.md).

The independent CLI uses the same session/executor contract:

```sh
cargo run --release --locked -p byakko-cli -- --help
cargo run --release --locked -p byakko-cli -- devices
cargo run --release --locked -p byakko-cli -- read
```

The first two commands are discovery/help; `read` sends getters and prints a
keymap snapshot. Apply/restore commands write device state. Start with the
[read-only Linux sequence](docs/linux-handoff.md) before any Linux write test.
Backups use the [normal-user data directory](docs/local-storage.md).

The Iced Diagnostic capture page provides native archive capture and export
for diagnostics. Full archive import/review/restore is deferred from the
pre-alpha UI because a recorded restore mismatch remains unresolved. Automatic
before-image backups for individual feature writes remain in place; see the
[support and recovery notes](docs/pre-alpha-support.md).

## Status and development

- [Current acceptance ledger](docs/parity-status.md) and [pre-alpha checklist](docs/public-pre-alpha-checklist.md)
- [Application architecture](docs/pre-alpha-proposal.md) and [frontend contract](docs/frontend-contract.md)
- [Dependency/source inventory](docs/dependency-source-audit.md)
- [Performance observations and limits](docs/performance-baseline.md)

The retained root `byakko gui` command is an egui research baseline, not the
Iced product. Research captures, vendor installers and extracted material are
not product assets and are not required to build. Original codecs and preserved
fixtures live alongside dated observations; no Sharkfin source, tables, tests
or assets have been incorporated. The renderer's local patch is included in Git.

USB Nia87 comes first. QMK/VIA, 2.4 GHz, browser delivery and unrelated visual
redesign remain deferred. Firmware flashing, vendor accounts and cloud sharing
are outside the stock-firmware configurator scope. Development uses local
build/test commands; this pre-alpha has no CI/CD requirement.

## License

Byakko's original code, documentation and project-owned assets are licensed
under the GNU General Public License, version 3 or (at your option) any later
version (`GPL-3.0-or-later`). See [LICENSE](LICENSE). Byakko is distributed
without any warranty; see the license for details.

Third-party code and assets retain their own notices and license terms. In
particular, the vendored `iced_tiny_skia` renderer retains its
[MIT license](vendor/iced_tiny_skia/LICENSE) and
[patch provenance](vendor/iced_tiny_skia/BYAKKO-PATCH.md). Files identifying a
separate third-party license are not relicensed by this project notice.

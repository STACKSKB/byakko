# Byakko

A native, USB-first Menel Nia87 configurator using Rust and Iced, direct HID
access and stock firmware. No JavaScript, Electron, webview or vendor helper
is needed by the product.

This branch is an **application rewrite in progress**, starting from `3db624f`.
The old application controllers have been removed. The replacement currently
supports keymap read, staged assignment, save, revert and manual reconnect through
Iced and an independent CLI. Macro discovery, selected-slot editing, save-and-assign
and CLI snapshot workflows also use the replacement core. Window-local macro
recording appends to the shared draft and releases held inputs on stop, focus loss
or close. Lighting, per-key painting and scalar settings use the same editors,
with automatic coalesced saves and native color pickers. Other frontend workflows are being rebuilt; this is
not a feature-complete release. See [plan.md](plan.md) for current scope and
[rewrite constraints](docs/rewrite-constraints.md) for the preserved requirements.

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

The keymap view uses device-supplied physical geometry and action choices. Edits
stay in the core editor until Save assignments. Save uses the cached before-image
and the existing native backup/write/readback transaction. Closing waits for a
pending save and asks before discarding edits. Automatic discovery/reconnect,
the richer assignment catalog, custom shortcut form, host lighting and diagnostic
capture are pending frontend milestones. Lighting/picture saves report transport
acceptance after established pacing; settings saves include one readback.

The independent CLI uses the same session/executor contract:

```sh
cargo run --release --locked -p byakko-cli -- --help
cargo run --release --locked -p byakko-cli -- devices
cargo run --release --locked -p byakko-cli -- read
cargo run --release --locked -p byakko-cli -- --demo read
```

The first two commands are discovery/help; `read` sends getters and prints a
keymap snapshot. `plan-keymap FILE` validates edits from a complete snapshot;
`apply-keymap FILE` writes with a backup and verifies the result. `--demo` uses
memory only. `list-macros` reads the macro library; `read-macro SLOT` emits a
backend snapshot. Retain its revision when editing for `plan-macro FILE` or
`apply-macro FILE`. `assign-macro SLOT LAYER KEY BINDING` uses the shared core
assignment workflow. `read-lighting`, `read-picture` and `read-settings` emit
snapshots for their corresponding `plan-FEATURE FILE` / `apply-FEATURE FILE`
commands. Retain the revision and selector context; a settings file changes one
scalar field. Other previous CLI commands are not exposed on this checkpoint.
Start with the
[read-only Linux sequence](docs/linux-handoff.md) before any Linux write test.
Backups use the [normal-user data directory](docs/local-storage.md).

Native device APIs, research tools, codecs and fixtures are retained. Public
archive capture/export will return in its planned milestone; archive restore
remains a developer operation. Existing recovery failures are not fixed by this
rewrite. The [support and recovery notes](docs/pre-alpha-support.md) describe
the previous executable and physical evidence, not acceptance of this rewrite.

## Status and development

Application code is organized by responsibility. Core's `editor/` contains one
shared lifecycle and feature implementations; `model/`, `validation/`,
`library/`, `recorder/`, `projection/` and `workflow/` contain the corresponding
feature files. Desktop separates input forms, views and shared widgets. Native
board and protocol-family modules retain the hardware-specific implementation.

- [Current rewrite architecture and sequence](plan.md)
- [Engineering rules](AGENTS.md) and [constraint inventory](docs/rewrite-constraints.md)
- [Previous implementation acceptance ledger](docs/parity-status.md) and [pre-alpha checklist](docs/public-pre-alpha-checklist.md)
- [Previous architecture](docs/pre-alpha-proposal.md) and [previous frontend contract](docs/frontend-contract.md)
- [Dependency/source inventory](docs/dependency-source-audit.md)
- [Performance observations and limits](docs/performance-baseline.md)

The egui application has been retired. Research captures, vendor installers,
and extracted material are not product assets and are not required to build.
Original codecs and preserved fixtures live alongside dated observations; no Sharkfin source, tables, tests
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

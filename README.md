# Byakko — PRE-ALPHA

A native Windows, macOS and Linux configurator for the **Menel Nia87 over USB**,
built with Rust and Iced. It uses the keyboard's stock firmware and direct HID
access; no vendor software, browser, or background service is needed.

**This is experimental pre-alpha software.** Hardware validation is still in
progress, and recovery from every failed write is not guaranteed. The current
application is on [`master`](https://github.com/STACKSKB/byakko/tree/master).
Download portable builds from the [Releases page](https://github.com/STACKSKB/byakko/releases).

## What it does

- Edit base and Fn key assignments, including shortcuts and macro bindings.
- Record and edit macros, name them locally, and import/export portable macro files.
- Choose onboard lighting effects or edit per-key RGB in layers 1, 2 and 3
  (Fn+Z, Fn+X and Fn+C).
- Run screen-following lighting and, on supported platforms, music-following
  lighting with live music controls.
- Edit supported keyboard settings and export diagnostic captures.
- Refresh affected features when the keyboard sends onboard-change events,
  while retaining unfinished edits.

The independent CLI supports discovery, reads, snapshot planning/apply and
archive comparison through the same application core. Use `byakko-cli --help`
for its commands.

## Getting started

See [Build from source](docs/source-build.md) for platform prerequisites,
commands and output locations. To try the interface without a keyboard:

```sh
cargo run --release --locked -p byakko-desktop -- --demo
```

Omit `--demo` to use a connected Nia87. Connect by USB and close other keyboard
configurators before starting.

On macOS, screen-following requires Screen Recording access, and music-following
is not available yet.

**Linux requires the Byakko udev rule and permission helper.** Follow
[Linux installation](docs/linux-install.md) to install
`packaging/linux/70-byakko-nia87.rules` and `byakko-hidraw-access`, reload udev
rules, and reconnect the keyboard. Run Byakko as your normal desktop user, not
with `sudo`. The rule grants access to the configuration interface; simply
building the executable does not grant device permissions.

Key assignments and macros have explicit save controls. Lighting and settings
changes save automatically after a short delay. Per-key RGB starts by selecting
and loading a layer; apply pending colors before switching layers. Before-image
backups and local macro names use the [normal-user data directory](docs/local-storage.md).

## Pre-alpha limits

- Only stock-firmware Nia87 USB configuration is supported. Wireless receivers,
  QMK/VIA, other keyboards and firmware flashing are outside this version.
- Fn system keys and Fn+Esc (factory reset) are protected from reassignment.
- Diagnostic archives can be captured and exported in the UI; restoring a full
  archive is not a public feature. Historical recovery failures remain unresolved.
- Physical layer 2/3 RGB persistence, some macro playback cases, host-lighting
  restoration and Linux hardware/runtime behavior still need acceptance checks.
  Passing software tests does not establish those results.
- Linux permissions for native onboard-change notifications still need validation; manual reads remain available.
- Linux screen-following needs X11; Wayland screen capture is unsupported.
  Music-following needs PulseAudio or a compatible PipeWire PulseAudio service.
- The interface is currently English. Translations and wider hardware support
  remain future work.

Current checks and their boundaries are recorded in the
[rewrite acceptance notes](docs/rewrite-acceptance-20260927.md).
When reporting a problem, include the source revision or package name, OS,
keyboard connection, steps to reproduce, and the complete error message.

## Development

The application separates portable domain/editor logic (`byakko-core`), native
device and OS effects (`byakko-devices`), Iced forms/views (`byakko-desktop`), and
CLI parsing/output (`byakko-cli`). Editors share one baseline/draft lifecycle;
feature-specific rules stay with their implementations.

- [Current plan and ownership map](plan.md)
- [Engineering rules](AGENTS.md) and [rewrite constraints](docs/rewrite-constraints.md)
- [Build and local verification](docs/source-build.md)
- [Dependency/source inventory](docs/dependency-source-audit.md)

Older architecture and acceptance documents are historical evidence, not the
current application design. Research captures and vendor material are not
product assets or build dependencies. The patched Iced renderer is included in
the checkout.

## License

Byakko-owned code, documentation and assets are **GPL-3.0-or-later**. See
[LICENSE](LICENSE). Byakko is distributed without warranty.

Third-party material retains its own licenses and notices. The vendored
`iced_tiny_skia` renderer retains its [MIT license](vendor/iced_tiny_skia/LICENSE)
and [patch provenance](vendor/iced_tiny_skia/BYAKKO-PATCH.md).

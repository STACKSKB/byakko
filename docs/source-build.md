# Build the source-only pre-alpha

The pre-alpha is a source checkout. No CI/CD, installer, signing service or
prebuilt binary download is required. Build the Iced desktop and independent
CLI explicitly; the root package's default GUI is the retained egui research
application.

## Prerequisites

Rust/Cargo 1.98.0 is the currently tested toolchain. Install the normal platform
linker: the MSVC C++ build tools on Windows, or a C/C++ build toolchain on Linux.
The first build needs access to crates.io; subsequent builds can use `--offline`
when the locked dependencies are cached. Use `--locked` to retain Cargo.lock.
Python 3.11+ is needed only for the optional source-license inventory scripts.

On Linux, use an active X11 or Wayland desktop session for the GUI. The selected
Iced build uses tiny-skia; X11/Wayland and keyboard-layout runtime libraries must
be available. Screen sampling separately loads `libX11.so.6` and optionally
`libXrandr.so.2`; audio sampling loads `libpulse.so.0`. `ldd` does not verify these
dynamically loaded libraries. Wayland GUI support does not imply Wayland screen
capture: screen-following currently supports X11 only.

## Checkout and build

```sh
git clone https://github.com/STACKSKB/byakko.git
cd byakko
cargo build --release --locked -p byakko-desktop -p byakko-cli
```

The patched renderer is included under `vendor/iced_tiny_skia` and selected by
Cargo's local patch. It does not need a separate download, submodule, manual
copy or vendor helper. A fresh extraction of tracked sources passed the Linux
release build on 2026-09-25. Its MIT license and patch provenance remain included.

On Linux:

```sh
./target/release/byakko-desktop --demo
./target/release/byakko-cli --help
```

On Windows (PowerShell):

```powershell
.\target\release\byakko-desktop.exe --demo
.\target\release\byakko-cli.exe --help
```

The demo uses an in-memory keyboard. For the attached Nia87, omit `--demo`.
Linux hardware access first needs the narrow permission setup in
[Linux installation](linux-install.md). Run the application as your ordinary
user, never as root. Close other configurator sessions before reading/editing.

## Local checks

Run before sharing a revision; no hosted automation is needed:

```sh
cargo fmt --all -- --check
cargo test --locked -p byakko-core -p byakko-devices -p byakko-desktop -p byakko-cli
cargo clippy --locked -p byakko-core -p byakko-devices -p byakko-desktop -p byakko-cli --all-targets -- -D warnings
cargo test --locked -p iced_tiny_skia --lib
```

Additional Linux permission-helper checks:

```sh
cargo build --release --locked -p byakko --no-default-features --bin byakko-hidraw-access
cargo test --locked -p byakko --no-default-features --bin byakko-hidraw-access
cargo clippy --locked -p byakko --no-default-features --bin byakko-hidraw-access -- -D warnings
```

The license inventory is optional development tooling. `cargo fetch --locked`
may be needed to cache the Windows and Linux dependency union before its offline
checks. Each output path must be new:

```sh
python3 -m unittest discover -s tools -p 'test_*.py'
python3 tools/check_dependency_licenses.py
python3 tools/check_dependency_licenses.py --package byakko-cli
python3 tools/audit_release_sources.py --output NEW_INVENTORY.json
```

Use `python` instead of `python3` if that names your Python 3.11+ installation
on Windows. No inventory script is part of the product build.

Keep the source revision (`git rev-parse HEAD`), OS, `rustc -Vv`, command and full
error output when reporting a build failure. Tests and a successful build do
not establish physical keyboard behavior; see [pre-alpha support](pre-alpha-support.md).

## License

Byakko-owned material is GPL-3.0-or-later; see [LICENSE](../LICENSE) and the
[repository license notice](../README.md#license). Third-party source retains
its own notices. The source checkout contains the vendored renderer's license
and supplemental notices under `packaging/notices`; Cargo obtains other locked
dependencies from their upstream packages during the build. Binary distribution
is outside this pre-alpha's chosen scope.

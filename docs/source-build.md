# Build Byakko (PRE-ALPHA)

These instructions build the native Iced desktop and independent CLI from the
current application branch. Prebuilt packages are being prepared separately;
this document does not announce a published release.

## Prerequisites

Use Git and Rust/Cargo **1.98.0**, the currently tested toolchain. The checkout
does not pin a toolchain automatically. Install Rust through rustup, then select
it for this checkout with `rustup override set 1.98.0` if needed. Keep
`Cargo.lock` and use `--locked`. The first build downloads Rust dependencies;
add `--offline` only after those dependencies are cached.

### Windows

Use the `x86_64-pc-windows-msvc` Rust toolchain and Visual Studio Build Tools with
**Desktop development with C++**, including the MSVC linker and Windows SDK.
Run the commands below in PowerShell with Cargo and the build tools available.
No vendor driver, Node.js or Python is needed to build the application.

### Linux

Install a C/C++ build toolchain and `pkg-config`. Run the GUI in an active X11
or Wayland desktop session with its display and keyboard-layout libraries
available. The renderer uses tiny-skia rather than requiring a Vulkan GPU.
Distribution package names vary; common Debian/Ubuntu prerequisites include
`build-essential`, `pkg-config`, `libxkbcommon-dev`, `libxkbcommon-x11-0`,
`libwayland-client0`, `libx11-6`, `libxrandr2` and `libpulse0`.

Screen sampling dynamically loads `libX11.so.6` and optionally `libXrandr.so.2`;
audio sampling loads `libpulse.so.0` and needs a PulseAudio server or PipeWire's
PulseAudio compatibility service. `ldd` does not check these dynamically loaded
libraries. Wayland can display the GUI, but **screen-following supports X11
only**.

## Checkout and build

```sh
git clone --branch master https://github.com/STACKSKB/byakko.git
cd byakko
cargo build --release --locked -p byakko-desktop -p byakko-cli
```

The local renderer patch under `vendor/iced_tiny_skia` is included in Git and
selected by Cargo. No submodule or manual renderer download is required. The
root package contains research utilities; build the named application packages
above rather than using an unqualified `cargo run`.

### Windows output

```powershell
.\target\release\byakko-desktop.exe --demo
.\target\release\byakko-cli.exe --help
```

### Linux output

```sh
./target/release/byakko-desktop --demo
./target/release/byakko-cli --help
cargo build --release --locked -p byakko --no-default-features --bin byakko-hidraw-access
```

The last command builds the Linux permission helper at
`target/release/byakko-hidraw-access`. Install it and the supplied **udev rule**
using [Linux installation](linux-install.md) before accessing hardware. Reload
the rules and reconnect the keyboard. Run the application as your normal
desktop user, not root.

Demo mode uses an in-memory keyboard. Omit `--demo` for the attached Nia87.
Close other configurator sessions first. These CLI commands only discover or
read the keyboard:

```sh
cargo run --release --locked -p byakko-cli -- devices
cargo run --release --locked -p byakko-cli -- read
```

Use `--help` for the complete command list. Snapshot apply commands write to the
keyboard; a successful build is not evidence that a new platform's writes or
recovery have been tested.

## Local checks

Run from the repository root:

```sh
cargo fmt --all -- --check
cargo test --workspace --all-targets --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --locked -p iced_tiny_skia --lib
```

Ignored hardware tests are not run by these commands. Do not enable them as an
unattended substitute for coordinated hardware acceptance. Linux helper tests
are included in the workspace checks and can also be run independently:

```sh
cargo test --locked -p byakko --no-default-features --bin byakko-hidraw-access
```

Optional license/source inventory tooling requires Python 3.11+. Use `python`
on Windows if that names your Python installation. Each output path must be new:

```sh
python3 -m unittest discover -s tools -p 'test_*.py'
python3 tools/check_dependency_licenses.py
python3 tools/check_dependency_licenses.py --package byakko-cli
python3 tools/audit_release_sources.py --output NEW_INVENTORY.json
```

`cargo fetch --locked` may be needed to cache dependencies for both platforms
before offline inventory checks. These scripts are development tools, not
runtime requirements.

Keep `git rev-parse HEAD`, OS, `rustc -Vv`, the command and full error output when
reporting a build failure. See the [current acceptance notes](rewrite-acceptance-20260927.md)
for tested behavior and outstanding physical checks.

## Distribution and licenses

Distribute executables with the matching source revision, GPL license and
applicable third-party notices. Byakko-owned material is GPL-3.0-or-later; see
[LICENSE](../LICENSE) and the [repository license notice](../README.md#license).
The checkout retains the renderer's MIT license and supplemental notices under
`packaging/notices`. Cargo obtains other locked dependencies from their upstream
packages during the build. A local release-profile build is not a published
GitHub release.

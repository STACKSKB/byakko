# Linux build and release readiness

## Iced desktop cross-link (2026-09-23)

At source commit `f228533`, the `byakko-desktop` release executable and the root package's
`byakko-hidraw-access` helper both cross-linked for
`x86_64-unknown-linux-gnu` using the locally retained Zig 0.15.2 and
`cargo-zigbuild 0.23.4`, with `--locked --offline`. Their ELF headers are
`7F 45 4C 46`. The 9,768,608-byte desktop SHA-256 is
`662149E6014A47F2A01A9B667FD25BE1D001EEEBBC4AD469810B04835960B285`;
the 381,640-byte helper SHA-256 is
`38C872B4C5BFA8004ECB1D97DBC99585357ED062BE341A955D5DC2C2A5BEA4E2`.
This proves linking only. Neither binary has launched on Linux, and linked
system-library availability, udev ACL behavior, and keyboard transactions
remain unverified. [Linux installation](linux-install.md) now names the Iced
desktop rather than the retained egui research GUI.

The locked desktop normal/build dependency metadata has 126 Windows and 175
Linux packages in the offline license gate. No selected package requires a
copyleft-only license; `self_cell` offers Apache-2.0 as an alternative to GPL.
The Iced desktop does not select legacy eframe or its bundled fonts. This
metadata check is not a complete source/asset audit or a distribution notice
bundle, so release packaging remains open.

## Linked build update

The retained egui research GUI also cross-linked to an ELF64 Linux executable
using cargo-zigbuild and Zig. See [the earlier build record](linux-link-build.md).
The earlier audit below is historical and describes the legacy package.

## Update after native-adapter migration

The audit below records the earlier HIDAPI dependency graph. That dependency was subsequently removed from both manifest and lockfile because of the build-script license discrepancy. The product now uses original `src/hid/windows.rs` and `src/hid/linux.rs` adapters with `windows-sys` and `libc`. A Rust Linux target was installed and both the core and full GUI passed `cargo check --target x86_64-unknown-linux-gnu --offline`. These checks do not link or execute a Linux program. Elevated WSL enumeration found only `docker-desktop`, not a general Linux development distro. The permission example and runtime test gates below still apply; dependency graph counts must be regenerated for release.

Audit date: 2026-09-22. This is a static review of `Cargo.toml`, `Cargo.lock`, target-specific `cargo tree --locked --offline`, `cargo metadata --locked --offline`, local registry manifests and build scripts, and the Windows device evidence in `docs/live-evidence.md`. No Linux build or runtime test was performed. This host has only the Windows Rust target installed; no usable Linux distro is available here, and WSL enumeration was denied.

## Build and runtime requirements

The default GUI build enables `eframe 0.36.2` with `glow`, X11, Wayland, and accessibility. The Linux `hidapi 2.6.7` feature is `linux-native-basic-udev`. Its selected branch reads `/sys/class/hidraw` through the pure Rust `basic-udev 0.1.2` crate, parses the HID report descriptor for usage page/usage, and opens `/dev/hidraw*` for feature-report ioctls. The selected HIDAPI build branch does **not** probe or link `libudev`, `libusb`, or the bundled HIDAPI C backend. `libudev-dev` and `libusb-1.0-dev` are therefore not required by this feature selection. A working udev service or equivalent device-node permission mechanism is still needed at runtime.

For a native Linux source build, install Rust 1.95 or newer (`eframe`/`egui 0.36.2` declare `rust-version = 1.95`), the `x86_64-unknown-linux-gnu` target, a C linker/toolchain, and `pkg-config`. The selected Wayland dependency enables `wayland-sys/dlopen`, and the X11 crate loads libraries dynamically; their local build scripts do not establish a hard `libwayland-dev` or `libX11-dev` requirement for this feature graph. Runtime still requires a graphical X11 or Wayland session, usable OpenGL/EGL/Mesa drivers, and matching shared libraries such as `libxkbcommon.so.0`, `libwayland-client.so.0` or `libX11.so.6`, and `libEGL.so.1`/`libGL.so.1`. Accessibility support additionally uses the desktop AT-SPI/D-Bus environment. Exact distribution package names and GPU support must be checked on the release target.

The Linux HID backend derives the USB interface number and HID usages from sysfs. The app currently accepts only VID `3151`, PID `4011` or `4015`, usage page `FFFF`, usage `0002`, and requires exactly one matching collection. The connected Windows device was `3151:4015`, interface 2, with the 20-byte descriptor shown in `docs/live-evidence.md`. Linux enumeration of that exact unit has **not** been verified. The app's 65-byte host buffer and feature-report read length also need a Linux hardware smoke test.

## Narrow hidraw permission example

The maintained native helper and installable rule now live in `src/bin/byakko-hidraw-access.rs` and `packaging/linux/70-byakko-nia87.rules`. Follow [Linux installation](linux-install.md). The Python example below is historical research, not the packaged setup. Linux udev matching and actual hardware access remain unverified.

Standard udev attribute matching can restrict VID/PID, but it does not expose the HID report descriptor's usage page and usage as a simple `ATTRS{}` pair. A VID/PID-only rule would grant access to every hidraw collection on that keyboard. For the observed `3151:4015`, `FFFF:0002` collection, use a fail-closed descriptor helper as the additional match. First confirm on Linux that `/sys/class/hidraw/hidrawN/device/report_descriptor` matches the physical unit; the Windows descriptor reconstruction is evidence, not a Linux read.

Example `/usr/local/libexec/byakko-hidraw-usage` (executable, root-owned):

```python
#!/usr/bin/python3
import pathlib
import sys

EXPECTED = bytes.fromhex(
    "06 ff ff 09 02 a1 01 09 02 15 80 25 7f 75 08 95 40 b1 02 c0"
)
if len(sys.argv) != 2 or not sys.argv[1].startswith("/devices/"):
    sys.exit(1)
descriptor = pathlib.Path("/sys") / sys.argv[1].lstrip("/") / "device/report_descriptor"
try:
    matches = descriptor.read_bytes() == EXPECTED
except OSError:
    matches = False
sys.exit(0 if matches else 1)
```

Example `/etc/udev/rules.d/70-byakko-nia87.rules` for a local desktop session using logind ACLs:

```udev
SUBSYSTEM=="hidraw", KERNEL=="hidraw*", ATTRS{idVendor}=="3151", ATTRS{idProduct}=="4015", PROGRAM=="/usr/local/libexec/byakko-hidraw-usage %p", TAG+="uaccess"
```

The USB VID and PID conditions match the same USB-device ancestor; the helper additionally requires the exact observed vendor-usage Application Collection and 64-byte feature-report descriptor. A missing or different descriptor fails closed. If Linux exposes a different but equivalent descriptor, inspect it and revise the helper before granting access. This example does not grant access to all HID devices, to PID `4011`, or to other collections of this keyboard. After installing the two files, reload udev rules and reconnect the keyboard, then verify the matching hidraw node's ACL and run a read-only identity/snapshot test as the desktop user. `uaccess` depends on an active local seat; a headless deployment would need a dedicated group and a similarly scoped rule.

## Dependency licenses and release work

The target-filtered runtime graph contains 203 Linux and 120 Windows package entries, including this project; adding build dependencies gives 213 and 130. `Cargo.lock` itself has 370 platform-wide entries, so auditing every lock entry as if it ships on both targets would misstate the product. In the two runtime graphs, the only dependency whose metadata offers a copyleft choice is `self_cell 1.3.0`: `Apache-2.0 OR GPL-2.0-only`. Its local crate includes both license files. The Apache-2.0 option is available; GPL is not forced by that expression. No target runtime package declares an LGPL-only or GPL-only license. The `r-efi` versions with an LGPL alternative occur in lock metadata but are absent from these Linux/Windows target graphs.

`hidapi 2.6.7` declares MIT. Its bundled C HIDAPI source offers BSD, original HIDAPI, or GPLv3 terms, but the selected Linux and Windows native backends do not compile that C source. Its `build.rs` carries a GPLv3 header despite the crate's MIT metadata; it runs during the build but is not linked into the application binary. Review that discrepancy before publishing a source distribution. The GUI's `epaint_default_fonts 0.36.2` metadata includes OFL-1.1 and Ubuntu-font-1.0 obligations in addition to the Rust code's MIT/Apache choice. Prepare third-party notices for the selected release artifacts and choose this project's own license; `Cargo.toml` currently has no `license` field and `publish = false`.

Before a Linux release, build and test with `--locked` on a real Linux host, run Clippy for that target, inspect linked shared libraries with `ldd`, and smoke-test both an X11 and a Wayland session if both remain enabled. Verify one-device enumeration, read-only identity/keymap/macro/lighting reads, and that the narrow udev rule grants only the intended hidraw node. Backed-up write/readback and restoration on Linux remain separate gates after those read-only checks. Neither a Windows build nor static target metadata establishes Linux runtime behavior.

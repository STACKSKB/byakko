# Byakko v0.0.1 Alpha

The native Nia87 configurator includes wired USB and 2.4 GHz receiver selection, per-key lighting Looks, and adaptive desktop layouts. Linux users can set up configuration permissions from the gear menu. Receiver notifications require a separate opt-in because the interface also carries key and pointer reports.

Run `byakko-desktop --demo` for the in-memory keyboard. See [source build](source-build.md) for native prerequisites and [Linux installation](linux-install.md) for hardware permissions. The public configurator supports diagnostic archive export. Hardware write recovery and cross-distribution compatibility remain subject to the limits in the README.

The release version is 0.0.1. Package managers may sort it below older builds labeled 0.1.0.

## Installation

- **Windows (x64)** — extract the portable ZIP, then run `byakko-desktop.exe`. The standalone EXE runs the same desktop app. The ZIP also includes the CLI, documentation and notices.
- **Debian/Ubuntu (amd64)** — install the downloaded DEB with `sudo apt install ./byakko*.deb` from a directory containing only this release's package.
- **Fedora/RPM systems (x86-64)** — use `sudo dnf install ./byakko*.rpm` in the download directory.
- **Arch/EndeavourOS (x86-64)** — keep the installer script and its matching `.pkg.tar.zst` together. Run the script with Bash as your normal user, or install the package directly with `sudo pacman -U ./byakko*.pkg.tar.zst`.
- **Portable Linux (x86-64)** — extract the tarball and launch `./byakko-desktop --demo` from inside the extracted directory. Launch without `--demo` for hardware.

System packages install a desktop menu entry. For keyboard access on Linux, use **Set up keyboard access** in Byakko's gear menu.

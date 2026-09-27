# Linux source-checkout setup (pre-alpha)

Byakko uses native hidraw access. The application runs as your desktop user;
installing the device permission rule requires administrator access once.
No background driver service, browser authorization, or JavaScript is involved.

Use the current [source build instructions](source-build.md) for Rust versions,
native dependencies and reproducible build commands. For a prebuilt Linux
archive, use the included executables and permission files instead of the two
Cargo build commands below. Run the archive's executables from a terminal first
to check compatibility; Linux archives are not automatically portable to older
glibc versions. The release must state its build platform and runtime minimum.
The permission helper and udev rule are required for device access:

```sh
cargo build --release --locked -p byakko-desktop
cargo build --release --locked -p byakko --no-default-features --bin byakko-hidraw-access
sudo install -d -m 0755 /usr/local/bin /usr/local/libexec /usr/local/share/applications /usr/local/share/icons/hicolor/256x256/apps
sudo install -o root -g root -m 0755 target/release/byakko-desktop /usr/local/bin/byakko-desktop
sudo install -o root -g root -m 0755 target/release/byakko-hidraw-access /usr/local/libexec/byakko-hidraw-access
sudo install -o root -g root -m 0644 packaging/linux/70-byakko-nia87.rules /etc/udev/rules.d/70-byakko-nia87.rules
sudo install -o root -g root -m 0644 packaging/linux/byakko.desktop /usr/local/share/applications/byakko.desktop
sudo udevadm control --reload-rules
```

Install the approved icon from a release archive:

```sh
sudo install -o root -g root -m 0644 assets/byakko.png /usr/local/share/icons/hicolor/256x256/apps/byakko.png
```

For a source checkout containing the approved icon, substitute
`packaging/icons/byakko.png` for `assets/byakko.png`.

These install commands replace files at the named installation paths. Review
existing files first if Byakko has already been installed. Keep the executable
and rule root-owned so an unprivileged process cannot replace the permission
check. The helper is a short-lived native executable invoked by udev, not a
resident driver.

Run the desktop from the application menu or a terminal without sudo. Its
demo mode exercises the UI without a keyboard:

```sh
byakko-desktop --demo
byakko-desktop
```

On the first hardware run, reconnect the keyboard after installing the udev
rule so the active desktop seat receives the new hidraw ACL. Byakko then
discovers and loads the Nia87 configuration collection automatically. The
research CLI is separate from this desktop release.

The rule matches USB `3151:4015` and grants access only if the helper finds
one of two exact observed 20-byte configuration descriptors. The Windows and
Linux observations differ only in the order of the final Report Size and
Report Count global items (`75 08 95 40` versus `95 40 75 08`); both describe
one unnumbered 64-byte Feature report. Other HID collections, PID `4011`,
missing descriptors and different descriptors do not match. A future
different descriptor will fail closed until independently verified. Do not
broaden the rule to every hidraw device to work around a mismatch.

The application also checks the opened collection itself. It requires the
vendor Application Collection to contain exactly one unnumbered Feature report
with 8-bit fields and a count of 64; a matching VID/PID and usage alone cannot
authorize a 65-byte host transaction. The dated
read-only HID acceptance is recorded in [Linux handoff](linux-handoff.md).

For diagnosis, identify the candidate `hidrawN` with the optional research
command `cargo run --locked -p byakko --no-default-features -- devices`, then
inspect:

```sh
udevadm info --attribute-walk --name=/dev/hidrawN
/usr/local/libexec/byakko-hidraw-access hidrawN
getfacl /dev/hidrawN
```

### Native onboard notifications

The current rule grants **configuration interface 2 only** (`FFFF/2`). It does
not grant the separate notification input interface 1 (`FFFF/1`, native report
ID 5, four bytes including the report ID). Consequently configuration can work
while the onboard-change listener reports a permission error. Manual refresh
remains available; installing this rule does not establish automatic onboard
synchronization on Linux.

The Windows input capability capture establishes the report shape, but not the
complete Linux hidraw descriptor. Before extending the helper, capture that
node's full `device/report_descriptor` from sysfs, its USB interface number and
physical USB ancestor, and verify that it belongs to the selected keyboard.
The Linux node may expose other reports on the same interface. Do not substitute
the small synthetic parser test fixture for a complete captured descriptor, or
grant access to every node matching the keyboard VID/PID. No live Linux input
node was available during the 2026-09-27 packaging review, so this permission
and physical listener acceptance gate remains open.

The helper prints `nia87-config` only on a match. `uaccess` grants access through
the active local desktop seat; an SSH/RDP/headless session may not receive that
ACL. Such deployments need a separately reviewed, dedicated-group policy.
The dated Linux evidence in [the handoff](linux-handoff.md) records successful
normal-user read-only checks on 2026-09-23 and 2026-09-25, including current-source
section reads and a single-sweep archive. It does not establish Linux writes,
restoration or GUI behavior. Follow the read-only-first sequence before writes.
Windows physical picture and steady-lighting evidence is tracked separately.

Rule syntax and local-seat ACL ordering are based on systemd's
[udev documentation](https://www.freedesktop.org/software/systemd/man/udev.html)
and [uaccess documentation discussion](https://github.com/systemd/systemd/issues/4288).
The rule and helper are original Byakko code.

## Dated Linux build check

The 2026-09-27 build of `d4f7ff9` passed for desktop, CLI and permission helper
using Rust 1.98.0 on Debian 13.7 x86-64 (glibc 2.41), with locked offline
dependencies. Both permission-helper tests passed. ELF version requirements
were GLIBC 2.39 for desktop and GLIBC 2.34 for CLI/helper; this is a symbol
requirement, not runtime acceptance on every distribution meeting that version.
`ldd` resolved all direct dependencies. X11, Xrandr, Wayland, xkbcommon and
PulseAudio runtime libraries were present; these dynamically loaded libraries
are not all listed by `ldd`. No libudev or ALSA development package was needed.
This check predates the final release revision and must be repeated for its
matching-source artifacts. It does not establish Linux GUI or hardware behavior.

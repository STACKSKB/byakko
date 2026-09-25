# Linux source-checkout setup (pre-alpha)

Byakko uses native hidraw access. The application runs as your desktop user;
installing the device permission rule requires administrator access once.
No background driver service, browser authorization, or JavaScript is involved.

This is a source-checkout procedure for pre-alpha evaluation, not a packaged
user installation guide. Use the current [source build instructions](source-build.md)
for Rust versions, native dependencies, and reproducible build commands. The
permission helper and udev rule below are still required by this checkout's
current hidraw access path:

```sh
cargo build --release --locked -p byakko-desktop
cargo build --release --locked -p byakko --no-default-features --bin byakko-hidraw-access
sudo install -d -m 0755 /usr/local/bin /usr/local/libexec /usr/local/share/applications
sudo install -o root -g root -m 0755 target/release/byakko-desktop /usr/local/bin/byakko-desktop
sudo install -o root -g root -m 0755 target/release/byakko-hidraw-access /usr/local/libexec/byakko-hidraw-access
sudo install -o root -g root -m 0644 packaging/linux/70-byakko-nia87.rules /etc/udev/rules.d/70-byakko-nia87.rules
sudo install -o root -g root -m 0644 packaging/linux/byakko.desktop /usr/local/share/applications/byakko.desktop
sudo udevadm control --reload-rules
```

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

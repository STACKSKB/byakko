# Linux desktop setup (hardware validation pending)

Byakko uses native hidraw access. The application runs as your desktop user;
installing the device permission rule requires administrator access once.
No background driver service, browser authorization, or JavaScript is involved.

Build on Linux with Rust and the native dependencies described in
[Linux readiness](linux-readiness.md):

```sh
cargo build --release --locked --bin byakko --bin byakko-hidraw-access
sudo install -d -m 0755 /usr/local/libexec
sudo install -o root -g root -m 0755 target/release/byakko-hidraw-access /usr/local/libexec/byakko-hidraw-access
sudo install -o root -g root -m 0644 packaging/linux/70-byakko-nia87.rules /etc/udev/rules.d/70-byakko-nia87.rules
sudo udevadm control --reload-rules
```

These install commands replace files at the named installation paths. Review
existing files first if Byakko has already been installed. Keep the executable
and rule root-owned so an unprivileged process cannot replace the permission
check. The helper is a short-lived native executable invoked by udev, not a
resident driver.

Reconnect the keyboard, then run the following without sudo:

```sh
target/release/byakko devices
target/release/byakko inspect
target/release/byakko gui
```

The rule matches USB `3151:4015` and grants access only if the helper finds the
exact observed configuration report descriptor. Other HID collections, PID
`4011`, missing descriptors and different descriptors do not match. The known
descriptor was reconstructed on Windows; its exact Linux representation has
not yet been measured. A legitimate but different Linux descriptor will fail
closed until independently verified. Do not broaden the rule to every hidraw
device to work around a mismatch.

For diagnosis, identify the candidate `hidrawN` from `devices`, then inspect:

```sh
udevadm info --attribute-walk --name=/dev/hidrawN
/usr/local/libexec/byakko-hidraw-access hidrawN
getfacl /dev/hidrawN
```

The helper prints `nia87-config` only on a match. `uaccess` grants access through
the active local desktop seat; an SSH/RDP/headless session may not receive that
ACL. Such deployments need a separately reviewed, dedicated-group policy.
Linux device reads, writes, restoration, and GUI runtime remain unverified;
cross-linking and helper unit tests do not establish hardware compatibility.

Rule syntax and local-seat ACL ordering are based on systemd's
[udev documentation](https://www.freedesktop.org/software/systemd/man/udev.html)
and [uaccess documentation discussion](https://github.com/systemd/systemd/issues/4288).
The rule and helper are original Byakko code.

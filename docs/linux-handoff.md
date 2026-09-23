# Linux Codex handoff

The product is the Rust/Iced `byakko-desktop` application, with a separate
`byakko-cli`. The root `byakko gui` command is the retained egui
research application. Use `AGENTS.md` for architecture and safety rules, and
`docs/linux-install.md` for the maintained installation procedure. Windows
captures and vendor fixtures under ignored paths are not in Git.

## Remaining TODO on Linux

- [x] Build the selected native packages, run their tests, and pass Clippy at
  the recorded Linux verification commit below:

  ```sh
  cargo build --release --locked -p byakko-desktop -p byakko-cli
  cargo build --release --locked -p byakko --no-default-features --bin byakko-hidraw-access
  cargo test --locked -p byakko-core -p byakko-devices -p byakko-desktop -p byakko-cli
  cargo clippy --locked -p byakko-core -p byakko-devices -p byakko-desktop -p byakko-cli --all-targets -- -D warnings
  cargo clippy --locked -p byakko --no-default-features --bin byakko-hidraw-access -- -D warnings
  ```

- [x] After pulling newer `master` commits, rerun the commands above and record
  the verified commit ID before hardware acceptance. The record below is a
  point-in-time check, not validation of every later Windows commit.

- [x] Record the distribution, display server, Rust version, build failures,
  and `ldd target/release/byakko-desktop` output. Launch
  `target/release/byakko-desktop --demo` in an X11 or Wayland desktop session;
  check the other display server if available. Verified on X11; Wayland was not
  available in this session. The demo uses an in-memory keyboard and does not
  need USB access.

- [x] If an Nia87 is attached, enumerate its hidraw collections and compare
  the configuration collection's report descriptor with the exact 20-byte
  descriptor in `docs/linux-install.md`. Run the native helper against that
  `hidrawN`. The udev rule is deliberately fail-closed: investigate a mismatch
  before changing it, and do not grant access to all HID devices.

- [ ] After installing the rule as described in `docs/linux-install.md`, verify
  that only the intended hidraw node gets the active-seat ACL. Run the desktop
  as the normal user. First run `target/release/byakko-cli devices`, then one
  `target/release/byakko-cli read` and repeat that read to check stability.
  If the first read fails, record its exact stderr and the ACL state before
  changing transport code or trying a setter. Then use `read-lighting`,
  `read-settings`, `read-colors`, and `read-macro slot-49` for
  initial read-only checks. Capture a new local archive with
  `target/release/byakko-cli capture-archive NEW_PATH.json` and repeat the read
  to establish stability. Compare the two saved files with
  `target/release/byakko-cli compare-archives FIRST.json SECOND.json`; an empty
  JSON list means the native configuration matches. The comparison runs offline
  and uses the same forward/reverse preflight as archive review, so an
  unrepresentable change is an error rather than a raw byte diff. These
  read-only commands send no setters. `plan-keymap` also sends no setter;
  `plan-settings`, `plan-lighting`, `plan-macro`, and `plan-colors` accept edited
  snapshots from the matching read commands and likewise send no setter.
  `apply-keymap`, `apply-settings`, `apply-lighting`, `apply-macro`, and
  `apply-colors` are explicit write commands
  and are not part of the initial unattended Linux smoke test.

  The configuration collection has an unnumbered 64-byte Feature report.
  Linux `HIDIOCSFEATURE` includes a leading zero report-number byte in the
  65-byte request. For `HIDIOCGFEATURE`, the kernel returns an unnumbered
  report's payload starting at byte zero; the Linux adapter prepends the
  host-side zero before passing the 65-byte result to the shared decoder.
  This matches the [kernel hidraw documentation](https://docs.kernel.org/hid/hidraw.html)
  but is not yet a successful device I/O test. Do not change that mapping
  merely because the ioctl returns 64 rather than 65 bytes.

- [ ] Check Linux removal/reconnect, one-device selection, GUI page entry
  reads, and clean shutdown. Record whether X11 and Wayland behave differently.
  Physical key output, macro playback/timing, RGB visual behavior and Linux
  write/readback/restoration remain separate acceptance gates.

- [ ] With an attached board and physical access, exercise the CLI keymap
  file workflow: save `read` output, edit one ordinary binding, run
  `plan-keymap`, then explicitly run `apply-keymap`. Confirm key output, the
  durable before-image, full readback and restoration. Do this only after the
  read-only transport checks and the existing recovery concern are resolved.

- [ ] After the same read-only and recovery gates, exercise one-field CLI
  settings apply and restoration from a freshly exported `read-settings`
  snapshot. Run `plan-settings` first; verify the before-image and full native
  readback after the explicit `apply-settings` command.

- [ ] After those gates, exercise one global-lighting file edit with
  `plan-lighting` and explicit `apply-lighting`; check visible output,
  before-image, full readback and restoration.

- [ ] If `read-lighting` finds a recognized stored host mode after a crashed
  streamer, verify that the GUI and CLI offer only an explicit onboard-effect
  exit. Plan against a fresh exact revision and retain the durable backup;
  physical exit/readback/restoration is not yet accepted. Do not auto-reset
  lighting on connect.

- [ ] With physical playback access, exercise one changed macro snapshot with
  `plan-macro` and explicit `apply-macro`; verify the backup, full slot readback,
  playback, and restoration. The currently observed empty slot 49 has stored
  repeat count zero; set a count in the editable range before proposing events.

- [ ] After read-only picture stability and recovery checks, exercise one
  per-key color edit with `plan-colors` and explicit `apply-colors`; verify the
  backup, full picture readback, visible output and restoration. Keep this
  pending while the Windows Iced picture write path lacks physical acceptance.

## Linux verification record (2026-09-23)

Checked on Debian 13 (Trixie), x86_64, kernel `6.12.107-1`, Rust `1.98.0`,
Cargo `1.98.0`, at source commit `4af1877` (`Validate host lighting frames
against selected source`). The following passed:

- Release builds for `byakko-desktop`, `byakko-cli`, and
  `byakko-hidraw-access` with `--locked`.
- `cargo test --locked -p byakko-core -p byakko-devices -p byakko-desktop -p byakko-cli`:
  271 tests passed, none failed.
- Clippy for the core, devices, desktop and CLI workspace packages with
  `--all-targets -- -D warnings`, and Clippy for `byakko-hidraw-access` with
  `-D warnings`.
- `ldd target/release/byakko-desktop`: all listed shared libraries resolved.

The CLI discovery check printed `Nia87 not connected`. The user confirmed the
keyboard is not connected to this machine, so that result is expected. No
hidraw node was available; descriptor matching, udev ACLs, and hardware reads
were not attempted.

The X11 server at `DISPLAY=:1` responded to `xdpyinfo` when run with display
access. The demo ran for 15 seconds without errors and was stopped by the
verification timeout. GUI startup is verified on X11; visual behavior and
Wayland startup remain unverified.

**Note for the Windows agent:** Linux release builds, tests, and Clippy all pass
at `4af1877`. The disconnected-device result is expected and does not indicate
a discovery regression. The in-memory demo starts on X11. No physical-device
behavior was assessed here.

## Linux verification update (2026-09-23, commit `69a987e`)

After pulling the newer `master`, the desktop/CLI release build, helper release
build, workspace tests, and both Clippy commands above passed again. `ldd`
reported no unresolved shared libraries, and the X11 demo remained running for
the 15-second smoke-test interval.

The connected USB device enumerated as VID/PID `3151:4015`; its configuration
collection is `/dev/hidraw2`, interface `02`. The native helper rejected
`hidraw2`: its Linux descriptor orders the final global items as `95 40 75 08`,
while the helper's exact expected bytes order them `75 08 95 40`. The helper
and udev rule were left unchanged pending review of this descriptor difference.
The other two collections did not match the helper either.

The Windows-side follow-up added only that second exact 20-byte descriptor to
the helper's allowlist. The udev rule and VID/PID gate remain unchanged. This
source change has unit tests but still needs a Linux helper rerun against
`hidraw2` before installation or ACL testing; do not infer a granted ACL from
the code change alone.

`/dev/hidraw2` was `root:root` mode `0600` with no active-user ACL. The read-only
CLI `devices` command identified it as the Nia87 configuration interface, but
`read` failed with `Permission denied`. Lighting, settings, colors, macro and
archive reads therefore could not be performed. No udev rules or permissions
were changed, and no device writes were attempted. Resolve the descriptor
allowlist mismatch and establish the narrow active-seat ACL before resuming the
read-only CLI and archive-stability checks.

**Note for the Windows agent:** Linux builds, tests, Clippy, and X11 demo
startup pass at `69a987e`. The connected board's Linux helper check fails only
the exact descriptor comparison shown above; preserve the fail-closed rule
until the Linux descriptor difference is reviewed. The current node has no
active-user ACL, so device reads are blocked.

Follow-up after source commit `a4fcb45`: the release builds, workspace tests,
both Clippy checks, `ldd`, and the X11 demo smoke test were rerun and passed.

## Linux descriptor and access check (2026-09-23, commit `09c9599`)

Rebuilt `byakko-hidraw-access` from this commit. It printed `nia87-config` for
`hidraw2` only; `hidraw0` and `hidraw1` were rejected. The connected board is
`3151:4015` and interface `02`; the accepted descriptor is the Linux ordering
listed in [Linux installation](linux-install.md).

After explicit user authorization, the root-owned helper and exact narrow udev
rule were installed temporarily. The rule added the `uaccess` tag only to the
matching configuration collection. `/dev/hidraw2` then had a `user:three:rw-`
ACL; the other collections remained rejected by the helper.

The latest `byakko-cli devices` identified `/dev/hidraw2` as the Nia87
configuration interface. The first normal-user `read` failed with:
`Error: "Device read failed: Unverified { problem: Read(\"unnumbered feature reply does not fit host buffer\") }"`.
The ACL was rechecked after the failure and remained present. Because the first
read failed, the repeat read, other section reads, and archive stability
comparison were not run. No device setters were run.

The user asked for the helper/rule to be removed after testing. The user later
ran the cleanup commands in an authenticated terminal. A follow-up check
confirmed `/usr/local/libexec/byakko-hidraw-access` and
`/etc/udev/rules.d/70-byakko-nia87.rules` are absent. At that check there were
no `/dev/hidraw*` nodes, so no live node ACL could be inspected; the previous
node's ACL does not survive node removal, and the removed rule cannot recreate
it. The user wants the eventual application image to avoid manual udev setup.

**Note for the Windows agent:** the updated exact-descriptor helper works on the
Linux descriptor and accepts only the configuration node. Linux permissions
were granted temporarily by the narrow rule, but the first read reaches the HID
transport and fails on the unnumbered feature-reply length. The temporary
helper/rule have since been removed. Do not change transport code or try a
setter without resolving the feature-reply framing with read-only evidence.

## Raw feature framing diagnostic blocked (2026-09-23)

A requested strictly read-only `HIDIOCGFEATURE` diagnostic was to send only the
known `0x80` identity request, using 65-byte and, if needed, 66-byte buffers,
and record the returned byte count plus the first and last eight response
bytes. It could not be run: the current environment has no `/dev/hidraw*`
nodes, and `lsusb` reports `unable to initialize libusb: -99`. The temporary
helper and udev rule are absent. No ioctl was issued, so there is no returned
count or response data; no permissions were changed. Repeat the diagnostic only
when the already validated Nia87 configuration collection is present, and keep
it read-only.

The earlier adapter error proves the first reply count was 65, because its
65-byte buffer rejects that count at the final fit check. The bytes were not
recorded. The adapter now handles both complete shapes for the descriptor's
64-byte payload: 64 payload bytes without a report ID, or 65 bytes with a
leading zero report ID. It rejects a 65-byte reply without that zero and all
partial lengths. Cross-target compilation and Clippy passed on Windows; the
new path still needs a Linux runtime read and full identity validation before
any write acceptance.

## Linux device-crate verification (2026-09-23, source `26554e0`)

On Linux at `26554e0919cd73c018db67e5b9c5b99af183883d`,
`cargo test --locked -p byakko-devices` passed: 180 unit tests and one
integration test passed, with no doc tests. `cargo clippy --locked
-p byakko-devices --all-targets -- -D warnings` also passed. The working tree
was clean after verification. No device access or permission changes were
made; these results do not replace the pending read-only hardware diagnostic.

## Linux CLI and device verification (2026-09-23, source `3ed3f3e`)

At `3ed3f3e549190f1c2e8cba9a401d08426fed56fc`, the following checks passed
on Linux with dependencies offline:

- `cargo build --locked --offline -p byakko-cli -p byakko-devices`
- `cargo test --locked --offline -p byakko-cli -p byakko-devices`: 197 tests
  passed (15 CLI unit tests, 181 device unit tests, and one device integration
  test); doc tests were empty.
- `cargo clippy --locked --offline -p byakko-cli -p byakko-devices --all-targets -- -D warnings`

No device access or permission changes were made. These checks do not verify
the new system-action records on a physical keyboard or the Linux runtime HID
framing.

## Known limits to carry forward

Linux release builds and X11 demo startup have been verified, but hidraw
permissions, 65-byte feature-report I/O and on-device behavior have **not**
been verified. The USB keyboard has firmware `0x0100`, profile 0 on the
observed Windows configuration collection `3151:4015`, interface 2,
`FFFF:0002`; a matching VID/PID alone does not authorize writes. A prior
fault-injected whole-archive apply caused unexplained collateral changes and
failed automatic recovery. An explicit later restore verified, but the root
cause is unresolved. Do not use archive apply or other live setters as the
first Linux smoke test; obtain a matching local before-image and resolve the
read-only transport/identity checks first. See
`Research/configuration-fault-verification.md` and `docs/parity-status.md`.

The stock-firmware Nia87 USB path comes first. QMK/VIA and 2.4 GHz remain
later adapters. Keep protocol-specific reports in `byakko-devices`, portable
session decisions in `byakko-core`, and the Iced GUI in `byakko-desktop`.

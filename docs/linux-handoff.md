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

- [ ] After pulling newer `master` commits, rerun the commands above and record
  the verified commit ID before hardware acceptance. The record below is a
  point-in-time check, not validation of every later Windows commit.

- [x] Record the distribution, display server, Rust version, build failures,
  and `ldd target/release/byakko-desktop` output. Launch
  `target/release/byakko-desktop --demo` in an X11 or Wayland desktop session;
  check the other display server if available. Verified on X11; Wayland was not
  available in this session. The demo uses an in-memory keyboard and does not
  need USB access.

- [ ] If an Nia87 is attached, enumerate its hidraw collections and compare
  the configuration collection's report descriptor with the exact 20-byte
  descriptor in `docs/linux-install.md`. Run the native helper against that
  `hidrawN`. The udev rule is deliberately fail-closed: investigate a mismatch
  before changing it, and do not grant access to all HID devices.

- [ ] After installing the rule as described in `docs/linux-install.md`, verify
  that only the intended hidraw node gets the active-seat ACL. Run the desktop
  as the normal user. Use `target/release/byakko-cli devices`, `read`,
  `read-lighting`, `read-settings`, `read-colors`, and `read-macro slot-49` for
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

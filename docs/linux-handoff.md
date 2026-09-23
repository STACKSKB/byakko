# Linux Codex handoff

The product is the Rust/Iced `byakko-desktop` application, with a separate
read-only `byakko-cli`. The root `byakko gui` command is the retained egui
research application. Use `AGENTS.md` for architecture and safety rules, and
`docs/linux-install.md` for the maintained installation procedure. Windows
captures and vendor fixtures under ignored paths are not in Git.

## TODO on a real Linux host

- [ ] Build and run the selected native packages and their tests from `master`:

  ```sh
  cargo build --release --locked -p byakko-desktop -p byakko-cli
  cargo build --release --locked -p byakko --no-default-features --bin byakko-hidraw-access
  cargo test --locked -p byakko-core -p byakko-devices -p byakko-desktop -p byakko-cli
  cargo clippy --locked -p byakko-core -p byakko-devices -p byakko-desktop -p byakko-cli --all-targets -- -D warnings
  cargo clippy --locked -p byakko --no-default-features --bin byakko-hidraw-access -- -D warnings
  ```

- [ ] Record the distribution, display server, Rust version, build failures,
  and `ldd target/release/byakko-desktop` output. Launch
  `target/release/byakko-desktop --demo` in an X11 or Wayland desktop session;
  check the other display server if available. The demo uses an in-memory
  keyboard and does not need USB access.

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
  unrepresentable change is an error rather than a raw byte diff. The CLI sends
  no setters.

- [ ] Check Linux removal/reconnect, one-device selection, GUI page entry
  reads, and clean shutdown. Record whether X11 and Wayland behave differently.
  Physical key output, macro playback/timing, RGB visual behavior and Linux
  write/readback/restoration remain separate acceptance gates.

## Known limits to carry forward

Windows cross-linking produced Linux ELF binaries, but Linux launch, hidraw
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

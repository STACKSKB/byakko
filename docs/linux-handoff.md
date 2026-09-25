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
  `target/release/byakko-cli read` once to establish the baseline.
  If the first read fails, record its exact stderr and the ACL state before
  changing transport code or trying a setter. Then use `read-lighting`,
  `read-settings`, `read-colors`, and `read-macro slot-49` for
  initial read-only checks. Capture a new local archive with
  `target/release/byakko-cli capture-archive NEW_PATH.json` using one complete
  sweep. Do not duplicate the sweep or reread every section as a preflight.
  Agent-directed diagnostic captures may be repeated as needed (user
  clarification, 2026-09-25); this does not change the executable's single-sweep
  policy. Where an earlier archive already exists, compare offline with
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

- [ ] Check Linux removal/reconnect, one-device selection, GUI connection loads with no reads on page navigation, and clean shutdown. Record whether X11 and Wayland behave differently.
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
  coordinated with the user while partial-upload recovery remains unresolved;
  newer Windows physical picture-upload evidence is recorded separately.

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
configuration interface. The first normal-user `read` on the pre-framing-fix
code failed with:
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
Linux descriptor and accepts only the configuration node. The earlier
pre-fix read failed on feature-reply framing; the updated adapter has now passed
a raw `0x80` identity exchange and stable CLI keymap reads on Linux. The
temporary helper/rule have since been removed. No setters were run. Other
panel reads and physical writes remain unverified.

## Raw feature framing diagnostic and Linux read (2026-09-23)

The first non-elevated shell check could see the sysfs HID collection but not
the host's `/dev/hidraw*` nodes; `lsusb` there failed with
`unable to initialize libusb: -99`. A read-only host inspection then confirmed
that the node was present. This was shell device-node visibility, not a missing
keyboard. As user `three`, `/dev/hidraw2` was `root:root` mode `0660` with
`user:three:rw-`; udev reported `TAGS=:seat:uaccess:` and
`CURRENT_TAGS=:seat:uaccess:`. Its sysfs parent identifies `3151:4015`,
interface `1.2`; the 20-byte descriptor was
`06 ff ff 09 02 a1 01 09 02 15 80 25 7f 95 40 75 08 b1 02 c0`.

On source `bf639cd6c32ae916ce89f3088e6cedfe39343bf2`, a raw read-only
identity exchange sent one 65-byte host report: report ID `00`, payload command
`80 00 00 00 00 00 00 7f` followed by zero padding. `HIDIOCSFEATURE` returned
65. `HIDIOCGFEATURE` with a 65-byte buffer returned **65**; response first 8
bytes were `00 80 00 01 00 00 00 00`, and last 8 were
`00 00 00 00 00 00 00 00`. The response includes the leading zero report ID,
followed by the `0x80` identity reply (`firmware 0x0100`). A 66-byte buffer was
not needed. Only the identity request was sent; no setters or permission
changes were made.

`byakko-cli devices` identified `/dev/hidraw2`. The CLI read-only snapshot
succeeded as user `three`, identifying firmware `0x0100`, profile 0. Two
successive captured `byakko-cli read` outputs both exited 0 and were byte-for-byte
identical: 36,186 bytes each, SHA-256
`75b94b74a58f5f012ada73b19763469eba22a8cab50b2ec1ded9278de8d8952f`.
This verifies Linux identity and keymap-read stability on the tested source;
other panel reads, GUI/runtime behavior, and live writes remain separate gates.
The adapter accepts both complete reply shapes, including the observed
65-byte form with a leading zero.

## Additional Linux read-only checks (2026-09-23, source `cc03b34`)

As user `three`, `byakko-cli list-macros` completed with
`{"capacity":50,"configured":[],"next_free":"slot-00"}`. The separate
`read-lighting`, `read-settings`, `read-colors`, and `read-macro slot-49`
commands all exited successfully. Two full `capture-archive` reads each
produced a 419,201-byte archive with SHA-256
`6bbc238f9b2e2bda6703f361168c68394c71b4e3e1c367187aa38fc4f77c7cf6`;
`compare-archives` returned `[]`. These read-only checks sent no setters and
changed no permissions. The two local archive files remain in `/tmp`.

## Picture context revision: Linux offline status (2026-09-23, source `9c1a4a0`)

At `9c1a4a0f8814338ba160a2ce47ba1b3a6df17477`, the Linux offline build passed
for `byakko-core`, `byakko-devices`, `byakko-cli`, and `byakko-desktop`. Their
pure tests passed (311 total), and Clippy passed with warnings denied. Linux
runtime verification of the new picture `context_revision` remains pending
until an Nia87 is attached. The
`[effect, option]` context and option-3 readback evidence in
`Research/picture-selector-audit.md` came from the Windows-connected board;
they are not Linux runtime evidence. No picture setter was sent.

## Linux device-crate verification (2026-09-23, source `26554e0`)

On Linux at `26554e0919cd73c018db67e5b9c5b99af183883d`,
`cargo test --locked -p byakko-devices` passed: 180 unit tests and one
integration test passed, with no doc tests. `cargo clippy --locked
-p byakko-devices --all-targets -- -D warnings` also passed. The working tree
was clean after verification. No device access or permission changes were
made at that verification point; the later hardware result is recorded above.

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

Linux release builds and X11 demo startup have been verified. On 2026-09-23,
Linux hidraw access and read-only identity, keymap, lighting, settings, colors,
macro and archive reads were verified, but GUI/runtime behavior, physical
output, and live writes with readback/restoration have **not** been verified.
The USB keyboard has firmware
`0x0100`, profile 0 on the
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

## Current read-only acceptance (2026-09-25, source `f8583d7`)

The user connected the Nia87 to this Linux laptop. The rebuilt CLI identified
`/dev/hidraw3`; the host node already had a `user:three:rw-` ACL. No permission
changes were made. The sandbox could enumerate sysfs but could not see the
host node (`No such file or directory`); the authorized host read as the normal
user succeeded. This was a sandbox visibility limit, not a HID protocol error.

One `read`, `read-lighting`, `read-settings`, `read-colors`, `read-macro slot-49`
and `capture-archive` pass succeeded on the rebuilt source. No setters were
sent. Picture output carried `Readback` evidence and context `[4,0]`; lighting
reported effect 4, speed 2, brightness 4, right/rainbow. The keymap JSON hash
matches the earlier Linux keymap evidence. The single archive's hash matches
both historical archives recorded at `cc03b34`; no second current archive was
required or performed.

Local evidence directory: `/tmp/byakko-readonly-20260925-r49cg5m_` (not tracked;
retain separately if long-term evidence is needed). Each command has stdout
and stderr. SHA-256:

| Output | Bytes | SHA-256 |
| --- | ---: | --- |
| Keymap JSON | 36186 | `75b94b74a58f5f012ada73b19763469eba22a8cab50b2ec1ded9278de8d8952f` |
| Lighting JSON | 686 | `ee3a8262010578c60ab498ceb6ae647f20ec0cd96649b9a1e5d6eef411c0c8df` |
| Settings JSON | 2291 | `f386ec90f827e8f3a931da9a3379426dc6be82439265143d0c76599567e3f433` |
| Colors JSON | 8186 | `998a47ff821693f0cca4c678e2243949929057e830d1960100e695e778995d45` |
| Macro 49 JSON | 1949 | `9a9fa47a17c3fa2d615e4165141068bcd6e89b4b9262ffe70c5eb92fefe55306` |
| Native archive wrapper | 419201 | `6bbc238f9b2e2bda6703f361168c68394c71b4e3e1c367187aa38fc4f77c7cf6` |

This closes the current Linux read-only selector-context check. It does not
establish physical writes, playback, power-cycle persistence, GUI behavior or
recovery. The desktop/CLI release build and all 376 selected product tests also
passed at this source; recovery classification tests do not prove hardware
recovery.

## Supervised lighting check and restoration (2026-09-25)

The user authorized a backed-up brightness round trip, then requested a larger
4→1 comparison and steady color. All commands used the same selected Linux node.
The source was `f8583d7` for product code (later commits changed documentation).
Ordinary setters returned TransportAccepted; separate reads matched submission.

Observed sequence:

1. Wave/rainbow brightness 4→3 read back correctly. The test's extra raw-byte
   assertion stopped because the codec also canonicalized rainbow RGB from
   `FF FF FF` to `FA FF FA`. The submitted snapshot predicted the same bytes;
   this was deterministic host encoding, not unexplained device drift. A full
   diagnostic archive confirmed only lighting indices 3, 5 and 7 differed.
2. At the user's request, Wave brightness 3→1 read back with only byte 3 changed.
3. Steady green (effect 1, fixed RGB `00 FF 00`) at brightness 4, then 1, read
   back correctly. Only raw byte 3 changed between those two static states.
   The user confirmed the keyboard became steady green, but reported no visible
   brightness difference. Physical brightness behavior remains unresolved.
4. The user requested original-state restoration. Offline comparison of a fresh
   current archive against the original planned **only one lighting change**,
   no keys/macros/colors/settings. The existing native restore example wrote
   that lighting report, but complete target verification mismatched. Automatic
   recovery then verified the preceding steady-green brightness-1 archive.
   No fault was injected. The pre-restore backup matched that captured state.
5. After the user selected visible-settings restoration, the normal lighting
   command restored Wave/rainbow at 4, speed 2, right, and a separate read matched
   the submitted state. Original lighting bytes 5 and 7 remain canonicalized to
   250 instead of 255. Do not call this exact raw restoration.

The failed exact-restore backup is
`Research/captures/backups/configuration-before-1790321203738684791.json`, SHA-256
`f205abfac43e0b3976ed1f6adc040f88b5d9fce65e257b4a0e3785e9bc19458f`.
Local captures, proposal files, readbacks, plan and restore stdout/stderr are
under `/tmp/byakko-readonly-20260925-r49cg5m_`. The exact-restore run had no
transport trace, and the application did not retain its mismatched capture;
there is insufficient evidence to identify that mismatch's bytes or cause.

This establishes one current Linux automatic recovery result (`Verified`) and
visible mode/color switching. It does not resolve the earlier Windows collateral
changes, establish exact raw archive restoration, or prove brightness dimming.
The user has been asked to obtain official-app brightness packet/visual evidence
from the Windows Agent before changing the codec. No further setters are implied.

Later on the same date, a fixed-camera steady-green 4→1 comparison showed lower
recorded LED output on this Linux unit. The current Wave/rainbow baseline
(`FA FF FA`) and camera settings were restored exactly, and a complete archive
comparison returned no changes. This resolves the narrow Linux static-green
brightness observation without a codec change; it does not resolve the older
exact `FF FF FF` archive restore mismatch. See
[camera method and evidence](../Research/brightness-investigation.md#linux-fixed-camera-comparison).


## Reconnect regression fix (2026-09-25)

A quick unplug/replug could leave the desktop executor bound to the previous
collection identity. Manual reconnect and feature retry previously reused that
worker, so repeated reads could report the same identity-change error. Explicit
Reconnect/retry and lighting, picture and settings retry now enumerate the
current unique supported collection, retire the previous executor, advance the
session generation and read through a newly bound executor. Pending edits are
retained; stale completions cannot satisfy the new read. Discovery compares all
pinned identity fields, including metadata when the OS reuses a path. Exact
collection selection inside each transaction and recovery is unchanged.

Memory-backend regressions cover missed discovery, retained draft/lighting
intent, stale completions and failed reattachment. This does not constitute a
physical unplug/replug test of the rebuilt executable. Onboard color and scalar
settings batching is now 200 ms; per-key RGB retains its 2 s window.

Validation: 90 desktop library tests and one composition-root identity test
passed, as did strict desktop Clippy, the Linux release build and the Windows
MSVC cross-target check. Logs: `/tmp/byakko-reconnect-{tests,clippy,build,windows-check}.log`.
The latter is a compile check, not native Windows runtime acceptance.

# Useful commands and acceptance tools

This inventory accompanies the egui retirement. Cargo commands below are
offline and local; hardware examples are listed for later user-coordinated
acceptance and were not run as part of this cleanup.

## Build and headless acceptance

From the repository root:

```powershell
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features --offline
cargo clippy --workspace --all-targets --all-features --offline -- -D warnings
cargo test --workspace --all-features --offline --lib --bins --tests
cargo run -p byakko-desktop --offline
```

The workspace checks compile hardware examples; they do not execute them. The
root package is now a diagnostic CLI and research-example host, not a GUI
library. `byakko-desktop` is the ordinary native UI target.

## Root diagnostic CLI

Use `cargo run -p byakko --offline -- --help` for the authoritative list.
`devices`, `inspect`, `descriptor`, `inspect-lighting`, and `inspect-settings`
are read-only device diagnostics. `inspect-configuration`,
`plan-configuration CURRENT TARGET`, and file validation operate without
device I/O; file exports read from the device but send no writes.
`capture-configuration PATH` performs one read-only full archive sweep. The
archive restore command is not exposed by the public Iced workflow; developer
restore APIs remain available for coordinated research.

Commands beginning `verify-` are physical round-trip acceptance tools. They
write a fixture, verify the result, and restore the captured before-image where
their workflow supports it. Run only when the exact device, backup location,
fixture, and recovery outcome have been reviewed. `restore-keymaps` and
`restore-macro` are explicit device writes from backups.

## Retained support binaries

`rpc_capture` remains an opt-in TCP relay/capture tool for the official
configurator research flow. It listens on `127.0.0.1:3815`, forwards to
`127.0.0.1:3814`, and creates new captures under `Research/captures`:

```powershell
cargo run -p byakko --bin rpc_capture --features research-tools --offline
```

The Linux `byakko-hidraw-access` helper validates the exact observed report
descriptor through sysfs and prints the udev tag; it never opens the device
node:

```sh
cargo build -p byakko --bin byakko-hidraw-access --offline
./target/debug/byakko-hidraw-access hidrawN
```

## Retained example tools

| Cargo invocation (append `--offline`) | Purpose / acceptance evidence | Device effect |
|---|---|---|
| `cargo run -p byakko-devices --example hid_selection_smoke --features research-tools` | Collection filtering and exact-target selection | Read-only discovery/open check |
| `cargo run -p byakko-devices --example read_keymap --features research-tools` | Raw Nia87 keymap snapshot | Read-only |
| `cargo run -p byakko-devices --example read_lighting --features research-tools` | Lighting report decode | Read-only |
| `cargo run -p byakko-devices --example read_picture --features research-tools` | Picture report decode | Read-only |
| `cargo run -p byakko-devices --example read_settings --features research-tools` | Settings report decode | Read-only |
| `cargo run -p byakko-devices --example sample_displays --features research-tools` | Windows display sampler availability | No HID I/O |
| `cargo run -p byakko-devices --example exercise_picture --features research-tools -- --write-f1 NEW_OUTPUT_DIRECTORY` | Twelve repeated F1 picture uploads with report trace | Writes; captures before-image but does not restore automatically |
| `cargo run -p byakko --example compare_macro_capture --features research-tools` | Offline macro-capture comparison | No device I/O |
| `cargo run -p byakko --example verify_configuration_roundtrip --features research-tools` | Full archive restore/recovery research | Writes and restores; coordinated acceptance only |
| `cargo run -p byakko --example restore_configuration_archive --features research-tools` | Developer archive restore path | Writes; coordinated acceptance only |
| `cargo run -p byakko --example restore_lighting_backup --features research-tools` | Lighting backup restore path | Writes; coordinated acceptance only |
| `cargo run -p byakko --example verify_macro_events --features research-tools` | Macro event encoding/playback acceptance | Writes; coordinated acceptance only |
| `cargo run -p byakko --example verify_macro_boundaries --features research-tools` | Macro storage boundary behavior | Writes; coordinated acceptance only |
| `cargo run -p byakko --example probe_macro_slot50 --features research-tools` | Last advertised macro slot investigation | Reads device; use only for planned diagnostic capture |
| `cargo run -p byakko --example capture_configuration_trace --features research-tools` | Capture transport/configuration trace | Read-only diagnostic capture |
| `cargo run -p byakko --example fn_capture_replay --features research-tools` | Fn report capture and replay research | May write; coordinated acceptance only |
| `cargo run -p byakko --example lighting_families --features research-tools` | Lighting family encoding/round-trip evidence | Writes and restores; coordinated acceptance only |
| `cargo run -p byakko --example lighting_camera_check --features research-tools` | Visible lighting behavior check | Writes and restores; coordinated acceptance only |
| `cargo run -p byakko --example host_frames_camera_check --features research-tools` | Host lighting stream/restore check | Streams and restores; coordinated acceptance only |
| `cargo run -p byakko --example sleep_capture_replay --features research-tools` | Sleep-setting capture/replay | Writes and restores; coordinated acceptance only |
| `cargo run -p byakko --example sample_audio --features research-tools` | OS audio sampler availability | No HID I/O |

To compile all feature-gated examples without executing them:

```powershell
cargo check -p byakko --all-targets --all-features --offline
cargo check -p byakko-devices --all-targets --all-features --offline
```

Never infer physical acceptance from compilation or headless tests. Preserve
the open Iced physical settings-write, host-stream, Linux runtime, and failed
recovery gates documented in the acceptance records.

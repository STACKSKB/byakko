# Linux cross-link build from Windows

On 2026-09-22, the default GUI executable linked successfully for `x86_64-unknown-linux-gnu` from this Windows checkout. This goes beyond the earlier `cargo check`: the output at `target/x86_64-unknown-linux-gnu/debug/byakko` is a 300,295,976-byte ELF64 executable (header begins `7F 45 4C 46 02 01 01`). Its SHA-256 in this build was `9E9099FD9EAF79460AF7B0654BFFF6AEB8A6EFDC6906B6A09C4DAC50DF357A93`.

The host already had the Rust target installed. It had no Zig or `cargo-zigbuild` executable, no usable general WSL Linux distro, and no running Docker daemon. I downloaded the official [Zig 0.15.2 Windows x64 archive](https://ziglang.org/download/0.15.2/zig-x86_64-windows-0.15.2.zip) and the upstream [cargo-zigbuild 0.23.4 Windows x64 archive](https://github.com/rust-cross/cargo-zigbuild/releases/tag/v0.23.4) into the ignored `Research/extracted` directory, then extracted them there. The Zig archive SHA-256 was `3A0ED1E8799A2F8CE2A6E6290A9FF22E6906F8227865911FB7DDEDC3CC14CB0C`, matching the [official Zig download index](https://ziglang.org/download/index.json). The `cargo-zigbuild` archive SHA-256 was `CD1226091F9F99AC7B46FB413968FB0B46EDBE3EA961817D31B968352DE4D4A6` (locally recorded; no separate checksum comparison was made). No product dependency or lockfile edits were needed.

From the repository root in PowerShell, with those archives extracted:

```powershell
rustup target list --installed
$env:PATH=(Resolve-Path 'Research\extracted\zig-x86_64-windows-0.15.2').Path + ';' + $env:PATH
& 'Research\extracted\zig-x86_64-windows-0.15.2\zig.exe' version
& 'Research\extracted\cargo-zigbuild\cargo-zigbuild.exe' zigbuild --target x86_64-unknown-linux-gnu --locked --offline
```

The final command reported `Finished dev profile [unoptimized + debuginfo]` in 33.28 seconds. `cargo-zigbuild` creates its linker wrapper cache under Windows LocalAppData, so the build needed filesystem access there in addition to the repository. The first sandboxed attempt failed with `Access is denied` while creating that cache; the authorized run completed. The resulting binary has not been run on Linux, and its shared-library requirements have not been inspected with Linux `ldd`. Before release, run it in an X11 or Wayland session on the intended Linux distribution and perform the device and permission checks in `docs/linux-readiness.md`.

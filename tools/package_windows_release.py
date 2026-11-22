"""Package the standalone x64 Windows desktop and portable ZIP built with llvm-mingw.

Use Rust 1.98.0 with the x86_64-pc-windows-gnullvm target and the official
llvm-mingw UCRT toolchain. From the repository root in a POSIX shell:

    export PATH="$LLVM_MINGW/bin:$PATH"
    export CARGO_TARGET_X86_64_PC_WINDOWS_GNULLVM_LINKER=x86_64-w64-mingw32-clang
    export CC_x86_64_pc_windows_gnullvm=x86_64-w64-mingw32-clang
    export CXX_x86_64_pc_windows_gnullvm=x86_64-w64-mingw32-clang++
    mkdir -p target/windows-static-runtime
    ln -sf "$LLVM_MINGW/x86_64-w64-mingw32/lib/libunwind.a" target/windows-static-runtime/libunwind.a
    export CARGO_TARGET_X86_64_PC_WINDOWS_GNULLVM_RUSTFLAGS="-L native=$PWD/target/windows-static-runtime -C link-arg=-static"
    cargo build --release --locked --target x86_64-pc-windows-gnullvm -p byakko-desktop -p byakko-cli
    cargo fetch --locked
    python3 tools/package_windows_release.py --llvm-mingw "$LLVM_MINGW" --output-dir DIST

The distribution includes the locked Cargo inventory and toolchain runtime notices.
"""
import argparse
import hashlib
import json
from pathlib import Path

from package_release import VERSION, assemble, run, write_archive


TARGET = "x86_64-pc-windows-gnullvm"
# System libraries used by the desktop/CLI. Reject newly introduced DLL
# dependencies until their availability and redistribution needs are reviewed.
WINDOWS_DLLS = set("""
    advapi32 bcryptprimitives cfgmgr32 dwmapi gdi32 hid imm32 kernel32
    ntdll ole32 setupapi shell32 user32 uxtheme
""".split())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--llvm-mingw", required=True, type=Path)
    parser.add_argument("--output-dir", required=True, type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    toolchain = args.llvm_mingw.resolve()
    build = root / "target" / TARGET / "release"
    files, executables = assemble(root, build, "windows", root / "packaging/icons/byakko.png")
    metadata = json.loads(files["BUILD.json"])
    metadata.update(target=TARGET, toolchain=toolchain.name, imports={})
    inspector = toolchain / "bin/llvm-readobj"
    for name in sorted(executables):
        headers = run(root, inspector, "--file-headers", build / name)
        if "IMAGE_FILE_MACHINE_AMD64" not in headers:
            raise ValueError(f"Expected Windows x64 image: {name}")
        imports = sorted(line.strip().removeprefix("Name: ").lower()
                         for line in run(root, inspector, "--coff-imports", build / name).splitlines()
                         if line.startswith("  Name: "))
        if not imports or any(not (dll.startswith("api-ms-win-") or
                                   dll.removesuffix(".dll") in WINDOWS_DLLS) for dll in imports):
            raise ValueError(f"Review runtime dependencies of {name}: {imports}")
        metadata["imports"][name] = imports
    files["BUILD.json"] = (json.dumps(metadata, indent=2) + "\n").encode()
    files["THIRD-PARTY/llvm-mingw/LICENSE.TXT"] = (toolchain / "LICENSE.TXT").read_bytes()
    crt = toolchain / "x86_64-w64-mingw32/share/mingw32"
    for name in ["COPYING", "COPYING.MinGW-w64.txt", "COPYING.MinGW-w64-runtime.txt",
                 "COPYING.winpthreads.txt", "COPYING.winstorecompat.txt"]:
        files[f"THIRD-PARTY/llvm-mingw/{name}"] = (crt / name).read_bytes()
    output = args.output_dir
    output.mkdir(parents=True, exist_ok=True)
    stem = f"byakko-{VERSION}-windows-x86_64"
    zip_path = output / f"{stem}-portable.zip"
    exe_path = output / f"{stem}.exe"
    if zip_path.exists() or exe_path.exists():
        raise FileExistsError(f"Release output already exists in {output}")
    write_archive(zip_path, files, executables, "windows")
    with exe_path.open("xb") as handle:
        handle.write(files["byakko-desktop.exe"])
    for path in (zip_path, exe_path):
        print(f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.name}")


if __name__ == "__main__":
    main()

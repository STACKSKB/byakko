"""Assemble portable pre-alpha archives from already-built native executables.

Uses the locked release dependency inventory and preserves all discovered notice
texts, including supplemental upstream notices. Never overwrites an archive.
Build binaries from the recorded revision before invoking this tool.
"""
import argparse
import gzip
import hashlib
import io
import json
import subprocess
import tarfile
import zipfile
from pathlib import Path

from audit_release_sources import inventory, NOTICE_PREFIXES


VERSION = "v0.1.0-pre-alpha.1"
ICED_SUPPLEMENT = {
    ("iced_core", "0.14.0"), ("iced_debug", "0.14.0"),
    ("iced_futures", "0.14.0"), ("iced_graphics", "0.14.0"),
    ("iced_program", "0.14.0"), ("iced_renderer", "0.14.0"),
    ("iced_runtime", "0.14.0"), ("iced_tiny_skia", "0.14.1"),
    ("iced_widget", "0.14.2"), ("iced_winit", "0.14.1"),
}


def run(root, *args):
    return subprocess.check_output(args, cwd=root).decode("utf-8").strip()


def notice_bundle(root, audit):
    metadata = json.loads(run(root, "cargo", "metadata", "--format-version", "1",
                              "--locked", "--offline"))
    sources = {(p["name"], p["version"]): Path(p["manifest_path"]).parent
               for p in metadata["packages"]}
    files = {}
    for row in audit["packages"]:
        key = row["name"], row["version"]
        source = sources[key]
        notices = [p for p in source.rglob("*") if p.is_file()
                   and p.name.lower().startswith(NOTICE_PREFIXES)]
        supplement = None
        if key in ICED_SUPPLEMENT:
            supplement = root / "packaging/notices/iced-0.14"
        elif key == ("clipboard-win", "5.4.1"):
            supplement = root / "packaging/notices/clipboard-win-5.4.1"
        if supplement:
            required = "LICENSE-MIT" if key in ICED_SUPPLEMENT else "LICENSE-BSL-1.0"
            if not (supplement / required).is_file():
                raise ValueError(f"Missing supplemental notice: {supplement / required}")
            for p in supplement.iterdir():
                if p.is_file():
                    files[f"THIRD-PARTY/{row['name']}-{row['version']}/upstream/{p.name}"] = p.read_bytes()
        if not notices and not supplement:
            raise ValueError(f"Missing license/notice texts for {key}")
        for p in notices:
            files[f"THIRD-PARTY/{row['name']}-{row['version']}/{p.relative_to(source).as_posix()}"] = p.read_bytes()
    files["THIRD-PARTY/inventory.json"] = (json.dumps(audit, indent=2) + "\n").encode()
    return files


def assemble(root, build, platform, icon):
    audit = inventory(root)
    files = notice_bundle(root, audit)
    executables = ["byakko-desktop", "byakko-cli"]
    if platform == "linux":
        executables.append("byakko-hidraw-access")
    for name in executables:
        filename = name + (".exe" if platform == "windows" else "")
        data = (build / filename).read_bytes()
        expected_magic = b"MZ" if platform == "windows" else b"\x7fELF"
        if not data.startswith(expected_magic):
            raise ValueError(f"Wrong executable format for {platform}: {build / filename}")
        files[filename] = data
    for name in ["LICENSE", "README.md", "docs/source-build.md", "docs/local-storage.md",
                 f"docs/release-notes-{VERSION}.md"]:
        files[name] = (root / name).read_bytes()
    files[f"assets/{icon.name}"] = icon.read_bytes()
    files["assets/README.md"] = (root / "packaging/icons/README.md").read_bytes()
    if platform == "linux":
        for name in ["docs/linux-install.md", "packaging/linux/70-byakko-nia87.rules",
                     "packaging/linux/byakko.desktop"]:
            files[name] = (root / name).read_bytes()
    revision = run(root, "git", "rev-parse", "HEAD")
    files["BUILD.json"] = (json.dumps({"version": VERSION, "revision": revision,
        "platform": platform, "rustc": run(root, "rustc", "--version"),
        "cargo_lock_sha256": hashlib.sha256((root / "Cargo.lock").read_bytes()).hexdigest(),
        "binaries": {name: hashlib.sha256(files[name]).hexdigest()
                     for name in files if name.endswith(".exe") or name in executables}}, indent=2) + "\n").encode()
    return files, set(name + (".exe" if platform == "windows" else "") for name in executables)


def write_archive(output, files, executables, platform):
    prefix = output.name.removesuffix(".zip").removesuffix(".tar.gz")
    with output.open("xb") as handle:
        if platform == "windows":
            with zipfile.ZipFile(handle, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
                for name, data in sorted(files.items()):
                    entry = zipfile.ZipInfo(f"{prefix}/{name}", (1980, 1, 1, 0, 0, 0))
                    entry.compress_type = zipfile.ZIP_DEFLATED
                    entry.external_attr = 0o100644 << 16
                    archive.writestr(entry, data)
        else:
            with gzip.GzipFile(fileobj=handle, mode="wb", filename="", mtime=0) as compressed:
                with tarfile.open(fileobj=compressed, mode="w") as archive:
                    for name, data in sorted(files.items()):
                        entry = tarfile.TarInfo(f"{prefix}/{name}")
                        entry.size = len(data)
                        entry.mode = 0o755 if name in executables else 0o644
                        entry.mtime = 0
                        archive.addfile(entry, io.BytesIO(data))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--platform", choices=["windows", "linux"], required=True)
    parser.add_argument("--build-dir", type=Path, default=Path("target/release"))
    parser.add_argument("--icon", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    files, executables = assemble(root, args.build_dir.resolve(), args.platform, args.icon.resolve())
    args.output.parent.mkdir(parents=True, exist_ok=True)
    write_archive(args.output, files, executables, args.platform)
    print(f"{args.output}: {hashlib.sha256(args.output.read_bytes()).hexdigest()}")


if __name__ == "__main__":
    main()

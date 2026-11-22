"""Build native Linux installers from an unpacked Byakko portable release.

Usage: python3 tools/package_linux_formats.py LINUX_STAGE RELEASE_DIRECTORY
Requires nFPM 2, GNU readelf, desktop-file-validate and Python 3.9+. No root or package installation.
The portable payload provides the version, binaries, launcher, icon and notices.
"""
import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import tempfile
from pathlib import Path


BINARIES = {
    "byakko-desktop": "/usr/bin/byakko-desktop",
    "byakko-cli": "/usr/bin/byakko-cli",
    "byakko-hidraw-access": "/usr/libexec/byakko/byakko-hidraw-access",
}
MAINTAINER = "Byakko <byakko@example.invalid>"


def sha256(path):
    with path.open("rb") as f:
        digest = hashlib.sha256()
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            digest.update(chunk)
        return digest.hexdigest()


def required_glibc(stage):
    versions = []
    for name in BINARIES:
        path = stage / name
        with path.open("rb") as f:
            header = f.read(20)
        if header[:6] != b"\x7fELF\x02\x01" or header[18:20] != b"\x3e\x00":
            raise ValueError(f"Expected x86-64 Linux ELF: {path}")
        info = subprocess.check_output(["readelf", "--version-info", path], text=True)
        versions.extend(tuple(map(int, v.split(".")))
                        for v in re.findall(r"\bGLIBC_(\d+(?:\.\d+)+)\b", info))
    return ".".join(map(str, max(versions)))


def package_contents(stage, work):
    docs = "/usr/share/doc/byakko"
    licenses = "/usr/share/licenses/byakko"
    launcher = work / "byakko.desktop"
    original = (stage / "packaging/linux/byakko.desktop").read_text()
    launcher.write_text(original.replace("Exec=/usr/local/bin/byakko-desktop",
                                         "Exec=/usr/bin/byakko-desktop"))
    subprocess.run(["desktop-file-validate", launcher], check=True)
    entries = []

    def add(path, destination, mode=0o644):
        entries.append({"src": str(path), "dst": destination, "file_info": {"mode": mode}})

    for name, destination in BINARIES.items():
        add(stage / name, destination, 0o755)
    add(launcher, "/usr/share/applications/byakko.desktop")
    add(stage / "assets/byakko.png", "/usr/share/icons/hicolor/256x256/apps/byakko.png")
    add(stage / "LICENSE", f"{licenses}/LICENSE")
    # Debian's conventional copyright path links to the complete installed license.
    entries.append({"type": "symlink", "packager": "deb",
                    "src": "../../licenses/byakko/LICENSE", "dst": f"{docs}/copyright"})
    for name in ["README.md", "BUILD.json"]:
        add(stage / name, f"{docs}/{name}")
    for folder, destination in [
        ("THIRD-PARTY", f"{licenses}/THIRD-PARTY"),
        ("docs", docs),
        ("packaging/linux", f"{docs}/examples"),
    ]:
        for path in sorted((stage / folder).rglob("*")):
            if path.is_file() and path.name != "byakko.desktop":
                add(path, f"{destination}/{path.relative_to(stage / folder).as_posix()}")
    return entries


def distribution_dependencies(glibc):
    # X11/Wayland, keyboard layout and optional screen/audio backends are loaded
    # dynamically, so ldd alone omits their package dependencies.
    shared = ["libgcc_s.so.1", "libX11.so.6", "libX11-xcb.so.1", "libxcb.so.1",
              "libXi.so.6", "libXcursor.so.1", "libXrandr.so.2", "libwayland-client.so.0",
              "libxkbcommon.so.0", "libxkbcommon-x11.so.0", "libpulse.so.0"]
    return {
        "deb": [f"libc6 (>= {glibc})", "libgcc-s1", "libx11-6", "libx11-xcb1", "libxcb1",
                "libxi6", "libxcursor1", "libxrandr2", "libwayland-client0", "libxkbcommon0",
                "libxkbcommon-x11-0", "libpulse0"],
        "rpm": [f"glibc >= {glibc}"] + [f"{library}()(64bit)" for library in shared],
        "archlinux": [f"glibc>={glibc}", "libgcc_s.so=1-64", "libx11", "libxcb", "libxi",
                      "libxcursor", "libxrandr", "wayland", "libxkbcommon",
                      "libxkbcommon-x11", "libpulse"],
    }


def build(stage, output, nfpm):
    manifest = json.loads((stage / "BUILD.json").read_text())
    version = manifest["version"].removeprefix("v")
    if manifest["platform"] != "linux" or not re.fullmatch(r"\d+\.\d+\.\d+", version):
        raise ValueError("Expected a Linux portable release with a numeric version")
    for name in BINARIES:
        if sha256(stage / name) != manifest["binaries"][name]:
            raise ValueError(f"{name}: staged binary differs from the release manifest")
    if not (stage / "THIRD-PARTY/inventory.json").is_file():
        raise ValueError("Portable release is missing its third-party inventory")
    glibc = required_glibc(stage)
    dependencies = distribution_dependencies(glibc)
    filenames = {
        "deb": f"byakko_{version}-1_amd64.deb",
        "rpm": f"byakko-{version}-1.x86_64.rpm",
        "archlinux": f"byakko-{version}-1-x86_64.pkg.tar.zst",
    }
    installer = f"install-byakko-{version}-arch.sh"
    output.mkdir(parents=True, exist_ok=True)
    for name in [*filenames.values(), installer]:
        if (output / name).exists():
            raise FileExistsError(output / name)
    with tempfile.TemporaryDirectory(dir=output) as directory:
        work = Path(directory)
        config = {
            "name": "byakko", "arch": "amd64", "platform": "linux",
            "version": version, "release": "1", "maintainer": MAINTAINER,
            "description": f"Native Nia87 keyboard configurator (alpha). Requires glibc {glibc} or later.",
            "homepage": "https://github.com/STACKSKB/byakko", "license": "GPL-3.0-or-later",
            "section": "utils", "priority": "optional", "contents": package_contents(stage, work),
            "archlinux": {"packager": MAINTAINER},
        }
        config_file = work / "nfpm.json"
        for format_name, filename in filenames.items():
            config_file.write_text(json.dumps({**config, "depends": dependencies[format_name]}))
            subprocess.run([nfpm, "package", "--packager", format_name, "--config", config_file,
                            "--target", work / filename], check=True)
        template = Path(__file__).resolve().parents[1] / "packaging/linux/install-arch.sh.in"
        script = work / installer
        arch = filenames["archlinux"]
        script.write_text(template.read_text().replace("@PACKAGE@", arch)
                          .replace("@SHA256@", sha256(work / arch)))
        script.chmod(0o755)
        # Publish complete artifacts without ever replacing an existing file.
        for name in [*filenames.values(), installer]:
            os.link(work / name, output / name)
    print(f"Built Debian, RPM and Arch installers for x86-64, glibc >= {glibc}.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("stage", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--nfpm", default=shutil.which("nfpm") or "nfpm")
    args = parser.parse_args()
    build(args.stage.resolve(), args.output.resolve(), args.nfpm)

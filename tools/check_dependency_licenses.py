"""Offline metadata gate for the current native dependency graphs.

This is not a source-header audit or a distribution notice generator. Unknown
expressions fail for review; font asset obligations are reported explicitly.
"""
import argparse
import json
import re
import subprocess
from pathlib import Path


TARGETS = ("x86_64-pc-windows-msvc", "x86_64-unknown-linux-gnu")
CHOICES = {
    "MIT": "MIT",
    "Apache-2.0": "Apache-2.0",
    "ISC": "ISC",
    "Zlib": "Zlib",
    "BSL-1.0": "BSL-1.0",
    "BSD-2-Clause": "BSD-2-Clause",
    "BSD-3-Clause": "BSD-3-Clause",
    "MIT OR Apache-2.0": "MIT",
    "MIT/Apache-2.0": "MIT",
    "Apache-2.0/MIT": "MIT",
    "Apache-2.0 OR MIT": "MIT",
    "Apache-2.0 AND MIT": "Apache-2.0 AND MIT",
    "Apache-2.0 OR GPL-2.0-only": "Apache-2.0",
    "Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT": "MIT",
    "BSD-2-Clause OR Apache-2.0 OR MIT": "MIT",
    "BSD-3-Clause OR Apache-2.0": "Apache-2.0",
    "MIT OR Apache-2.0 OR Zlib": "MIT",
    "MIT OR Zlib OR Apache-2.0": "MIT",
    "Zlib OR Apache-2.0 OR MIT": "MIT",
    "0BSD OR MIT OR Apache-2.0": "MIT",
    "Unlicense OR MIT": "MIT",
    "(MIT OR Apache-2.0) AND Unicode-3.0": "MIT AND Unicode-3.0",
}
FONT_EXPRESSION = "(MIT OR Apache-2.0) AND OFL-1.1 AND Ubuntu-font-1.0"


def selection(name, expression):
    if name == "epaint_default_fonts" and expression == FONT_EXPRESSION:
        return "MIT AND OFL-1.1 AND Ubuntu-font-1.0 (font assets require notices)"
    if expression not in CHOICES:
        raise ValueError(f"Review required: {name}: {expression!r}")
    return CHOICES[expression]


def parse_tree(text):
    packages = {}
    for line in text.splitlines():
        if not line.strip():
            continue
        package, expression = line.rsplit("|", 1)
        match = re.fullmatch(r"([\w-]+) v([^ ]+)(?: .*)?", package)
        if not match:
            raise ValueError(f"Unexpected cargo tree row: {line!r}")
        name, version = match.groups()
        expression = expression.removesuffix(" (*)").strip()
        if name in {"byakko", "byakko-core", "byakko-devices", "byakko-desktop", "byakko-cli"}:
            continue  # Project license remains the owner's decision.
        row = {"name": name, "version": version, "declared": expression,
               "selected": selection(name, expression)}
        key = (name, version)
        if key in packages and packages[key] != row:
            raise ValueError(f"Conflicting package metadata: {key}")
        packages[key] = row
    if not packages:
        raise ValueError("No dependencies found; audit cannot pass on empty input")
    return sorted(packages.values(), key=lambda row: (row["name"], row["version"]))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--package", default="byakko-desktop", help="Workspace application package to audit")
    parser.add_argument("--output", type=Path, help="Create a new JSON inventory; never overwrite")
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    inventory = {"package": args.package,
                 "scope": "default features, normal/build edges, current host build dependencies",
                 "limitations": "Metadata only; source headers and release notices need separate review",
                 "targets": {}}
    for target in TARGETS:
        result = subprocess.run(
            ["cargo", "tree", "-p", args.package, "--locked", "--offline", "--target", target,
             "--edges", "normal,build", "--prefix", "none", "--format", "{p}|{l}"],
            cwd=root, capture_output=True, text=True, check=True)
        rows = parse_tree(result.stdout)
        inventory["targets"][target] = rows
        print(f"{target}: {len(rows)} dependency packages; recognized license selections")
    if any(row["name"] == "epaint_default_fonts"
           for rows in inventory["targets"].values() for row in rows):
        print("Font assets retain OFL/Ubuntu font-license obligations.")
    print("Source audit is separate.")
    if args.output:
        with args.output.open("x", encoding="utf-8") as output:
            json.dump(inventory, output, indent=2)
            output.write("\n")


if __name__ == "__main__":
    main()

"""Inventory local license texts for the locked native release dependency graph.

This is an input to packaging review, not a license grant or a notice bundle.
It deliberately records source hashes and missing texts without guessing a
license from Cargo metadata or copying unrelated files into the product.
"""

import argparse
import hashlib
import json
import subprocess
from pathlib import Path

from check_dependency_licenses import TARGETS, parse_tree


PRODUCTS = ("byakko-desktop", "byakko-cli")
NOTICE_PREFIXES = ("license", "licence", "copying", "notice")


def command(args, root):
    return subprocess.check_output(args, cwd=root).decode("utf-8")


def packages_for(package, root):
    targets = {}
    for target in TARGETS:
        output = command(
            ["cargo", "tree", "-p", package, "--locked", "--offline", "--target", target,
             "--edges", "normal,build", "--prefix", "none", "--format", "{p}|{l}"],
            root,
        )
        targets[target] = parse_tree(output)
    return targets


def notice_files(source):
    return sorted(
        (path for path in source.iterdir()
         if path.is_file() and path.name.lower().startswith(NOTICE_PREFIXES)),
        key=lambda path: path.name.lower(),
    )


def source_row(row, metadata):
    key = (row["name"], row["version"])
    package = metadata[key]
    source = Path(package["manifest_path"]).parent
    if not package["source"] or not package["source"].startswith("registry+"):
        raise ValueError(f"Unexpected source for {key}: {package['source']}")
    texts = [
        {"name": path.name, "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}
        for path in notice_files(source)
    ]
    return {**row, "source": package["source"], "texts": texts,
            "missing_text": not texts}


def inventory(root):
    metadata = json.loads(command(
        ["cargo", "metadata", "--format-version", "1", "--locked", "--offline"], root,
    ))
    by_key = {(package["name"], package["version"]): package
              for package in metadata["packages"]}
    graphs = {product: packages_for(product, root) for product in PRODUCTS}
    selected = {}
    for product, targets in graphs.items():
        for target, rows in targets.items():
            for row in rows:
                key = (row["name"], row["version"])
                if key in selected and selected[key]["selected"] != row["selected"]:
                    raise ValueError(f"Conflicting license choices for {key}")
                selected[key] = row
    rows = [source_row(selected[key], by_key) for key in sorted(selected)]
    return {
        "scope": "Locked offline normal/build graphs; top-level notice-like source files only",
        "products": {product: {target: [[row["name"], row["version"]] for row in rows]
                              for target, rows in targets.items()}
                     for product, targets in graphs.items()},
        "packages": rows,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path,
                        help="Create a new JSON inventory; never overwrite")
    args = parser.parse_args()
    result = inventory(Path(__file__).resolve().parent.parent)
    with args.output.open("x", encoding="utf-8") as output:
        json.dump(result, output, indent=2, ensure_ascii=False)
        output.write("\n")
    missing = [f"{row['name']} {row['version']}" for row in result["packages"]
               if row["missing_text"]]
    print(f"{len(result['packages'])} selected package versions; "
          f"{len(missing)} without a top-level license/notice text")
    for package in missing:
        print(f"  {package}")
    print(f"Inventory: {args.output}")


if __name__ == "__main__":
    main()

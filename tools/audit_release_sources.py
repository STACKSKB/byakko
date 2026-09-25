"""Inventory local license texts for the locked native release dependency graph.

This is an input to packaging review, not a license grant or a notice bundle.
It deliberately records source hashes and missing texts without guessing a
license from Cargo metadata or copying unrelated files into the product.
"""

import argparse
import hashlib
import json
import subprocess
import tomllib
from pathlib import Path

from check_dependency_licenses import TARGETS, parse_tree


PRODUCTS = ("byakko-desktop", "byakko-cli")
NOTICE_PREFIXES = ("license", "licence", "copying", "notice")
VENDORED_PACKAGES = {
    ("iced_tiny_skia", "0.14.1"): "vendor/iced_tiny_skia",
}


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


def file_hash(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def vendored_source_row(row, package, root, expected_path):
    source = Path(package["manifest_path"]).parent.resolve()
    approved = (root / expected_path).resolve()
    if source != approved:
        raise ValueError(f"Unexpected vendored path for {(row['name'], row['version'])}: {source}")
    manifest = source / "Cargo.toml"
    patch = source / "BYAKKO-PATCH.md"
    license_file = source / "LICENSE"
    if not manifest.is_file() or not patch.is_file() or not license_file.is_file():
        raise ValueError(f"Vendored package is missing required provenance/license files: {source}")
    with manifest.open("rb") as handle:
        identity = tomllib.load(handle)["package"]
    if (identity.get("name"), identity.get("version"), identity.get("license")) != (
        row["name"], row["version"], "MIT"
    ):
        raise ValueError(f"Vendored package manifest identity/license mismatch: {manifest}")
    files = [
        {"path": path.relative_to(source).as_posix(), "sha256": file_hash(path)}
        for path in sorted(source.rglob("*")) if path.is_file()
    ]
    texts = [
        {"name": path.name, "sha256": file_hash(path)}
        for path in notice_files(source)
    ]
    return {
        **row,
        "source": "vendored",
        "path": expected_path,
        "texts": texts,
        "missing_text": not texts,
        "provenance": {
            "record": "BYAKKO-PATCH.md",
            "record_sha256": file_hash(patch),
            "files": files,
        },
    }


def source_row(row, metadata, root=None):
    key = (row["name"], row["version"])
    package = metadata[key]
    if key in VENDORED_PACKAGES:
        if root is None:
            raise ValueError(f"Repository root is required to validate vendored package {key}")
        if package["source"] is not None:
            raise ValueError(f"Unexpected Cargo source for vendored package {key}: {package['source']}")
        return vendored_source_row(row, package, Path(root), VENDORED_PACKAGES[key])
    source = Path(package["manifest_path"]).parent
    if not package["source"] or not package["source"].startswith("registry+"):
        raise ValueError(f"Unexpected source for {key}: {package['source']}")
    texts = [{"name": path.name, "sha256": file_hash(path)} for path in notice_files(source)]
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
    rows = [source_row(selected[key], by_key, root) for key in sorted(selected)]
    return {
        "scope": "Locked offline normal/build graphs; root notices plus approved vendored source hashes",
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

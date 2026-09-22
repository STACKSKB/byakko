# Target dependency source-license audit

Date: 2026-09-22. This is a local source and metadata check, not legal clearance or a distribution notice. Commands used `cargo tree --locked --offline --target x86_64-pc-windows-msvc --edges normal,build` and the corresponding `x86_64-unknown-linux-gnu` target, with unique package/version entries (including proc macros). The Windows graph has **124** entries and the Linux graph **210**. These counts include build dependencies; they are not runtime-only counts or the platform-wide lockfile count.

In both selected graphs, the only package metadata with a GPL/LGPL/AGPL/MPL term is `self_cell 1.3.0`, declared `Apache-2.0 OR GPL-2.0-only`. The Apache choice is available; that expression does not force the GPL option. No selected package metadata declares a copyleft-only license.

I searched the local crates.io registry `.rs` files, including `build.rs`, for SPDX copyleft headers and explicit GNU/Mozilla license statements. The only concrete differing source header found was `hidapi 2.6.7/build.rs`, whose header states GPLv3-or-later even though that crate's package metadata is MIT. `hidapi` is **not in either current selected target graph**; the project migrated to its own HID adapters. This remains a historical cache/source finding, not selected compiled or build-script code. No GPL/LGPL/AGPL/MPL source header was found in the selected Rust source or build scripts by that scan.

The scan deliberately distinguishes license statements from words such as `impl`, from tests/doc examples, and from license-text files. It does not establish the licensing of every bundled asset, generated file, transitive native library, or final distribution. Before release, repeat against the exact locked source archive and build artifacts, review permissive-license notice obligations, and have the chosen licensing interpretation reviewed by the release owner.

## Repeatable metadata check

Run `python tools/check_dependency_licenses.py` from any directory. It uses the
locked, offline default-feature graphs for both targets, including normal and
build edges. Unknown or unapproved expressions fail for review. It explicitly
selects Apache-2.0 for `self_cell`; it does not accept arbitrary expressions just
because they contain the word MIT. The counts exclude Byakko itself: **123**
Windows dependencies and **209** Linux dependencies, corresponding to the graph
counts above including the project. Cross-target build dependencies reflect the
current build host; rerun on the actual release host.

Use `--output NEW_PATH.json` to create a new inventory without overwriting an
existing file. `python -m unittest discover -s tools -p test_dependency_licenses.py`
tests rejection of copyleft-only/unknown expressions and the asset exception.

The sole font exception is scoped to `epaint_default_fonts`: its Rust code has
a permissive choice, while its bundled fonts retain OFL-1.1 and Ubuntu-font-1.0
obligations. This checker does not produce the notices needed to distribute
those fonts or other dependencies. The supplied font notices and embedded
copyright attributions are now collected in
`packaging/notices/epaint_default_fonts-0.36.2`, with source hashes. Release
packaging still needs to include them and collect the other dependency notices.
The checker supplements, and does not replace, the
source-header audit that identified the historical HIDAPI discrepancy.

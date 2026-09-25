# Target dependency source-license audit

## 2026-09-25 update

The project is GPL-3.0-or-later; third-party notices remain intact. The local
`iced_tiny_skia 0.14.1` patch builds from tracked Git files. The source inventory
now explicitly supports that vendor directory, records its MIT license and
hashes the source and `BYAKKO-PATCH.md` provenance record. Unreviewed local
sources still require a separate audit decision. Python 3.11+ is required by
the inventory tool's TOML parser.

The full inventory succeeds with 182 selected dependency versions; 10 (rather
than 11) lack root texts in their crate archives because the renderer's vendored
LICENSE is now included. Existing supplemental notices cover those ten.
The historical records below describe the earlier registry-only inventory.

## Current Iced desktop metadata gate (2026-09-23)

`python tools/check_dependency_licenses.py` (defaulting to `byakko-desktop`)
passes for the locked, offline Windows MSVC and Linux GNU normal/build graphs:
126 and 175 third-party package/version entries respectively. The matching
five checker unit tests pass. No selected package metadata requires a
copyleft-only license. `self_cell 1.3.0` offers Apache-2.0 in addition to GPL,
and the Iced desktop graph has no legacy eframe or bundled
`epaint_default_fonts` asset. Its dependency choices include MIT, Apache-2.0,
BSD, Zlib, BSL, ISC, and Unicode-3.0 obligations; this is a metadata result,
not a complete source-header, native-library, or distribution-notice audit.
The release bundle must include required third-party license and attribution
texts. Byakko-owned material is now GPL-3.0-or-later (2026-09-25); dependency
licenses and notices remain unchanged.

The independent `byakko-cli` graph also passes the same gate with
`--package byakko-cli`: 13 Windows and 12 Linux third-party package/version
entries. The checker excludes Byakko's own workspace crates from both counts.

## Offline source-text inventory (2026-09-23)

`tools/audit_release_sources.py --output NEW_PATH.json` joins the two locked,
target-filtered native application graphs to the local Cargo source cache. It
records the reviewed metadata choice, source registry, and SHA-256 of each
top-level license/notice-like file. Output creation is exclusive. The ignored
local result is `Research/captures/release-source-inventory-20260923.json`.

The Windows/Linux desktop and CLI graphs contain 182 distinct third-party
package/version entries in union. Of those, 171 have a top-level license or
notice text in the extracted crate; 11 have none: `clipboard-win 5.4.1` and
`iced_core`, `iced_debug`, `iced_futures`, `iced_graphics`, `iced_program`,
`iced_renderer`, `iced_runtime`, `iced_tiny_skia`, `iced_widget`, and
`iced_winit` at their locked 0.14.x versions. A metadata license expression
alone does not supply their notice text.

The eleven missing repository-root texts were obtained from their exact
upstream source commits on 2026-09-23 and are stored under
`packaging/notices/iced-0.14` and `packaging/notices/clipboard-win-5.4.1`.
The ten Iced crates share one MIT license whose bytes match at each of four
recorded source revisions; `clipboard-win` has its own BSL-1.0 text. Each
directory records the covered package versions, upstream revisions, URL and
SHA-256. The inventory still reports these eleven as missing *from the local
crate archives*. The supplemental files resolve their notice-text provenance,
not the remaining packaging and asset review.

This inventory does not choose which of a crate's alternate license files to
ship, inspect nested attribution or bundled native libraries, or grant a
license to Byakko itself. Preserve compound obligations such as Unicode-3.0
when assembling release notices. Byakko's own license is GPL-3.0-or-later; final release packaging remains open.

The historical audit below describes the retained root research package,
which has a different dependency graph. Do not use its counts or font notices
as a desktop release inventory.

Date: 2026-09-22. This is a local source and metadata check, not legal clearance or a distribution notice. Commands used `cargo tree --locked --offline --target x86_64-pc-windows-msvc --edges normal,build` and the corresponding `x86_64-unknown-linux-gnu` target, with unique package/version entries (including proc macros). The Windows graph has **124** entries and the Linux graph **210**. These counts include build dependencies; they are not runtime-only counts or the platform-wide lockfile count.

In both selected graphs, the only package metadata with a GPL/LGPL/AGPL/MPL term is `self_cell 1.3.0`, declared `Apache-2.0 OR GPL-2.0-only`. The Apache choice is available; that expression does not force the GPL option. No selected package metadata declares a copyleft-only license.

I searched the local crates.io registry `.rs` files, including `build.rs`, for SPDX copyleft headers and explicit GNU/Mozilla license statements. The only concrete differing source header found was `hidapi 2.6.7/build.rs`, whose header states GPLv3-or-later even though that crate's package metadata is MIT. `hidapi` is **not in either current selected target graph**; the project migrated to its own HID adapters. This remains a historical cache/source finding, not selected compiled or build-script code. No GPL/LGPL/AGPL/MPL source header was found in the selected Rust source or build scripts by that scan.

The scan deliberately distinguishes license statements from words such as `impl`, from tests/doc examples, and from license-text files. It does not establish the licensing of every bundled asset, generated file, transitive native library, or final distribution. Before release, repeat against the exact locked source archive and build artifacts, review permissive-license notice obligations, and have the chosen licensing interpretation reviewed by the release owner.

## Repeatable metadata check

For the retained root research package, run
`python tools/check_dependency_licenses.py --package byakko`. The checker uses
locked, offline default-feature graphs for both targets, including normal and
build edges. Unknown or unapproved expressions fail for review. It explicitly
selects Apache-2.0 for `self_cell`; it does not accept arbitrary expressions just
because they contain the word MIT. The historical root counts exclude Byakko
itself: **123** Windows dependencies and **209** Linux dependencies, corresponding
to the graph counts above including the project. Cross-target build dependencies
reflect the current build host; rerun on the actual release host.

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

## Current RustSec check (2026-09-25)

A temporary `cargo-audit 0.22.2` installation checked all 430 dependencies in
Cargo.lock against RustSec database commit
`593df8c1b5ed0bcde9dddadfeeead776fa514ff8`, updated 2026-09-24. No advisories
were ignored. The result reported zero known vulnerabilities and one
informational unmaintained-package warning:
[`RUSTSEC-2026-0192`, ttf-parser 0.25.1](https://rustsec.org/advisories/RUSTSEC-2026-0192.html).
The advisory lists no patched version; it is not a reported vulnerability.

The Linux Iced graph reaches it through `fontdb → cosmic-text` and
`owned_ttf_parser → ab_glyph → sctk-adwaita → winit`. Replacing it would need
upstream integration, not an isolated version bump. This is recorded as a
maintenance follow-up; no dependency, lockfile or renderer changes were made
for the informational warning. The check does not audit local Byakko or
vendored patch correctness.

Local JSON result: `/tmp/byakko-audit-current.json`. The temporary audit tool
is outside the checkout; building the product does not require it.

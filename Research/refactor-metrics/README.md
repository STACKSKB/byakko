# Refactor measurements — 2026-09-25

Reproduce from `/home/three/code/Byakko` with cached dependencies only:

```sh
cargo build --offline --manifest-path Research/refactor-metrics/ast/Cargo.toml --quiet
python3 Research/refactor-metrics/decisions.py 09f193c 264102c WORKTREE
```

The Rust visitor is `Research/refactor-metrics/ast/src/main.rs`; the runner is `Research/refactor-metrics/decisions.py`; full per-file results with 12-character SHA-256 source hashes are `Research/refactor-metrics/decisions.txt`. The visitor uses cached `syn` 2.0.119 with `full` and `visit` features; it does not install or download anything. It parses committed blobs through `git show` and current files through the working tree. The working-tree results below reflect the source hashes in the results file and must be rerun if edits continue.

**Counting convention:** one syntax decision for each `if` (including `if let`/`else if`), each `match` arm beyond the first, each match guard, each `&&`/`||`, each `for`/`while`/`loop`, each `?`, and each `let ... else`. The visitor traverses closures and function bodies and counts all non-test `cfg` branches. It skips test paths and `#[cfg(test)]` items/modules. Macro token streams, including `matches!`, are opaque; the tool counts macro boundaries separately. It does not expand macros, evaluate conditions, construct a control-flow graph, count runtime paths in combinators such as `Option::map`, or assign a base complexity of 1 per function. **This is a documented AST decision-syntax score, not conventional cyclomatic complexity.**

Selected production scope: all `crates/byakko-core/src/session.rs` and `session/*.rs`; `crates/byakko-devices/src/executor.rs` and `executor/*.rs`; `crates/byakko-devices/src/nia87/device.rs` and `device/*.rs`, excluding test paths/modules. This includes unchanged files in those modules and the new transaction file. It excludes desktop code and other device modules. Counts are summed over 21 files at `09f193c` and 22 at later revisions.

| Scope | 09f193c | 264102c | WORKTREE | Baseline → worktree |
|---|---:|---:|---:|---:|
| Core session | 279 | 258 | 251 | -28 |
| Executor | 97 | 101 | 79 | -18 |
| Nia87 device feature modules | 274 | 258 | 246 | -28 |
| **Total syntax decisions** | **650** | **617** | **576** | **-74 (-11.4%)** |
| Total excluding `?` | 397 | 378 | 349 | -48 |
| Functions visited | 238 | 254 | 262 | +24 |
| Closures visited | 129 | 144 | 173 | +44 |
| Opaque macro boundaries | 70 | 63 | 64 | -6 |

The score fell by 33 in `264102c` relative to `09f193c` and by another 41 in the current working tree. The added shared executor dispatcher accounts for most of the latter: executor scope fell from 101 to 79 despite the transaction helper's local increase from 8 to 16. The current session score is 251 versus 279 at baseline; the device-feature score is 246 versus 274. The breakdown of syntax kinds in the full results shows `match` alternatives falling 133 → 120 → 95 and `?` falling 253 → 239 → 227 across the selected scope. Functions and closures increase, so a lower aggregate score does not establish simpler local functions or lower cognitive load.

No CFG-based Rust complexity tool was used, and this AST convention does not prove conventional CC in either direction. The evidence supports a narrower statement: across the affected production modules, the counted explicit decision syntax declined, while production physical LOC did not meaningfully fall and abstraction count rose.

Validation fixtures for the visitor: a function containing `if`, a three-arm `match` with one guard, `?`, and a `while` with `&&` scores 7 under this convention; a `#[cfg(test)]` module is skipped; a standalone `matches!` statement scores zero decisions and one opaque macro boundary. These were run against the built binary.

## Production line counts

The initial completion summary overstated the structural cleanup. Hoisting the
envelope did remove repeated correlation fields, but separate dispatch tables
and repeated verification sequences remained. The second revision addresses
those directly; it still does not produce a large net source reduction.

Run `python3 Research/refactor-metrics/lines.py 09f193c 264102c WORKTREE`
from the repository root. The tool reports physical lines and a lexical proxy
that omits blanks and lines beginning with comment markers. It is not
parser-based SLOC. Inline test modules and external tests, examples, docs and
this measurement tool are reported separately. Both new helpers and existing
callers count toward production totals.

| Production Rust | Before initial refactor | Initial refactor | Revised |
|---|---:|---:|---:|
| Physical lines | 35,283 | 35,313 | 35,289 |
| Nonblank/noncomment proxy | 32,322 | 32,297 | 32,254 |

The revised change removes **43 proxy code lines / 24 physical lines** versus
the initial refactor. Across both revisions, proxy code lines fall by 68,
while physical production lines rise by 6. Test growth is not included in
these numbers and is not evidence of structural simplification.

## Structural changes in the revision

- Feature commands/results encode Read/Apply once, reducing each top-level
  payload enum and session completion dispatch from 14 variants/arms to 8.
- Execution, rejected commands and panic completion now use one feature
  dispatcher. Separate exhaustive execution/failure tables and the repeated
  failed-write classification match are gone.
- Session request creation allocates the operation ticket, records activity
  and constructs the envelope together.
- Macro, settings and verified-lighting transactions share the actual
  write/read/compare and restore/read/compare sequence. The helper delegates
  recovery classification to the existing recovery shell. Keymap's special
  restoration order and archive's multi-section semantics remain explicit.
- Frontend completion classification no longer repeats Read/Apply pairs just
  to identify a feature. Macro assignment uses envelope correlation directly.

The source-contract JSON changes again to nested feature Read/Apply payloads;
all in-tree consumers are migrated together. Persisted snapshots, native
backups and portable macro documents are unchanged.

## Validation and limits

Workspace tests, strict Clippy including research targets, native release
builds and Windows cross-target checks passed. No physical writes were run.
The macro EPROTO readback / unavailable recovery and genuine library scan
failures remain unresolved hardware acceptance gates. This structural work
must not be described as proving that those failures are fixed.

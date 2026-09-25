#!/usr/bin/env python3
"""Offline syn AST decision-syntax audit of Byakko session/executor/device feature modules."""
import hashlib
from pathlib import Path
import subprocess
import sys

REVISIONS = sys.argv[1:] or ["09f193c", "264102c", "WORKTREE"]
BIN = str(Path(__file__).resolve().parent / "ast/target/debug/byakko_ast_audit")
FIELDS = ("decisions", "functions", "closures", "if", "match", "arms", "alternatives", "guards", "shortcircuit", "for", "while", "loop", "try", "let_else", "macro_boundaries", "skipped_test_items")

def git(*args):
    return subprocess.check_output(("git", *args), text=True)

def paths(rev):
    names = (git("ls-files", "--cached", "--others", "--exclude-standard").splitlines()
             if rev == "WORKTREE" else git("ls-tree", "-r", "--name-only", rev).splitlines())
    out = []
    for p in names:
        if not p.endswith(".rs") or "/tests/" in p or p.endswith(("/tests.rs", "_tests.rs")):
            continue
        if p == "crates/byakko-core/src/session.rs" or p.startswith("crates/byakko-core/src/session/"):
            group = "core_session"
        elif p == "crates/byakko-devices/src/executor.rs" or p.startswith("crates/byakko-devices/src/executor/"):
            group = "executor"
        elif p == "crates/byakko-devices/src/nia87/device.rs" or p.startswith("crates/byakko-devices/src/nia87/device/"):
            group = "device_features"
        else:
            continue
        if rev != "WORKTREE" or Path(p).is_file():
            out.append((p, group))
    return out

def source(rev, path):
    return Path(path).read_bytes() if rev == "WORKTREE" else subprocess.check_output(("git", "show", f"{rev}:{path}"))

def measure(rev):
    rows = {}
    for path, group in paths(rev):
        data = source(rev, path)
        result = subprocess.run((BIN,), input=data, capture_output=True, check=False)
        if result.returncode:
            raise RuntimeError(f"parse error {rev}:{path}: {result.stderr.decode()}")
        counts = dict(zip(FIELDS, map(int, result.stdout.decode().split()), strict=True))
        rows[path] = (group, counts, hashlib.sha256(data).hexdigest()[:12])
    return rows

def total(rows, group):
    selected = [counts for g, counts, _ in rows.values() if group == "all" or g == group]
    return {field: sum(row[field] for row in selected) for field in FIELDS}

runs = [(rev, measure(rev)) for rev in REVISIONS]
print("Convention: decision syntax = if + match arms beyond first + guards + &&/|| + for/while/loop + ? + let-else. This is not CFG cyclomatic complexity. Macro tokens are opaque and counted separately. Test paths and #[cfg(test)] items are skipped. All other cfg branches are included.")
print("Fields: " + ", ".join(FIELDS))
for group in ("core_session", "executor", "device_features", "all"):
    print("\n" + group)
    previous = None
    for rev, rows in runs:
        counts = total(rows, group)
        line = ", ".join(f"{f}={counts[f]}" for f in FIELDS)
        delta = "" if previous is None else f"  delta_decisions={counts['decisions']-previous['decisions']:+}"
        print(f"  {rev}: files={len([1 for g, _, _ in rows.values() if group == 'all' or g == group])}; {line}{delta}")
        previous = counts
print("\nPer-file decision counts and source hashes:")
for path in sorted({p for _, rows in runs for p in rows}):
    items = []
    for rev, rows in runs:
        if path in rows:
            _, counts, digest = rows[path]
            items.append(f"{rev}={counts['decisions']}({digest})")
        else:
            items.append(f"{rev}=absent")
    print(path, " ".join(items))

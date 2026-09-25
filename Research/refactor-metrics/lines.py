#!/usr/bin/env python3
"""Read-only physical/code-line audit for two Byakko revisions; no dependencies."""
import re
import subprocess
import sys
from collections import defaultdict
from pathlib import Path

REVISIONS = sys.argv[1:] or ["09f193c", "264102c", "WORKTREE"]

def git(*args):
    return subprocess.check_output(("git", *args), text=True)

def paths(rev):
    if rev == "WORKTREE":
        names = git("ls-files", "--cached", "--others", "--exclude-standard").splitlines()
        return [p for p in names if (p.endswith(".rs") or p.endswith(".md")) and Path(p).is_file()]
    return [p for p in git("ls-tree", "-r", "--name-only", rev).splitlines()
            if p.endswith(".rs") or p.endswith(".md")]

def bucket(path, line_no, test_start):
    if path.startswith("Research/refactor-metrics/") and path.endswith(".rs"):
        return "measurement_tool"
    if path.endswith(".md"):
        return "docs"
    if path.startswith("examples/") or "/examples/" in path:
        return "examples"
    if ("/tests/" in path or path.endswith("/tests.rs") or
            path.endswith("_tests.rs") or test_start is not None and line_no >= test_start):
        return "tests"
    return "production"

def test_split(lines):
    for i, line in enumerate(lines):
        if line.strip() == "#[cfg(test)]":
            next_lines = " ".join(lines[i + 1:i + 3])
            if re.search(r"\bmod\s+tests\s*\{", next_lines):
                return i + 1
    return None

def code_line(line):
    stripped = line.strip()
    return bool(stripped and not stripped.startswith(("//", "/*", "*", "*/")))

def audit(rev):
    counts = defaultdict(lambda: [0, 0])
    files = {}
    for path in paths(rev):
        lines = (Path(path).read_text().splitlines() if rev == "WORKTREE"
                 else git("show", f"{rev}:{path}").splitlines())
        split = test_split(lines) if path.endswith(".rs") else None
        per_file = defaultdict(lambda: [0, 0])
        for n, line in enumerate(lines, 1):
            group = bucket(path, n, split)
            per_file[group][0] += 1
            per_file[group][1] += code_line(line)
            counts[group][0] += 1
            counts[group][1] += code_line(line)
        files[path] = per_file
    return counts, files

audits = [(rev, *audit(rev)) for rev in REVISIONS]
print("Revisions:", " -> ".join(REVISIONS))
print("Definitions: physical lines include blanks/comments; lexical code lines omit blank lines and lines beginning //, /*, *, or */. Inline #[cfg(test)] mod tests { to EOF is tests. This is not parser-based SLOC.")
print("\nWhole-repository tracked .rs/.md lines:")
for group in ("production", "tests", "examples", "docs", "measurement_tool"):
    print(group)
    previous = None
    for rev, counts, _ in audits:
        item = counts[group]
        delta = "" if previous is None else f" (physical {item[0]-previous[0]:+}, lexical {item[1]-previous[1]:+} from prior)"
        print(f"  {rev:10} physical {item[0]:6}, lexical {item[1]:6}{delta}")
        previous = item
for (a_rev, _, a_files), (b_rev, _, b_files) in zip(audits, audits[1:]):
    print(f"\nChanged paths, class-specific physical/lexical net: {a_rev} -> {b_rev}")
    for path in sorted(set(a_files) | set(b_files)):
        a, b = a_files.get(path, {}), b_files.get(path, {})
        groups = sorted(set(a) | set(b))
        diffs = [(g, b.get(g, [0,0])[0]-a.get(g, [0,0])[0], b.get(g, [0,0])[1]-a.get(g, [0,0])[1]) for g in groups]
        if any(x or y for _, x, y in diffs):
            print(path, ", ".join(f"{g} {x:+}/{y:+}" for g, x, y in diffs if x or y))

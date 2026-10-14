#!/usr/bin/env python3
"""Sum cargo-llvm-cov's per-file line coverage by crate.

Usage: coverage-by-crate.py coverage.json   (from `cargo llvm-cov report --json --summary-only`)

Prints `crate lines covered percent`, lowest percent first. A crate is the first
three path segments of `crates/<layer>/<name>/...`; a file outside `crates/` is
grouped under its first segment.
"""
import json
import sys


def crate_of(filename: str) -> str:
    parts = filename.split("/")
    if "crates" in parts:
        at = parts.index("crates")
        return "/".join(parts[at : at + 3])
    return parts[-1] if len(parts) == 1 else "/".join(parts[-2:-1])


def main(path: str) -> None:
    with open(path) as f:
        files = json.load(f)["data"][0]["files"]
    totals: dict[str, list[int]] = {}
    for entry in files:
        lines = entry["summary"]["lines"]
        row = totals.setdefault(crate_of(entry["filename"]), [0, 0])
        row[0] += lines["count"]
        row[1] += lines["covered"]
    rows = sorted(
        ((name, n, c, 100.0 * c / n if n else 100.0) for name, (n, c) in totals.items()),
        key=lambda r: r[3],
    )
    print(f"{'crate':<40} {'lines':>8} {'covered':>8} {'percent':>8}")
    for name, n, c, pct in rows:
        print(f"{name:<40} {n:>8} {c:>8} {pct:>7.1f}%")


if __name__ == "__main__":
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    main(sys.argv[1])

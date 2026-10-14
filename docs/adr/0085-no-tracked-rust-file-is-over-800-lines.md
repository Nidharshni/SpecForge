# No tracked Rust file is over 800 lines; the file-size baseline only shrinks

**Status:** accepted (2026-10-14, structure round, plan 05). Extends [ADR 0066](0066-one-check-list.md) (one check
list): `gate/baseline.json` becomes one section per check.

Nothing stopped a file from growing. At the round-9 merge, 87 tracked `.rs` files were over 800 lines, against 62
six rounds earlier: the refactoring rounds split some and others grew past them, most of them test files. The
largest was a 3,327-line test file. The structure round's splits ([ADR 0087](0087-a-file-over-the-cap-becomes-a-directory-module-along-its-own-seams.md))
then took every source file under 800. What remains are 53 crate test files. A file's size alone is not a defect, but
past a few hundred lines a reader (human or agent) can no longer hold it, and its seams stop being visible.

## D1. The cap

No tracked `.rs` file (`git ls-files '*.rs'`: crates, their tests, the guests, fixtures, gate, xtask) is over 800
lines, counted as `wc -l` counts them: blank lines, comments and inline test modules included. 800 sits above the
90th percentile of source files (661) at the time; a file over it whose own code is under it needs only its test
module moved out.

## D2. The baseline only shrinks

`gate/baseline.json`'s `file-size` section lists each file over the cap at its size, seeded on the split tree. The
`file-size` check (a hook check) fails when a file over the cap is not listed, when a listed file grew, and when one
shrank, fell under the cap or is gone, until its row is lowered or dropped in the same commit (the rule the `specs`
section already had). `cargo gate baseline --only file-size` lowers and drops rows and never adds a file.

## D3. A move carries its row

When a listed file is renamed (git's rename detection, from `HEAD` or `--since <rev>` to the working tree), the
writer passes its row to the new path at the new size, never a larger one. A split's row is dropped when every part
is under the cap, or carried to the one part git pairs with the old file. No tool raises a row; a raise is a hand
edit, reviewed in the diff with its reason.

## D4. One baseline file, one section per check

`gate/baseline.json` holds `{"file-size": {path: lines}, "specs": {project: {code: count}}}`, read and written by
one module (`gate::baseline`), unknown sections refused. `cargo gate baseline [--only <check>]` lowers every
section or the one named.

## Rejected

- A count of offenders: a new offender could replace a fixed one.
- Excluding inline test modules from the count: a 3,191-line file would pass with 2,316 lines of tests, and test
  files would have no limit.
- Source files only: `tests.rs` files and the guests' sources would be unlimited; the largest files were tests.
- A separate cap for tests: a second number and a classification to keep right.
- An explicit `--carry old=new`: git already knows the move; a `--seed` or `--raise` mode: a permanent bypass.
- Capping `.spec` files: they grow with the specification, one entity per block.

## Consequences

- Adding to a listed file is refused: new code or tests go in a new module (for a test binary,
  `tests/<file>/<topic>.rs`), or the file is split first. A lint edit that rewraps lines in a listed file is refused
  too.
- A commit that shrinks a listed file runs `cargo gate baseline --only file-size` (no build) and commits the row.
- Merging a branch whose moves are committed: `cargo gate baseline --only file-size --since HEAD^1`.

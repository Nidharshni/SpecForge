# Text a user types is tested by properties over the repository's corpus

**Status:** accepted (2026-10-14, structure round, plan 11).

At `0113232f` the parser and formatter had 245 tests, all over fixed strings or the valid corpus. The formatter's
"random valid inputs" test formatted 8 fixed strings. The parser's fuzz target had never been buildable: it was
neither a workspace member nor excluded. Two minutes of seeded edits of the 235 tracked `.spec` files (delete, insert
a token, duplicate a line, truncate) found three bugs. The parser panicked on a long error line holding a multi-byte
character (`specforge check` exited 101). `specforge format` wrote a line once per broken form that shared it, and
doubled it on every run. The formatter stripped trailing whitespace inside strings, changing field values.

## Decision

- **D1. Properties over the corpus.** `specforge_parser::testing` (feature `testing`) holds the corpus (the tracked
  `.spec` files under fixed roots), the edits a user could type, and the run configuration. The parser and the
  formatter each assert their properties over them in every `cargo nextest` run: parsing yields a file; a reparse
  reads as a cold parse; formatting reaches a fixed point and writes no source line twice; a valid edit keeps its
  entities; a relaid file formats as the file does.
- **D2. proptest, reproducibly.** A fixed seed, 256 cases (`PROPTEST_CASES` raises it for a deep run), no regression
  files in the tree. A counterexample a deep run finds becomes a named regression test linked to the obligation it
  broke.
- **D3. No fuzz crate.** The properties check what the fuzz target checked, on stable, in the gate.
- **D4. Line coverage is measured, not gated.** A non-blocking workflow on `main` reports it per file and crate. The
  spec coverage of the project's own spec (`specforge analyze coverage`) is recorded beside it each round.

## Consequences

- A change to the parser or formatter is tested against about 1,300 edited files per run, not only the inputs its
  author thought of.
- A property failure prints a shrunk input; reproducing it needs no fuzzer.
- Coverage numbers inform each round's test plan; no refactor fails on them.
- `specforge format` output changes where it was wrong: a string literal keeps its trailing whitespace, an
  expression block's lines are indented one level inside it, and a form that shares a line with a broken form is kept
  with it, each line written once.

## Rejected

- **Repairing the fuzz crate with a nightly workflow:** a second toolchain and workflow for a target the properties
  subsume.
- **A coverage percentage or ratchet gate:** the bugs above were in covered lines. What was missing was inputs, and a
  percentage gate fails on moves.
- **Hand-rolled seeded RNGs** (as `crates/session/project/tests/session.rs`): no shrinking, and the corpus edits make
  large inputs.

## What would reopen it

The properties stop finding bugs while users still report parser or formatter crashes: then coverage-guided fuzzing
earns a nightly job.

# A test binary records; collect scores

**Status:** accepted (2026-10-13, architecture round 8, plan 06). Follows ADR 0014 D3 (precedent: it removed a similar side
channel) and ADR 0019 (the coverage rule's standing).

`specforge-graph` had a build script that parsed this repository's `spec/` with the raw parser, decided which entities
were testable by a rule of its own (a `verify` statement or a `gherkin` field), and wrote `graph.json` into the target
directory, or into the source tree when the target directory's path had no `target` component. It printed
`cargo::rerun-if-changed=<absolute>/spec`, so any `.spec` edit recompiled `specforge-graph` and the ten workspace
crates above it (40 s for the CLI), and two checkouts sharing a target directory shared one unit that watched whichever
had built it last. Its only reader was `specforge-test`'s exit handler, which scored the binary's records against it
with a second coverage rule and printed a table. No user project ever had that file: the documented way to make it
(`specforge export --format=graph --output …`) is not a command, and `export`'s graph has another shape. In this
repository the table showed one test binary's slice of the proof, so obligations other binaries prove read as gaps.
Keeping its rule in step with the host's took three fixes (86adafc9, 98f2266c, 9dd019ed). ADR 0014 D3 removed a
similar side channel (`.specforge/graph.json`).

## Decisions

- **D1. No build script reads `spec/`.** `specforge-graph/build.rs` and its build-dependencies are gone. The graph is
  exported by `specforge export`.
- **D2. The exit handler writes the binary report and nothing else.** It reads no graph, prints nothing (but a failed
  write) and keeps no side file: the per-test `.jsonl` lines, which nothing read, are gone. Which obligations the
  recorded tests prove is scored once, by the coverage rule (`specforge-coverage`) over the recorded test report, with
  the entity snapshot's standing (ADR 0019): `specforge collect`, then `specforge analyze coverage`. Coverage is a fact
  about the project; one test binary cannot know it.
- **D3. The binary report schema describes what the writer writes:** `module_path` is in it, `verify_kind` (never
  written) is not. `schema_version` stays `1.0`: no report's bytes change.
- **D4. One slug algorithm, on the host** (`specforge_common::slug`). `specforge-test` links by the attribute's exact
  text and never needs one.
- **D5. A report is build output.** Without `$SPECFORGE_REPORT` it goes to `specforge/` in the cargo target directory
  the binary was built into, the nearest ancestor holding `CACHEDIR.TAG` or `.rustc_info.json`. A binary outside any
  target directory writes none and says so; nothing is ever written relative to the working directory. With a custom
  target directory, `specforge collect --report "$CARGO_TARGET_DIR/specforge"` reads what a plain `cargo test` wrote.

## Consequences

- `cargo test` prints no SpecForge table; `specforge collect` runs the tests and records them, and `analyze coverage`
  reports the gaps (as the Rust test tracing guide always said).
- Editing a `.spec` file rebuilds no Rust crate.
- `specforge-test` loses `coverage`, `ports` and `slugify`, and `TestRecordEntry::verify_kind`.
- A test target whose process dies before exit leaves no report; its tests reach collect only through libtest's output.

## What would reopen this

- A need for coverage feedback during `cargo test` that `specforge collect` cannot give; the answer would be a collect
  mode, not a second rule in the test crate.
- A runner whose results can only be recovered from per-test lines (then the collector, not the test crate, decides
  what it reads).

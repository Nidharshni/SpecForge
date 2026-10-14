# A file over the size cap becomes a directory module along its own seams

**Status:** accepted (2026-10-14, structure round, plans 07 and 08). Applies the cap of ADR 0085 (no tracked `.rs`
file is over 800 lines).

The round found 87 files over 800 lines. Some were long because of an inline test module, some held several
concerns their own section comments named, and a few were one long function. A split by line count alone would
scatter code that belongs together. A split along the code's seams makes each file one concern. Test files are held
to the cap too (the user's decision of 2026-10-14): an inline `mod tests`, a `tests.rs` and a `tests/` directory all
count.

## Decision

- **D1. The parent keeps its path.** A file `foo.rs` that splits keeps its path and gains a sibling directory
  `foo/` holding its children (`foo/bar.rs`). A crate root's children are `src/<child>.rs`. New directory modules
  are not written as `foo/mod.rs`.
- **D2. Children are private; the parent is the interface.** Children are declared `mod bar;`. The parent
  `pub use`s exactly the items that were reachable before the split, at the same path, so every item keeps one
  path and no caller outside the split file changes. Items only siblings need get `pub(super)` (or `pub(crate)`
  across the crate), never wider. A re-export of a private child is the module's interface, not a compatibility
  path: no second path exists.
- **D3. Tests sit out of line, named as tests.** An inline `#[cfg(test)] mod tests { … }` becomes `foo/tests.rs`,
  keeping the module path `foo::tests` and so its snapshot names; an insta snapshot moves to `foo/snapshots/`. A
  test module over the cap splits into `foo/tests/<group>.rs` along its own markers (section comments, `behavior`
  tags). A test file is named `tests.rs` or `*_tests.rs`, or lives under a `tests/` directory: the diagnostics
  source scanner and the blob builder treat those as test code. A blob input never holds an inline test module.
- **D4. A split is a move.** Bodies move byte for byte; `rustfmt` may re-indent. The commit holds only the move,
  the `mod`/`use`/`pub use` lines, the visibility D2 allows, `impl` headers when one `impl` spans files, and
  comments that state a file's location. Anything else, such as breaking one long function into several, is its
  own commit before the move, pinned by a test that covers its output.
- **D5. A file stays whole when it has no seam.** When the code outside the tests is under the cap and cohesive,
  only the tests move.
- **D6. Names avoid collisions.** A child is not named like a crate or module it would shadow in relative paths
  (`core_tool`, not `core`; `expression`, not `expr`; `coverage_tracking`, not `coverage`).

## Consequences

- A reader opens `foo.rs` first and sees the module's whole interface in its `pub use` lines and `mod` list.
- Moving a guest's or the SDK's source re-vendors the blobs: panic locations embed source paths, so the blob bytes
  change while the declarations do not.
- `git log --follow` on a child starts at the split; `git log -- foo.rs` keeps the parent's history.

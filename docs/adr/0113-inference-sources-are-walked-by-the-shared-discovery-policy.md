# Inference sources are walked by the shared discovery policy

**Status:** accepted (2026-10-16, architecture round 12, plan 02). Extends [ADR 0073](0073-a-batch-of-paths-narrows-the-stale-comparison.md)
D5 and [ADR 0080](0080-a-projects-sources-end-at-a-nested-project.md) D1; amends
[ADR 0104](0104-containment-is-applied-where-the-operation-opens-the-file.md) D2.

"Which source files do the enabled analyzers cover, and what do they hold" was answered by a second, hand-written
walker (`read/infer/discovery.rs`) and then four more reads that opened a file by `root.join(name)` (scan, the stale
check, the I202 line count, the mark). Only the mark was held inside the project. `specforge_common::discovery` states
that it is the one walker "for every surface that scans a workspace"; inference did not use it. Six defects were
reproduced against the base binary:

- R1: a `src/loop -> ..` link turned 2 files into 65 keys.
- R2: `src/ext -> ../../outside` was listed by `infer-status` and read by MCP `infer_gaps`.
- R3: one declared `excluded_dirs` replaced the default skips (a rust-only project walked `node_modules/` and
  `vendor/`; a TypeScript-only one walked `target/`).
- R4: a manifest with a `..` root or `..` index entries made `infer_progress` mark an outside file stale or deleted,
  `infer_gaps` scan it and the `inferred` lint report I200/I202 on it.
- R5: a file symlinked out of the project was scanned.
- R6: overlapping roots `["src", "."]` counted a file twice, and a nested project's files were the outer project's.

## Decision

- **D1. One module answers "the inference sources"** (`specforge_ops::read::infer::sources`): `SourceKey` (the source
  path rule), `Analyzers` (what is claimed and skipped), `ProjectFiles` (reads held inside the project root) and
  `InferenceSources` (the keys, walked once). Progress, gaps and its scan, the `inferred` lint and the session mark ask it.
- **D2. The walk is the shared walker.** `specforge_common::walk_files(root, skip, keep, stop)` generalises
  `walk_spec_files` from `.spec` to a predicate; no symlinked directory is entered, and a symlink is a source only when
  it resolves to a file.
- **D3. Skipped directories are the defaults united with every declaration, and hidden names.** The defaults are the
  shared `SKIP_DIRS`, `__pycache__` and `vendor`; every enabled analyzer's `excluded_dirs` is added, never substituted.
  A name starting with a dot is skipped (inference's predicate; `.spec` discovery is unchanged).
- **D4. Everything is held inside the project root, on every surface.** Inference reads use
  `Containment::within(root)`. A source root, a file or an indexed path that lands outside is none of the sources and is
  never read.
- **D5. One key rule.** Every source is a `SourceKey`, listed once whatever roots reach it. An index entry that is no
  key or lands outside the project is reported `deleted`, not `stale`.
- **D6. Each file is read once per operation**: the lint hashes and counts an indexed file from the same read.
- **D7. The walk ends at a nested project** (`ProjectRoot::is_root`), as ADR 0080 D1 does for `.spec` sources. The mark
  still records a file an agent names inside one.

## Rejected

- A second walker fixed in place: the shared one exists for this.
- Declared-replaces-defaults (today's rule): neither builtin analyzer can mean "walk the other's build output".
- The request's containment for the read views: the CLI would list files that leave the project, which the mark refuses
  over MCP.
- `ProjectPath` as the key: it admits absolute paths outside the root.
- Reading through the session ledger or `Disk`: inference is not a project input (`CONTEXT.md`, Inference manifest).

## Consequences

- CLI and MCP counts change (R1, R3, R6); R2, R4 and R5 are closed on both surfaces.
- A symlinked source root is walked as its target and keyed by the target's real path.
- The lint reads each indexed file once, where it read it twice.

## What would reopen it

An analyzer that must see generated output under a skipped name, or a request for symlinked directories as sources
(ADR 0073's own reopener).

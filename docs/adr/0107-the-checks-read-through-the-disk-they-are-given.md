# The checks read through the disk they are given

**Status:** accepted (2026-10-16, architecture round 11, plan 04). Reopens ADR 0031 D1 ("`RegistryBuild::files` is the
one set of files those checks read"). Amends ADR 0030 D1, D2 and D3 (check inputs are the recorded reads, stamped at
the read), ADR 0047 D3 (one `CompiledProject::check`), ADR 0020 (`Rules::files` is gone), ADR 0019 (`RuleInput` carries
the disk) and ADR 0106 D5 (the check inputs are what one check run read, read by the checks themselves rather than by
`before_checks`). D5(2) is amended by [ADR 0118](0118-the-paths-a-field-names-are-read-once.md): a missing `file_exists` file's directory is listed again, by the suggestion, as a real read.

## Context

Every disk read a check makes had a twin that predicted it for the project session:

- E016's `exists` and its suggestion's `read_dir` were mirrored by `RegistryBuild::files` and `with_named`;
- `file_exists`'s `exists` was mirrored by `rules::check::files`;
- the check passes' `BuildCache::read` was mirrored by a hand-copied condition (`with_check_passes`).

The core's check was split into `snapshot_now` and `check_over` only so the session could stamp the prediction between
the halves, and four tests existed only to keep the pairs in step. ADR 0031 D1 made the prediction the contract.

A prediction records what it predicts, not what the read followed, and it drifted twice (REPRODUCED in plan 04):

- a `file_reference` path that is a symlink is read through the link, but only the link's directory was watched, so
  `specforge watch` never re-ran E016 when the target appeared or went;
- the directory of every missing named file was a listing, though only E016's suggestion lists, so a file beside a
  missing `file_exists` file re-ran the checks for nothing.

## Decision

- **D1.** The checks reach the file system only through `specforge_registry::Disk` (`exists`, `list`, `read`), carried
  in `RuleInput`. No module of `specforge-registry` outside `#[cfg(test)]` calls `std::fs`; no check pass reads the
  build cache by another path.
- **D2.** A session's check inputs are its checks' recorded reads, the stamp taken at the read: the paths asked
  `exists` or `read` (`CheckInput::File`) and the directories asked `list` (`CheckInput::Listing`, also reached by any
  file in it), and the target of each that is a symlink.
- **D3.** One check step: `CompiledProject::check(&mut impl Reads)` takes the snapshot, runs the checks and seeds the
  memo inside `Reads::run_checks`, which replaces ADR 0106's `before_checks`. `snapshot_now` and `check_over` go.
- **D4.** Three adapters of `Disk`: the ledger's check run (`RunDisk`, recording), `PlainDisk` (a one-shot compile
  through `Unrecorded`) and `MemoryDisk` (tests, logging every read asked).
- **D5.** Behaviour changes: (1) a symlinked check input's target is watched and registered, and its change re-runs
  the checks; (2) a missing `file_exists` file no longer makes its directory a listing; (3) the build cache is
  recorded when a check pass reads it, in read order.

## Rejected

- Keeping the predictions with a debug-build comparison against the reads: the twin stays.
- A disk rooted at the spec root: two adapters would each resolve relative and absolute paths.
- The port in `specforge-common` now: every caller is at the pipeline layer or above.
- A gate forbidding `std::fs` in the registry's `src`: inline test code legitimately reads fixtures.
- Module reads (`Installed::load`) through the port: not a check read; its prediction shares `Installed::resolve` with
  the read and cannot miss a path.

## What would reopen it

A check that needs a read the port cannot express (metadata, a glob), or a lower layer that needs the port (it then
moves down).

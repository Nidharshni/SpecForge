# A project session reads the disk through one ledger

**Status:** accepted (2026-10-16, architecture round 11, plan 03). Carries out ADR 0030 D1 and D3 and ADR 0073;
amends ADR 0047 D3. Amended by [ADR 0107](0107-the-checks-read-through-the-disk-they-are-given.md) (`Reads::before_checks` is replaced by `Reads::run_checks`: the checks read through the run). Amended by [ADR 0166](0166-a-one-shot-compile-reads-its-sources-through-the-disk-it-is-handed.md) (`Unrecorded` reads each source through the disk it is handed).
D1's prediction is reversed by [ADR 0114](0114-environment-inputs-are-recorded-as-read.md) (the reader port into `specforge-installed`: the lock and the modules are stamped at their read; `before_load` is gone).
D5's "the renewals of the links happen in place" is amended by [ADR 0129](0129-a-source-read-is-a-recorded-read.md): a source's hops are recorded by its read, through the recording run; discovery records nothing. D5's `finish` is amended by [ADR 0133](0133-recorded-inputs-are-one-table.md): every run is recorded by one `finish`, and each recorded input's canonical form is taken when it is recorded.

ADR 0030 D3's rule is "stamp a file, then read it". It had no owner. `ProjectSession` carried it out by hand at six
sites (the open, the cold read, a re-read, a release, the checks), with the stamps (`freshness::DiskSnapshot`) and the
inputs value (`inputs::SessionInputs`) in two modules that only make sense together. A source key was derived from a
path in six places under three rules: stamps, `discover_at`, `link_at`, the cold read, `reach` and `watched`. They agreed
only because a walk always joins names onto the spec root.

One disagreement was a live defect. An editor buffer of a file reached through a symlinked directory under the spec
root (`dirlink/a.spec`, with `dirlink -> sub`) entered the session's graph as a second source: the LSP reported E002
that `specforge check` did not, and it stayed after `didClose`. `SourceRoot::holds` asked whether each directory on the
way held `specforge.json`, but never whether it was a symlink; the walk (`files_at`) asks both (ADR 0073 D5). The same
rule, "a source that is no source any more leaves the session", took three modules to state (`7954b9f4`: a stamp
removal, a link filter, a read-as-gone), and the input kind added by `893abc6c` (symlink targets) took all three files.

## Decision

- **D1.** One crate-private module, `specforge_project::ledger`, does every disk read a project session makes, and
  stamps each file in the call that reads it. The lock and the modules are stamped before the load reads them: a
  prediction, ADR 0030 D3 unchanged.
- **D2.** A one-shot compile runs the same steps through an adapter that records nothing (`Unrecorded`), behind the
  crate-private `trait Reads` that `Ledger` also implements. `Environment::load` and the session's open are one
  function (`Environment::read_through`); `CompiledProject::read` and `::check` are written once. A source is stamped
  when it is read, so a held source is not stamped until it is released (it is left out of `changes` and `stale`
  meanwhile).
- **D3.** A source key is derived in `ledger::key`: a walked path lexically (`discovered`), a canonical path or link
  target by `relative_canonical`, a path an editor or watcher spells by `source_key`. A key with a symlinked directory
  on the way is no source (`SourceRoot::holds`), as discovery has always said.
- **D4.** `SessionInputs` stays the value watch, the LSP and MCP read. The stamps are the ledger's alone.
- **D5.** The check inputs are what one check run read (`Ledger::checks() -> CheckRun`): `read` stamps a path and then
  runs the caller's read, `finish` records exactly what was read, in read order, and retakes only the canonical forms of
  the check inputs. A reader someone else calls can record through it (the seam the next plan uses). The build cache is
  recorded when the checks run, and the renewals of the links happen in place: `on_disk` runs once, at `opened`.

## Rejected

- A public `Ledger` giving `watch_roots` and `watched`: every consumer renamed, and its equality would have to skip the
  stamps by hand.
- The one-shot compile through a recording ledger: a stat per source, a second read of every file modified in the last
  two seconds (the racy hash) and a canonicalization per input, for a record nobody reads.
- One `Ledger` with a "records nothing" flag: ADR 0047's rejected optional session state.
- Stamping every discovered source at discovery: a held key is not read from disk, so it should not be stamped.

## What would reopen it

A reader port into `specforge-installed` (then the lock and the modules are stamped at their read too); a surface that
needs the stamps themselves.

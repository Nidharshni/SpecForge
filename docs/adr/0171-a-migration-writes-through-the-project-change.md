# A migration writes through the project change

**Status:** accepted (2026-10-19, architecture round 18, plan 02). Amends [ADR 0144](0144-a-project-file-is-written-by-replacing-its-name.md) D1 and D2 (migrate's sources and restores are replaced by their names, and a backup has its source's permissions), [ADR 0128](0128-one-project-change-over-held-files.md) D1 (a change stages a name the operation owns; its "Reopen when" is answered), [ADR 0112](0112-every-project-file-an-operation-opens-goes-through-the-requests-containment.md) D1 (an owned name is opened at its spelling; its Rejected "Opening migrate's sources as `Held`" is reversed), [ADR 0158](0158-an-edited-compile-and-a-migration-read-no-source-past-the-bound.md) D3 (replaced: a file that cannot be migrated leaves every file as it was) and [ADR 0170](0170-a-failed-run-is-an-outcomes-fact.md) F4 (a migration's write failure is the project change's `Err`).

**Amended by [ADR 0186](0186-a-migration-is-judged-before-it-is-written.md) (round 20): D1 and D4: the hooks and the graph comparison run before anything is written, and a refused migration writes nothing; there is no put-back to undo it.

## Context

`specforge_migrate` kept its own writer beside the project change: a temp created beside each source and renamed over
it, a second copy of that for the restores (`write_atomically`), the record written and removed by name, and an ops
pre-hold list of every name that writer would open, whose `Held`s were dropped. ADR 0144 D1 had declared the
`fs::write`-then-`rename` copies gone; this one was missed. It had drifted from ADR 0144 D2 and ADR 0128 D3, each
reproduced: a 0444 source was rewritten and came back 0644, exit 0, over the CLI and over MCP, while `specforge set`
refuses the same file (`file_unwritable`); a 0600 source was widened and its backup created 0644; `--rollback`
widened a private file and rewrote a read-only one; a record the run could not write was dropped and the run
reported `ok`; and a run with one failed file left the others migrated, with no hooks, no graph comparison and no
record, so `--rollback` had nothing to undo. Every source was also read twice per run.

## Decision

- **D1.** The migration module answers what each file becomes (`Rewrite { path, name, before, after }`, written by
  nothing: `migrate_project` has no `dry_run` and no `no_backup`); ops writes the run as one project change. The
  automatic rollback is the change's put-back; the hooks and the graph comparison are `commit_then`'s last step. The
  pipeline's rollback loop, `run_rollback`, opens no file: it is handed the record, a `read` and a `write`.
- **D2.** A file that cannot be migrated leaves every file as it was. The plan reads and reports every file, a dry
  run and a failed run alike; a failed run writes and compiles nothing. This replaces ADR 0158 D3.
- **D3.** A backup is an **owned name** in the change (`ProjectChange::stage_owned`, `Held::create_owned`): never
  read, created new at its spelling with its source's permissions set before the first byte (whatever is there, a
  link or a file left by an earlier run, is unlinked first, never its target), unlinked when the change is put back.
- **D4.** An undone run leaves nothing: the put-back removes the run's backups, and `writes` is what could not be put
  back.
- **D5.** A backup has its source's permissions; the record is a project file in the change, staged over its current
  bytes (read through the bound), so a read-only record refuses the run. It hashes the planned text: a hook cannot
  write a file (the sandbox grants no directory).
- **D6.** `--rollback` restores per file, each through `Held::replace` (it keeps the mode and refuses a read-only
  file), with the caller's containment (`rollback(root, caller)`); it is not a project change, because the spec makes
  each restore independent. The record is rewritten or removed through its `Held`.
- **D7.** A write failure is the change's `Err`, worded once (ADR 0155): `file_unwritable`, `file_changed`,
  `file_unreadable`. A source, a backup or the record outside the bound is `outside_root` (`source_outside_root` is
  gone). The temporary names are not held: `replace_file` unlinks its own. `replace_file` sets the mode before it
  writes.

Rejected: D2, keeping per-file independence for the writes (two rules, a writability probe at plan time) or keeping
today's per-file writes with no project change (migrate keeps its own put-back); D3, a backup as an ordinary staged
file (`Held::replace` follows an inside link, and reading a stale backup to put it back could copy an outside file
in); D4, keeping the backups of an undone run (writing them before the change, with a false "nothing was written");
a `Failure::Change` variant (the change's `Err` is already the report); `--rollback` as one change.

## Consequences

CLI: `specforge migrate` over a read-only source or record exits 1 with `error[file_unwritable]: cannot migrate: ...;
nothing was written` (was exit 0 and the file made writable); a source keeps its mode and a backup gets it; one bad
file stops the run with every file as it was; an undone run leaves no backup; `--rollback` keeps each file's mode and
fails a read-only one. MCP: the same, as `isError` with `permission_denied`; a `.spec.tmp` link alone that leaves the
project is unlinked, no longer refused. Rust API: `migrate_project` loses its flags; `Original`, `restore`,
`written_beside` and `MigrationRecord::{read, write, remove}` are gone; `migrate::rollback` takes a `Caller`.

## What would reopen it

A hook gains a file-system capability (the record would hash a re-read); a second operation owns a name in the
project.

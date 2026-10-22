# A migration is judged before it is written

**Status:** accepted (2026-10-21, architecture round 20, plan 03). Amends [ADR 0171](0171-a-migration-writes-through-the-project-change.md) D1 (the hooks and the graph comparison are no longer the project change's last step) and D4 (an undone run is replaced by a refused run that writes nothing), [ADR 0158](0158-an-edited-compile-and-a-migration-read-no-source-past-the-bound.md) D4 (there is no post compile: the edited compile replaces it, and it reads no source) and [ADR 0170](0170-a-failed-run-is-an-outcomes-fact.md) F1 ("a change it undid" becomes "a migration it refused"). Amended by round 22 plan 04 (2026-10-21): D5 (the environment is loaded before the plan, whose defaults step reads its field forms) and D7. Amended by round 23 plan 01 (2026-10-22): D6, see [ADR 0211](0211-a-migration-is-judged-by-the-errors-it-adds-whatever-its-graph.md). Cites [ADR 0162](0162-a-mutations-preview-is-a-fact-of-its-outcome.md) P1 and [ADR 0037](0037-the-sandbox-is-limits-the-host-enforces.md).

## Context

- `migrate::run` wrote the migrated texts first and judged them afterwards: the extensions' migration hooks and the graph
  comparison were the last step of the project change's commit, and a refused run was put back. A dry run returned before
  any of that, so it reported `ok: true` (exit 0) for a migration the real run refused (exit 1, W054), and a refused run
  replaced the user's file twice (new inode, new mtime) and created and unlinked its backup.
- A hook gets no file system (ADR 0037, ADR 0171 D5), so writing first bought it nothing; ADR 0171's reopen trigger (a hook
  with a file-system capability) has not fired. Typed edits already judge in memory (`CompiledProject::edited`, ADR 0151).
- Three smaller defects sat in the same path: the CLI's line for a refused run said "files restored from backups" under
  `--no-backup` and named a cause the run's `Failure` did not, and `Failure::DoesNotCompile` counted every error of the
  migrated project, the ones it already had included.

## Decision

- **D1. A migration is judged before anything is written.** `migrate::judge(pre, planned, input)` runs the hooks in load
  order and, when none fails, compiles the planned texts over `pre` (`CompiledProject::edited`) and takes the graph delta
  less the steps' own changes. `Verdict::refusal` is the run's cause.
- **D2. A dry run is judged as the run is.** Its `ok`, `structural_differences`, `post_migration_errors`,
  `hooks_invoked` and failure are the run's. A dry run the run would refuse is a failed run (ADR 0170).
- **D3. Only a kept migration is staged and committed.** A refused one writes nothing: no source, backup or record,
  `--no-backup` included. There is no automatic rollback: `put_back_summary`, `Outcome.rollback` and the document's
  `rolled_back` and `rollback` are gone. The change's put-back stays for a write that fails (ADR 0128).
- **D4. The hooks are called with the planned files' names before they are written.** A hook gets no file system, so the
  order is invisible to it.
- **D5. Order** *(amended, round 22: see D5 below the Amendment)*: plan; nothing pending or a file failed: return, nothing compiled; `pre`; judge; dry run or refused:
  return; stage; commit. Each source is read twice (the plan and `pre`), the environment loaded once.
- **D6. A refusal for errors counts the errors the planned project has that the project has not**
  (`edit::validate::added`). *(Amended, round 23: a refusal for errors is read before the graph delta, ADR 0211.)*

## Rejected

- Compile first and plan over `pre`'s texts: one read, but a compile on every run with nothing pending or a failed file
  (reversing ADR 0171 D2), and `pre`'s E025 in place of the bound's message.
- Judging the dry run in memory and the run after writing: two judges that can disagree, which is the defect.
- A dry run without hooks: a preview whose verdict leaves out a cause the run has.
- Migrate on `PlannedChange` (ADR 0184): it stages no owned name and no record.

## Consequences

- CLI: `migrate --dry-run` exits 1 when the run would be refused; a pending dry run reports
  `post_migration_validated: true`; the human report of a failed run is the W054s and the failure's message.
- MCP: a refused dry run is `isError`, its document the data.
- Both: the document loses `rolled_back` and `rollback`; the failure messages end "nothing was written".
- Rust API: `Outcome.verdict`, `migrate::{judge, Verdict}`; `Outcome.rollback` and the flat verdict fields are gone.
- A pending dry run costs a compile and an edited compile; a run costs one environment load fewer than before.

## Amendment (architecture round 22, plan 04)

- **D5, amended.** Order: config; hold the sources; load the environment; plan with its field forms; nothing pending
  or a file failed: return, nothing compiled; `pre` in that environment; judge; dry run or refused: return; stage;
  commit. The environment is loaded once, before the plan, also when nothing is pending (it makes extension calls,
  never a compile or a source read). `CompiledProject::compile_through(env, disk)` takes the environment the run
  loaded.
- **D7. The plan asks the rule the judge applies.** The defaults step hoists a field only where
  `FileDefaults::entry` applies it under the environment's field forms (ADR 0154 D1), and writes its block above the
  file's first block (after an import only when that import stands above it, E078). A step that would change the graph
  is a defect the judge refuses, never a per-step retry.
- **Left open (04-D3), closed by [ADR 0211](0211-a-migration-is-judged-by-the-errors-it-adds-whatever-its-graph.md).** `Verdict::refusal` keeps a migration that adds an error while its graph is unchanged. This
  amendment does not close it: refusing every added error would also refuse a migration that only moves lines in a
  file reporting a message that names a position (a duplicate ID). It stays a later card, to be settled with the
  key `edit::validate::added` compares by.
- **Rejected:** a predicate exported from `graph::defaults` for one caller (the hoist probes `FileDefaults` with a
  one-field block); re-planning without the defaults step after a refusal; the hoist after `pre` (this ADR's own
  Rejected); loading the environment only when something is pending (a third read of each source).

## What would reopen it

A hook gains a file-system capability (then the hooks see files and judging must follow writing, as ADR 0171 had it); a
check that reads the sources from disk (the edited compile would then judge stale files).

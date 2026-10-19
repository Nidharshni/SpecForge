# An edited compile and a migration read no source past the bound

**Status:** accepted (2026-10-19, round 16, plan 01). Amends [ADR 0145](0145-the-session-reads-sources-through-the-disk-the-surface-hands-it.md) D1, [ADR 0146](0146-a-file-with-more-than-one-name-is-not-read-through-a-bound.md) D4 and [ADR 0151](0151-a-spec-file-is-changed-by-a-typed-edit.md) D3. **Amended by ADR 0166 (2026-10-19):** D4 is replaced: the compiles before and after read through the same bound; the compile before the migration did read a refused source.

Over MCP the session reads each `.spec` source through the contained disk, and a source the bound refuses (a symlink
that leaves the project, a file with more than one name) is held as an unreadable key with its E025. Two paths read
such a source plainly after all. `CompiledProject::edited`, the compile every typed edit validates against, put the
unreadable keys back through `Keyed`, whose source read was `Unrecorded`'s plain read: the outside file was parsed, its
diagnostics came back in E080 and in `EditReport.added`, and its shape decided whether an edit was refused. A migration
read every source with `std::fs::read_to_string`: a hard-linked outside file's text came back in `diffs` (a dry run
too), its diagnostics in `post_migration_errors`, and a run copied it into the project. `edited` (2026-10-18) came a day
after the bound (2026-10-17) and forgot it.

## Decision

- **D1.** An edited compile reads no source: each unplanned source is what the project holds
  (`SourceCache::as_read`: its text, or its recorded E025), the planned texts replace theirs, then the checks run as
  any compile's (ADR 0145 D3). `Keyed` is deleted; `Reads` has two adapters, `Unrecorded` and `Ledger`.
- **D2.** A migration reads each source once, through the caller's containment (`migrate_project`'s `read`; ops
  passes `caller.containment().disk()`): over MCP a file with more than one name fails that file with the bound's
  message; the CLI reads plainly as before.
- **D3.** A refused source fails its file; the other files migrate, as when any source is unreadable.
- **D4.** Migrate's pre and post compiles stay one-shot plain compiles (stated, not closed): over MCP they run only
  after every source of the run was read through the bound, because a failed file returns the run before them.

Rejected: a disk handed to `edited` (ADR 0139 and 0145 reject a disk in the compiled project; it would re-read sources
the plan is made over); a pre-read in the migration guard (two reads and a window, ADR 0146 D2); `&dyn Disk` in
`specforge-migrate` (a new crate edge for one method); refusing the whole migration when one source is refused.

## Consequences

An edit's verdict over MCP no longer depends on files outside the project. An edited compile does not see a source that
became readable after the compile (the write's `file_changed` guard is unchanged). A bounded migration with a
hard-linked source reports `FilesFailed` and leaves that name with two names. `specforge_migrate::migrate_project`
takes a sixth argument, `read`.

## What would reopen it

A typed edit that must validate against disk state newer than its compile; a migration hook that reads sources itself;
a bounded surface that needs a compile outside the session (then a `CompiledProject::compile_through` with the
session's source disk).

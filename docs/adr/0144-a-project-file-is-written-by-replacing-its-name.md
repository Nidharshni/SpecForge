# A project file is written by replacing its name

**Status:** accepted (2026-10-17, round 14 follow-up G2). Amends [ADR 0112](0112-every-project-file-an-operation-opens-goes-through-the-requests-containment.md) D1 (the rule holds below `app` too), [ADR 0128](0128-one-project-change-over-held-files.md) (its `openat` reopen item is not needed for hard links), [ADR 0137](0137-a-lock-plan-bounds-its-own-writes.md) (the lock's temporary name) and [ADR 0139](0139-a-compiled-project-reads-its-recorded-report-through-the-disk-it-is-handed.md) (the Consequences sentence on hard links).

D3 is amended by [ADR 0145](0145-the-session-reads-sources-through-the-disk-the-surface-hands-it.md): the read of a hard-linked source has its home there.

Round 14 recorded that "a hard link is out of scope: no path rule can see a hard link". That is true of the name and
false of the write. Every write that went through `Held::replace` or migrate's `create_new_beside` unlinks the name
first and never opens the shared inode. Four writers still opened a project path in place and wrote through a hard
link (three of them over MCP): the migration record (`.specforge/migration.json`), the lock's temporary file
(`specforge.lock.tmp`, also a symlink escape, so `add_extension` and `remove_extension` could overwrite any file the
user may write), collect's captured output, and the build and schema caches' temporary files (CLI only).

## Decision

- **D1.** A project file is written only by replacing its name: `specforge_common::fresh::replace_file(path, bytes)`
  (unlink the temporary name, create it new, sync, rename onto the name) or, for a stream that cannot be staged,
  `create_fresh(path)` (unlink the name, create it new). The inode that held a name is never opened for writing.
  `Held::replace` and `Held::create_fresh`, migrate's backups, temporary files and record, the lock, the build cache
  and the schema cache all use them; the `fs::write`-then-`rename` copies are gone.
- **D2.** A file that is there keeps its permissions, and one the process may not write is refused with
  `PermissionDenied`: the owner's read-only marker is not lifted by a rewrite. A read-only lock or cache is now refused
  where a rename used to replace it silently.
- **D3.** A hard link on READ is not refused. A read of a hard link is a read of the file in the project. The exposure is
  stated, not closed: an MCP read of a hard-linked source quotes the first unexpected token of a parse error, and a
  hard-linked report can steer a coverage verdict. The symlink spelling of the same read is a separate question.

## The attacker, measured

On macOS an unprivileged `ln` of a file the user cannot read (`/etc/master.passwd`) succeeds; there is no
`fs.protected_hardlinks`. On Linux that sysctl (default on) refuses a link to a file the caller neither owns nor can
read and write. The planter is therefore a process that may create a name in the project but is confined from the
target, sharing the project with an MCP server that runs with the user's rights: ADR 0092's confused deputy, in a
spelling the canonical-path rule cannot see. For a write the deputy turns "may create a name" into "may overwrite any
file the user may write on that volume"; for a read, into one parse-error token or one boolean. Writes are cheap to make
unreachable; reads are not worth a rule. A hard link cannot cross a file system, and a clone cannot carry one
(`bsdtar` refuses a hostile hard-link member).

## Rejected

- **A refusal on `st_nlink > 1` (reads of state files, or every read).** `Disk` has no metadata method; false positives
  for `cp -l` snapshots, pnpm, Nix, Bazel and `rsnapshot`; Unix only. It would close the coverage-steering read for
  a planter who gains one boolean.
- **`openat` (ADR 0128's reopen item).** It narrows a race between `resolve` and open, not a hard link.
- **A `disallowed-methods` lint on `std::fs::write`.** It would hit hundreds of test sites; a grep gate is the
  follow-up if the pins prove not enough.
- **A lock-specific variant that lifts the read-only refusal.** The owner's marker is the owner's decision.

## Consequences

The pins are `crates/surfaces/mcp/tests/hard_links.rs`, `crates/surfaces/cli/tests/hard_links.rs`,
`containment/replace_tests.rs` and `collect_capture_is_not_written_through_a_hard_link`; each plants a link and asserts
the outside bytes. A new writer of a project file can regress this: use `replace_file`.

## What would reopen it

The project served to a sandboxed agent with a stated promise that it cannot learn the contents of outside files: then
the `nlink` refusal on state-file reads comes back.

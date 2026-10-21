# A one-shot compile reads its sources through the disk it is handed

**Status:** accepted (2026-10-19, round 17, plan 02). Amends [ADR 0106](0106-a-project-session-reads-the-disk-through-one-ledger.md) D2 (the one-shot adapter takes the source disk), [ADR 0145](0145-the-session-reads-sources-through-the-disk-the-surface-hands-it.md) D1 (a one-shot compile is handed a source disk too) and replaces D4 of [ADR 0158](0158-an-edited-compile-and-a-migration-read-no-source-past-the-bound.md). Keeps [ADR 0139](0139-a-compiled-project-reads-its-recorded-report-through-the-disk-it-is-handed.md) D2.

The crate-private `Reads` seam had two adapters, and only the session's `Ledger` could be told which disk a source is
read through; `Unrecorded`, the one-shot compile's, read `PlainDisk`. So no one-shot compile could be bounded, and
migrate, the one operation a bounded surface (MCP) runs that compiles one-shot, read its sources through the caller's
bound and then compiled the project plainly twice. ADR 0158 D4 accepted that on the claim that both compiles run only
after every source passed the bound. That holds for the compile after the files and not for the one before them: it
runs after a preview whose refused file only made the run pending, and before the failed-file return, so over MCP the
applying migration read the bytes of a hard-linked outside source (seen through the file's access time; nothing of it
reached the reply, but the project's own extensions were handed its entities).

## Decision

- **D1.** `Unrecorded` reads each source through the disk it is handed; the environment and the checks stay plain
  (ADR 0145 D3). `CompiledProject::compile_through(env, &dyn Disk)` (round 22 plan 04: the caller loads the environment) is the bounded one-shot compile;
  `compile` and `of` read plain. The compiled project keeps no disk.
- **D2.** A migration compiles the project before and after through the same disk it reads its sources with
  (`caller.containment().disk()`): over MCP a refused source is E025 in both compiles; on the CLI the unbounded disk
  reads what `PlainDisk` reads.
- **D3.** MCP keeps three spellings of one containment: the session's source disk, the view's `contained` flag, the
  `Caller`'s `Containment` (ADR 0139 D2, 0145 D1, 0124). They are three consumers' shapes of one root (an owned box
  kept across reloads, a flag in a `Copy` view, a borrow in the `Caller`), set from one root, and have not drifted.

Rejected: a session opened for the one-shot compile (ADR 0106's rejection; its racy stamp reads a refused file
plainly); returning before the first compile when the preview failed a file (ADR 0158 D3); a required disk on
`compile` (24 call sites for a rule review keeps); the view reading its report through the session's source disk
(widens "source disk" to every project read, moves ADR 0139 D2's per-view decision to open time, no drift observed);
a disk stored in the compiled project (ADR 0139, 0145).

## Consequences

No observable change in any reply. A bounded migration with a hard-linked source no longer hands the outside entities
to the project's extensions in its first compile. The session's racy stamp still hashes a refused source's bytes below
the `Disk` port (stated; the stamps belong to the link-chain plan).

## What would reopen it

A bounded caller of `of`; a second bounded one-shot compile outside ops (then a required disk on `compile`); a view
whose report bound must differ from its surface's, or a `contained` flag found missing (then the view reads through the
session's disk).

# Recorded inputs are one table, and the class decides

**Status:** accepted (2026-10-17, architecture round 14, plan 01). Implements [ADR 0114](0114-environment-inputs-are-recorded-as-read.md) D3 and [ADR 0129](0129-a-source-read-is-a-recorded-read.md) D3 in one place. Amends ADR 0030 D6 and ADR 0106 D5. Keeps ADR 0106 D4, ADR 0129's rejected "a stamp per hop" and ADR 0114's rejected "`read_link` on the port".

ADR 0114 D3 and ADR 0129 D3 give one rule for what a session read: every input is keyed by its own name in its
canonical directory, and each link hop it followed is an input of its own. The ledger wrote that rule once per kind:
four parallel lists and four canonical maps in `SessionInputs`, three stamp maps in `Stamps`, three `Run::finish_*`
methods and 22 walks over one kind's container. Five commits in four days carried the rule to one more kind. One walk
forgot a kind: the pending directories covered environment and check inputs but not source hops, so `specforge watch`
never saw a symlinked source come back when a hop's missing directory outside the root was created, while MCP did
(its `stale()` walks discovery). The spec says "whatever the stamps say" for every input.

## Decision

- **D1.** A session's recorded inputs are one ordered table in `SessionInputs`: each row is its spelling as read, its
  class (`Environment`, `Check`, `Hop(source key)`, `Listing`), its key (its own name in its canonical directory; a
  listing's directory canonical) and, when its directory is missing outside the root, where creating the next
  directory toward it is seen. Key and pending are taken when the row is recorded.
- **D2.** Every answer iterates that table, and the class decides how a changed path reaches a row, which directory is
  watched for it, how the LSP names it and what its change is (the environment loads again; the checks run again; a
  hop's source is read again).
- **D3.** A run is recorded by one `finish`, told what it recorded (`Recording`); one ledger function replaces the rows
  and the stamps it covers. The stamps stay the ledger's alone (ADR 0106 D4), keyed by the row; a hop row has none and
  compares its source's stamp (ADR 0129).
- **D4.** A source hop whose directory is missing outside the root is pending like every input: its nearest existing
  ancestor is watched for its own entries, and a directory created on the way is that source's change, whatever the
  stamps say (amends ADR 0030 D6).
- **D5.** Behaviour changes: watch sees a symlinked source come back when a hop's missing directory is created;
  `watched()` lists each path once.

## Rejected

- The stamps inside the rows of `SessionInputs` with an equality that skips them: ADR 0106's rejected "its equality
  would have to skip the stamps by hand".
- The table in `Ledger` with `SessionInputs` rebuilt from it: a second copy of the rows.
- A hop's pending change through the stamp comparison: lost when the directory appears before the file.
- Every row's key and pending directory taken again at every record.
- A trait with four implementations, one per class: no second implementation of the seam.

## What would reopen it

An input whose change is not a reload, a re-run of the checks or a source reread (a per-source check input, say), or
symlinked directories as sources (ADR 0073 D5). A dangling source link that comes alive is not covered: discovery
skips it, and recording its hops at discovery reopens ADR 0129's rejected option.

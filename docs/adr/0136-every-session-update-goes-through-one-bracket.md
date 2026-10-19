# Every session update goes through one bracket

**Status:** accepted (2026-10-17, architecture round 14, plan 04). Amends [ADR 0046](0046-the-project-session-holds-the-editor-buffers.md) D3 (the sentence "A disk update always applies: what an import names is read from disk") and [ADR 0109](0109-reopening-a-project-in-place-is-one-session-operation.md) D2 (where the skipped-checks flag lives). Keeps ADR 0030 D2, [ADR 0100](0100-a-session-takes-a-change-from-disk-only-as-changes.md) D1 and ADR 0106 D4.
*Amended (2026-10-19, round 16, plan 02): D5 (a disk change is one update over all it names, ADR 0159).*

Whether an update changed the session's inputs was decided three ways: a `bool` threaded by hand through 11 functions
and 50 places (`set_hops`, `finish_source`, `read_source`, `reread`, `forget_source`, `applied`, ...), a
clone-and-compare inside the check run (`Run::finish`), and an equality test in `replaced`. The `Update` was
assembled in four places (`applied`, `unchanged`, `replaced`, `recheck`), and "the checks were skipped" was stored
twice (the session's flag and the compiled project's emptied check list). The three doors disagreed on whether a read
is a change: `hold` and `release` dropped a read that gives the text the project already holds, `reread` dropped only
a current `Gone`. So a source touched, or written again with its own text, was re-parsed, re-checked and printed
`rebuilt` by `specforge watch` (7.8 s per touch on this repository's spec, debug build), while the same text through
`release` changed nothing. The bool was rewritten five times in four days. ADR 0046 D3 intended the re-parse for a
reason ADR 0055 removed: import resolution reads no file.

## Decision

- **D1.** Every `Update` is made by one private bracket, `ProjectSession::update(work)`. What the session reports and
  a copy of its inputs are taken before `work` runs; after it, `inputs_changed` is their comparison and the `Update`
  is built there. `Update::of`, `applied`, `unchanged` and `replaced` are gone.
- **D2.** No ledger or compiled-project function says whether the inputs moved: the nine `-> bool` returns go.
- **D3.** One rule for whether a read changes the project: it does unless the project already holds what it gives
  (`CompiledProject::is_current`), for a held buffer, a released one and a source reread from disk alike. A source
  whose read gives the text the session built it from is re-stamped and changes nothing else; a link hop its read
  followed that moved still changes the session's inputs.
- **D4.** "The checks were skipped" is the compiled project's state (`CompiledProject::checks_skipped`).
- **D5.** `release` and a disk change answer `None` when the update changed nothing: no file applied, no check run,
  the inputs as they were (`Update::changed_nothing`).

## Rejected

- D1 as a guard value with an explicit close: a door could read through the ledger before the open and lose a hop's
  move.
- D2 as a generation counter on `SessionInputs`: `set_checks` would still compare to avoid false moves (ADR 0099).
- D4 as an `Option` from `work`: a second assembly site, or an `Option` from `hold`.
- A release that re-parses: contradicts ADR 0046 D2.

## What would reopen it

A surface that must re-parse or re-check a source whose text did not change (a "force rebuild" command); an update
kind whose work cannot run inside one `&mut ProjectSession` borrow.

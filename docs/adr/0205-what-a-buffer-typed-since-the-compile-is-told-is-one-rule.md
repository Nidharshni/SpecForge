# What a buffer typed since the compile is told is one rule

**Status:** accepted (2026-10-21, architecture round 22, plan 06). Amends [ADR 0023](0023-the-lsp-reads-a-document-through-one-module.md) D3 and D7.

## Context

- In the same moment, over the same file, the LSP gave two answers about the compile's positions: rename refused
  `ContentModified` because the editor's text was not the text the project was compiled from, while documentSymbol,
  references, definition and workspace symbols answered line numbers of that compiled text, which the editor no longer
  had. ADR 0023 D7 let "locations stay positions in the compiled text".
- The rule lived in handlers: hover, `offered` and `workspace_edit` each asked staleness; the list handlers did not.
  A closed file changed on disk with no event was treated by rename as changed and by the lists as compiled.
- A diagnostics publish is the one place a compiled position is right, for it is labelled with the version of the text
  it was computed from.

## Decision

- **D1.** A span of the compile is a range of the editor's text only while that text (the open buffer, else the file
  on disk) is the compiled text (`navigation::Compiled::range`). A list leaves the item out; a definition answers
  none; an edit is refused `ContentModified` (`Unplaced`). This replaces ADR 0023 D7's "Locations stay positions in the
  compiled text".
- **D2.** What is sent labelled with the compiled text's version, a diagnostics publish, converts against that text
  whatever the editor holds (`Compiled::compiled_range`); a related location follows D1.
- **D3.** An editor range read into the compile (a code action's) follows D1 inversely (`Compiled::span`).
- **D4.** No handler asks staleness itself, but the precondition of an edit on the document it is asked from (rename:
  `Changed` waits, `Unbuilt` renames nothing). The cursor's rule (D7's own word) stays `Compiled::target`'s.
- **D5.** For the stale window the item is left out (option a): no outline entry, no location in the typed file.

## Consequences

- Wrong lines are never answered. A closed file changed on disk without an event is treated as typed.
- For the stale window the outline of the typed document is empty and F12 into it answers nothing; each request reads
  each distinct closed file it names once.

## Rejected

- Per-handler checks (the churn this removes).
- Mapping compiled positions through the edits since the compile: a change log per document and a second position
  conversion beside the one line index ADR 0023 D3 made the only one.
- Answering a stale outline from the buffer's syntax: the outline is navigation's (ADR 0016); a second outliner.
- `ContentModified` for documentSymbol: VS Code treats it as `null`, the same as leaving the items out.

## What would reopen it

An outline felt empty after typing in a project whose update is long: a request that waits (bounded) for its
document's compile before answering.

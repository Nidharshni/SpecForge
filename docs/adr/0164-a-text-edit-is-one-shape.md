# A text edit is one shape: a span and its new text

**Status:** accepted (2026-10-19, architecture round 16, plan 07). Amends [ADR 0016](0016-navigation-is-one-module.md)
(its rejected item "0-based byte columns"), [ADR 0032](0032-one-graph-build.md) (its consequence that `RenameEdit`
lives with the rename operation) and [ADR 0151](0151-a-spec-file-is-changed-by-a-typed-edit.md) (round 15 plan 06's
amendment: MCP returns the spec's `TextEdit`, not `McpRenameEdit`). Amended by round 17 plan 07 (2026-10-19): D4 is one conversion. D2 amended by [ADR 0184](0184-a-typed-edit-and-a-rename-write-one-planned-change.md): `rename::plan` refuses an edit that does not lie in its text, before anything is planned; `apply` is the planned change's.

## Context

One MCP server answered the same token's column as `13` from `specforge.find_references` and `specforge.suggest_fixes`
and as `12` from `specforge.rename`, under the same key `start_col`. A rename plan's edits were a type of their own,
`RenameEdit { file, line, start_col, end_col }`, with 0-based byte columns that `rename::plan` made by subtracting 1
from navigation's spans, applied by a private applier (which ignored an edit past the last line and panicked on a
column inside a character), and turned back into a `SourceSpan` by the LSP, which added the 1 back. MCP copied the type
field by field (`McpRenameEdit`) beside the spec's `TextEdit` that `suggest_fixes` returns.

ADR 0016 kept `RenameEdit` "so MCP's rename JSON is byte-identical": a backward-compatibility reason, which a
pre-release project does not honour.

## Decision

- **D1.** `specforge_ops::read::navigate::TextEdit { span, new_text }` is the one edit of a project file's text that
  ops answers (a fix, a rename plan), in ADR 0016 D3's convention: 1-based lines, 1-based byte columns, end exclusive.
  An empty span inserts. `RenameEdit` is deleted.
- **D2.** One applier, `navigate::apply_edits(text, edits)`, applies a file's edits whole or not at all: a span outside
  the text, an end inside a character or two overlapping edits refuse the set (`None`, never a panic); insertions at one
  position keep the order given. `rename::apply` refuses (`file_unreadable`) before staging a file when the edits do not
  lie in the text it planned.
- **D3.** MCP renders it once, as the spec's `TextEdit` (`reply::TextEdit`, `{file_path, range, new_text}`), for
  `suggest_fixes` and `rename`. `McpRenameEdit` is deleted from the spec.
- **D4.** The LSP converts an edit's span with `Compiled::range` and nothing else.

  > **Amended by round 17 plan 07 (2026-10-19):** `Compiled::workspace_edit` turns a request's edits into one
  > workspace edit, whole, or answers the first file whose editor text is not the compiled text (`Unplaced`); rename
  > and code actions both use it.

Rejected: 1-based columns under the old keys (a silent meaning change); folding the formatter's `format::Edit` into
`TextEdit` (it edits one buffer, possibly detached, with byte ranges the LSP converts straight to UTF-16; ADR 0021 D8
stands); a `Serialize` on ops' `TextEdit` with the wire's names (the wire is MCP's, ADR 0048).

## Consequences

`specforge.rename`'s `edits[]` is `{file_path, range, new_text}`: a client reading the old keys fails loudly. The LSP
and the fixes are unchanged on their wires. The spec's `verify unit "TextEdit schema is valid"` is proved by
`message_spec`.

What would reopen it: an edit that is not a position in a compiled text (it would be another module's, as the
formatter's is).

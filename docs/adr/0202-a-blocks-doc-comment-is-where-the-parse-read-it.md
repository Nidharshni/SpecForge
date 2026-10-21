# A block's doc comment is where the parse read it

**Status:** accepted (2026-10-21, architecture round 22, plan 03). Amends [ADR 0148](0148-every-block-has-a-title.md)
D4 (the titles step lifts the doc comment the parse read) and its amendment of [ADR 0095](0095-the-format-header-is-read-and-rewritten-by-one-module.md)
D3 (withdrawn: `header_row` is private again), [ADR 0149](0149-a-file-states-a-kinds-shared-fields-once.md) D12 (the
hoisted block goes above the first form's doc comment, below the format header) and
[ADR 0151](0151-a-spec-file-is-changed-by-a-typed-edit.md)'s Consequences (a delete removes the block's doc comment).

## Context

- "The `//` lines directly above a block, the format header never one" was rebuilt from text by three modules,
  written the same day; only the titles step excluded the header.
- `specforge delete` of a file's first block deleted the format header: a 0.9 file then read as 1.0, I007 gone.
- The defaults hoist wrote its block above the header, which stopped being the header and became the first block's
  doc comment. E080 and the migration's graph judge saw neither: the header is no node.

## Decision

- D1. The parse records where each block's doc comment is written (`Entity::doc_span`): the run of whole-line
  comments directly above the block's first line, no blank line between. A comment on the row where the item above
  ends trails it. The format header (ADR 0095's placement rule) is never one.
- D2. A typed delete removes the block and its doc comment; the titles step lifts it; the defaults hoist writes its
  block above it. None of them reads a comment line from text.
- D3. The formatter keeps its blank-line rule; its corpus property proves that formatting never changes a block's
  doc comment.
- D4. `header_row` is the parser's own (ADR 0095 D3's interface).
- D5. Comments inside a block (a member's, a field's) are not covered: they cannot be the header.

## Rejected

- The doc comment's text on the entity: ADR 0148 makes it a title, never a second field.
- A blank line after the header in canonical layout: about 100 test fixtures and every header-first user file would
  change, for a rule the parse now states.
- Amending ADR 0187 again: a comment outside the block is no declaration's name, title or statement.

## Consequences

- Breaking (Rust API, specforge-parser): `Entity::doc_span`; `header_row` no longer exported.
- `specforge delete` and MCP `delete` keep the format header; `specforge migrate` keeps it first.
- A file the earlier hoist already rewrote keeps its moved line: an ordinary comment now, its file at the current
  version. Move it back by hand.

## Reopen when

A surface must show a block's doc comment (hover, export), or a member's comment must be read from the parse.

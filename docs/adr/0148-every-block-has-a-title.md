# Every block has a title

**Status:** accepted (2026-10-18, syntax round, plan 02). Amends the `migration_semantic_preservation` invariant (a
migration step may add a title where a block had none) and [ADR 0095](0095-the-format-header-is-read-and-rewritten-by-one-module.md) D3
(the parser also tells where the header stands). D4 and its amendment of ADR 0095 D3 amended by [ADR 0202](0202-a-blocks-doc-comment-is-where-the-parse-read-it.md): the step lifts the doc comment the parse read; `header_row` is private again. Extended by [ADR 0225](0225-untitled-is-one-answer.md): a blank title is no title for every reader of the rule; the titles step writes one in its place.

A block's title is optional in the grammar and a union block has no slot for one, so 659 tracked blocks reach the graph
with no title: 568 entity blocks and 91 unions. For 119 of them the author wrote the description as a `//` comment
directly above the block, which the parser drops. A second, undocumented form also exists: a body field `title "..."`
is exempt from W020, but it is not the title.

## Decision

- **D1. Which blocks need a title.** Every block that becomes a graph node: entity blocks, unions, `spec` (its name)
  and `ref` (required by the grammar). Not `define` (never a node, W143). Nested blocks are field values.
- **D2. A semantic check, not a grammar rule.** The title stays optional in the grammar. A missing token would become
  an `ERROR` node and take the block out of the graph while it is being typed. The graph build reports **E076**
  per untitled or blank-titled block (an error, U1) and still places the block.
- **D3. E076 is emitted by the graph build**, in `declarations` beside W143, E002 and W060.
- **D4. `specforge migrate` writes the missing titles** (the `titles` step). `format` runs on every save and cannot
  invent content. A migration step runs on every file it changes, whatever the file's format version.
- **D5. Steps are not format versions.** A file with no header is at the current version, so a version bump would
  reach no file. A step runs on any file it changes.
- **D6. W160 is not used.** A warning for a title that restates its id would fire about 1,486 times.
- **D7. One title form.** The body `title` field's W020 exemption is deleted. A body `title` is a field like any
  other.
- **U1. E076 is an error.** Every writer already writes a title and `migrate` fixes the rest.
- **U2. A union takes a title:** `type S "States" = a | b`.
- **U3. The whole doc comment becomes the title**, its lines joined by one space.
- **U4. A block with no doc comment** gets its id in Title Case words.

## Consequences

E076; a grammar slot for the union title (grammar, scanner, lexer, formatter, LSP walk, TextMate grammar);
`specforge_migrate::beyond_steps`; the migration invariant keeps "nothing else in the graph may change"; a body
`title` is W020 unless an extension declares it.

## Rejected

- Making the title required in the grammar: it costs error recovery.
- Exempting unions: "every block has a title" would have an exception.
- `format` lifting comments: it would eat a comment as the user types.
- The first sentence only: sentence detection cuts at `e.g.`, and part of the comment is still lost.

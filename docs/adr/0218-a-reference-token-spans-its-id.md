# A reference token spans its ID

**Status:** accepted (2026-10-22, architecture round 24, plan 06). Extends [ADR 0163](0163-a-written-token-is-found-by-one-finder.md) D1 (a written token's span is its ID's, inside any quotes) and [ADR 0093](0093-a-fields-declared-type-decides-what-its-value-references.md) D1/D2 (the parse keeps the token's span; the reading answers the ID's). Amends [ADR 0116](0116-navigation-reads-the-text-the-project-was-compiled-from.md) D1's account of `Precision::Entity` for a reference. Declarations are unchanged ([ADR 0187](0187-a-declarations-name-is-where-the-parse-read-it.md) D3/D4).

## Context

- A quoted reference (`payload "user"`, `produces ["ev_one", ev_two]`) is a reference (ADR 0093 D3: it links, and only W159
  says to write it bare). `Reading::references` handed the parser's token span, quotes included, on as where the reference
  writes its ID. The finder, E003/E022/I004 and navigation read it, and navigation's check that the compiled text spells the
  ID at that span failed on valid input.
- Rename refused with `internal_error` (MCP) or `invalid_params` (LSP); find-references answered the whole holder block
  (MCP) or the quotes (LSP); the token at a position was nobody, so prepareRename answered nothing; E003's "Replace with" fix
  disappeared; E003/E022 underlined the quotes against "E003 span covers exactly the unresolved identifier token". The LSP
  cursor's lexical "in a string" gate also ran before navigation's token.
- The drift was in the reading, not the parse: ADR 0093 D1's token span is right for W159 and the formatter, which rewrite
  the whole value.

## Decisions

- **D1. `Reading::references` answers where a reference writes its ID**, `field_form::spell::id_span`, beside the escape
  table: one byte inside the quotes of `"id"`, three inside an unpadded `"""id"""`, else the value's span (a bare word, a
  scheme ref, a literal, or a value with an escape, padding or a line break, which has no bytes that spell its ID).
  `Reference.span` is that span, owned.
- **D2. The finder's token and the linker's E003, E022 and I004 are at that span.** W159 and the formatter keep the parse's
  whole-value span (ADR 0093 D1 unchanged).
- **D3. A reference is `Precision::Entity` only** for a file the compile holds no text of, or a value with no bytes that spell
  its ID (an escape, padding in triple quotes, a line break). Rename keeps refusing those with today's message.
- **D4. The LSP cursor asks navigation's token before its lexical place.** The token is token-precise, so prose in a string
  or comment is never one and still names nothing.

## Rejected

- A parser-recorded ID span: a kernel AST change that puts a reading concept in the parse (ADR 0093 D1: the parser reads
  syntax only).
- A text re-check in navigation: a fourth copy of the rule, and E003 would still span the quotes.
- Both spans on `Reference` / `WrittenToken`: no reader wants the token span.
- A replace fix that writes the candidate bare: the edit is the unresolved ID; writing it bare is W159's concern.

## Consequences

- Rename, references, the token at a position and the replace fix work on a quoted reference; definition, references, rename
  and hover work from inside the quotes.
- E003, E022 and I004 columns of a quoted reference move one byte inside each quote.
- `specforge-graph`: `field_form::Reference::span` is `Option<SourceSpan>` and `Reference` is no longer `Copy`.

## What would reopen it

A value form whose ID is written with a fixed delimiter other than `"` and `"""`, or a reader that needs the token span from
a `Reference`.

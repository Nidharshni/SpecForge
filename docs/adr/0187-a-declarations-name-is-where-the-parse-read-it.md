# A declaration's name is where the parse read it

**Status:** accepted (2026-10-21, architecture round 20, plan 04). Amends [ADR 0016](0016-navigation-is-one-module.md) D3 (a declaration's `Precision::Entity` is never a matter of layout), [ADR 0116](0116-navigation-reads-the-text-the-project-was-compiled-from.md) D1 (the cases in which a declaration answers `Entity` are listed) and [ADR 0176](0176-the-finder-is-the-only-record-of-where-a-name-is-written.md) (scope: a declaration's name is read from the same parses, by navigation). Amended by round 21 plan 03 (2026-10-21): D4 (the title too), D5 (reversed), Rejected (spans on verify statements). Its titles-step amendment extended by [ADR 0225](0225-untitled-is-one-answer.md): a blank title is written over where the parse read it.

## Context

- The parser held the tree-sitter node of each block's name and kept only its text. Navigation found the name by
  re-lexing the block with a first-line rule (`declaration_name`), and a method's by a second one (`method_name`).
- When a valid file writes the name on a later line than the kind keyword, the first rule misses. Definition, outline,
  find-references and the editor's symbols then answered the whole block. MCP `rename` refused with `internal_error`
  ("do not spell it where the graph says"), the LSP's prepareRename answered `null`, and an LSP rename started from a
  reference was refused.
- The same miss had been patched once for one kind (`4c532af7`, a spec block's rename) and left in place.
- A second defect shared the cause: the typed retitle of a spec block (whose title is its name) was refused as
  `block_unreadable`, blaming the parser for a valid file.

## Decision

- **D1.** The parse records where each block's name is written (`Entity::name_span`: the identifier, a ref's ID, a spec
  block's string with its quotes) and each method's (`MethodDecl::name_span`, never serialized). `None` means the parse
  read no name.
- **D2.** Navigation reads a declaration's name from the compile's parse of its file: the entity whose block is the
  node's, in the parses the finder reads (ADR 0176). The graph keeps no name position.
- **D3.** A declaration answers `Precision::Entity` only when the compile holds no parse or text of the file, the parse
  read no name, or the text does not spell the id at the name: a stale text, or a spec block's quoted name. It never
  answers `Entity` because of layout. A spec block's definition is its block, and prepareRename stays unavailable on a
  spec name.
- **D4.** The typed retitle reads the header's name from its block's own parse. A spec block's title is its name, so a
  retitle of it is refused `not_editable`.
- **D5.** Verify statements keep no span. `verify_lines` reads the block's canonical text, where the rule holds by layout. *(Amended, round 21: see the Amendment.)*

## Rejected

- A `name_span` on the graph's `Node`: it would make the graph a second record of where names are written, and every
  `Node` literal (53 in 27 files) would name it.
- Spans on verify statements: no defect needs them, `delta.rs` compares obligations by equality (a moved block would read
  as changed), and one literal lies in a file at its size cap. *(Withdrawn, round 21.)*
- A spec name's inner span: prepareRename would offer a rename that `rename::plan` refuses.
- Teaching the LSP's own syntax walk (ADR 0023) two-line headers: navigation now answers first.

## Consequences

- A header written over two lines answers its name with `precision: token` over MCP and LSP, and rename succeeds on it.
- `declaration_name`, `method_name` and the lexeme half of `SourceText` go: navigation lexes no block.
- One lookup per declaration answer (the files, then the file's blocks).
- The CLI's and MCP's refusal code for a spec block's retitle is `not_editable`.

## Reopen when

A pipeline check must point at a block's name (then the node carries the span), a diagnostic must point at one `verify`
statement, or the typed edits must read an unformatted block.

## Amendment (architecture round 21, plan 03)

- **D4, extended.** The typed retitle also reads the title from its block's own parse (`Entity::title_span`):
  it replaces the title the parse read, or writes one after the name. A union's string variant is never its title.
- **D5, reversed.** The parse records where each verify statement is written (`Entity::obligation_spans`, one per
  obligation, in order). Add-verify and remove-verify read them; `verify_lines` is gone. The premise "the rule
  holds by layout" was false: a block field's entry named `verify` stands on its own line in canonical layout too,
  and remove-verify deleted it, on a file `format --check` accepts.
- **The titles step** (ADR 0148 D4) writes a missing title after the name the parse read (`name_span`), on
  whichever line of the header it stands; `after_id` is gone.
- **The graph keeps none of these positions.** The spans are the parse's `Entity`'s, never `VerifyStatement`'s, so
  `Node` and `compute_graph_delta`'s obligation equality are unchanged. Rejected: a span inside `VerifyStatement`
  (it would ride onto every `Node`, and 11 literals would name it); retyping `Node::obligations` to the graph's
  spanless `Obligation` (no defect needs it).

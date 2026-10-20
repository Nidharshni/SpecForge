# A written token is found by one finder

**Status:** accepted (2026-10-19, architecture round 16, plan 06). Amends [ADR 0127](0127-the-linker-records-where-each-edge-is-written.md)
D5 (navigation's tokens ask the graph's finder; its holders still read `Graph::written`), extends
[ADR 0149](0149-a-file-states-a-kinds-shared-fields-once.md) D11 (a default no block takes is one token, held by none)
and [ADR 0156](0156-a-delete-finds-references-by-the-one-reference-rule.md) D1 and D2 (the referrer scan is the finder;
a derived type name refuses too), as ADR 0156's "Reopen when" foresaw. D2 amended by [ADR 0176](0176-the-finder-is-the-only-record-of-where-a-name-is-written.md): `Navigator::references` and the token at a position ask the finder too, which names the blocks that hold each token.
Amended again by the round 18 amendment below (plan 11, citing ADR 0176): the finder reads each block as the build stores it; the derived names one block links are one function.

"Which written tokens name entity X" was answered by two walks that disagreed both ways:

- Rename and the LSP read the graph's edges (`Navigator::tokens`). A `defaults` entry no block takes (W161) produces no
  node, no edge and no written span, so a rename left `invariants [inv_a]` behind, and the next block created in that
  file failed with an E003 the user never wrote. The editor's find-references missed it even asked from the token.
- Delete read the parsed texts with its own walk (`referrers`, `names`). It saw every `defaults` entry, and no type name
  a derived field reads: `delete User --unlink-references` silently left `owner User` dangling, or failed with a
  generic E080/E004 for a port signature.

## Decision

- **D1. One finder answers which tokens of a file name an entity.** `specforge_graph::written_tokens` reads a parsed
  file with the graph build's own pieces: the applied defaults (taken or not), each block's own entries coerced then
  `Reading::references`, and the derived walk's type expressions. It names who writes each token (`Holder`) and how
  (`Form`).
- **D2. Navigation's tokens ask it over the compiled parses.** Rename, the editor's find-references and LSP rename read
  `Navigator::tokens(id, include_declaration)`; `Navigator::references` keeps one holder per block from the edges (MCP
  `find_references` lists holders, so a default no block takes is not listed there).
- **D3. A delete's referrer scan asks it over the planned texts.** A token in a block's declared field is unlinked or
  unset; any other (a default, an undeclared field, a derived type name) refuses the delete as `not_editable`, naming
  `file:line (holder, field)`. A default entry that is not applied (E077, E078) names nothing.

Rejected: the graph recording the tokens no node holds (leaves delete's walk a second copy and grows the graph's delta
and subgraph copy for a fact derivable from the files); the finder in ops (the derived helpers are the graph's and ADR
0156 puts the coerce-then-ask pair there); delete through the navigator (ADR 0156); an `Option` holder on `Occurrence`;
a check for a default naming a missing entity.

## Consequences

MCP `specforge.rename` and the LSP's references and rename include a default no block takes. `delete --unlink-references`
refuses a type name a derived field reads. `Navigator::tokens` takes `include_declaration` and returns `Token`;
`CompiledProject::parsed` lends the build's parses. `prepareRename` on a default no block takes still answers `null`.

What would reopen it: a typed edit that writes `defaults` blocks or type expressions (then D3 edits instead of
refusing); a third walk over parsed files appearing outside the graph crate.

## The finder reads a block as the build stores it (amendment, architecture round 18, plan 11)

**Context.** The linker reads each node's entries after the build coerced them; the finder's derived walk read the
block's entries as parsed. A declared list field written as one bare word (`composed_types Customer`, `refs Customer`)
was a list to the graph and a type name to the finder, so `delete --unlink-references` refused where `delete` succeeded,
MCP `find_references` answered `[]` while `rename` edited the word, and the LSP answered `null` at the token yet renamed
it from the declaration. The only parity test ran over fixtures that wrote no such list.

**Decision.**
- D1 is sharpened: the finder reads each block's and each default's entries coerced once, as the build stores them, for
  its references **and** its derived names, so a list written as one value is a list (ADR 0156 D1). A `string_list`
  word is no reference and no type name.
- `derive::derived_names` answers which names a derived field links from one block; the linker makes each an edge and
  the finder a token.
- `graph/tests/agreement.rs` holds the two walks to agreement, in both directions, over the cases where their inputs
  differ on purpose (defaults taken or not, E002 and W060 losers, unresolved and mistyped references, the block's own
  name). It reads the blocks that hold each token from `held_by` ([ADR 0176](0176-the-finder-is-the-only-record-of-where-a-name-is-written.md)).

**Rejected.**
- The linker reading the finder's walk: the two read different inputs by design (merged, coerced nodes with the taken
  defaults against every parsed block, losers included, and each default once); every other output reads node fields, so
  edges equal what the nodes say; diagnostics come in node order; the finder would add a second `FileDefaults::of`, a
  sort and a `Vec` to every compile; and it would not have caught this defect, which is in the finder's input.
- A guard in `derive::expressions` that skips what a reading would coerce: it would rely on a fact about `coerce`'s
  outputs that `expressions` cannot see.
- A debug assertion in the build that runs the finder: the cost lands on every debug compile and the build holds no
  parsed files at that point.

**Reopen when.** The linker stops reading node fields (then one per-block reader is free), or a third reader of a
block's references over parsed files appears.

ADR 0181 was reserved for this decision and is unused.

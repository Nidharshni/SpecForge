# A written token is found by one finder

**Status:** accepted (2026-10-19, architecture round 16, plan 06). Amends [ADR 0127](0127-the-linker-records-where-each-edge-is-written.md)
D5 (navigation's tokens ask the graph's finder; its holders still read `Graph::written`), extends
[ADR 0149](0149-a-file-states-a-kinds-shared-fields-once.md) D11 (a default no block takes is one token, held by none)
and [ADR 0156](0156-a-delete-finds-references-by-the-one-reference-rule.md) D1 and D2 (the referrer scan is the finder;
a derived type name refuses too), as ADR 0156's "Reopen when" foresaw. D2 amended by [ADR 0176](0176-the-finder-is-the-only-record-of-where-a-name-is-written.md): `Navigator::references` and the token at a position ask the finder too, which names the blocks that hold each token.

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

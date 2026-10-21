# The finder is the only record of where a name is written

**Status:** accepted (2026-10-20, architecture round 18, plan 06). Supersedes [ADR 0127](0127-the-linker-records-where-each-edge-is-written.md) D1, D2, D5 and D7 (the graph records no written span; navigation reads the finder; debug builds assert every edge is held by a token) and keeps its D3, D4 and D6. Amends [ADR 0163](0163-a-written-token-is-found-by-one-finder.md) D2 (`Navigator::references` and the token at a position ask the finder too; the finder names the blocks that hold each token), [ADR 0016](0016-navigation-is-one-module.md) D2 (an occurrence is a written token the finder reads, held by each block whose node holds it) and [ADR 0149](0149-a-file-states-a-kinds-shared-fields-once.md) D11 (the holder per block is the finder's `held_by`). D2 amended by [ADR 0182](0182-the-graph-answers-what-a-blocks-field-holds.md): a default's tokens are held by the blocks that take the default's entry. Complemented by [ADR 0187](0187-a-declarations-name-is-where-the-parse-read-it.md): a declaration's name is read from the same parses, by navigation.

## Context

- "Where is this name written" had two records. The linker kept spans parallel to its edges (`Graph::written`, ADR 0127);
  a question by position (LSP prepareRename, hover, definition) and MCP `find_references` read them. The graph's finder
  over the parses (`written_tokens`, ADR 0163) answered a question by id (rename, the LSP's references and rename,
  delete).
- They disagreed on two shapes users write. On a default no block takes (W161), prepareRename answered `null` where
  references and rename act. On a reference inside a duplicate declaration (E002, W060), prepareRename answered `null`,
  MCP `find_references` omitted the token and MCP `rename` edited it.
- Over the repository's own spec (2,213 entities, 6,011 edges) the two records agree on all 8,437 locations: the second
  was redundant, and kept index-aligned by hand, compared under verify and asserted in debug builds.
- Round 16 weighed holders from the finder and rejected them: the finder would re-resolve defaults per block. Which blocks
  take a default is `FileDefaults`' rule, not the linker's, and the finder already builds it.

## Decision

- **D1.** The finder answers every "where is this name written" question: the tokens of an id, the references to it,
  the token at a position, a delete's referrers.
- **D2.** `WrittenToken::held_by` names the blocks that hold a token: its block; every block of the kind in the file that
  does not write the field, for a default (`defaults::takers`, the rule `resolve` and W161 state); never the block a
  default's derived type name names (a block derives no edge to itself); none for a default no block takes.
- **D3.** `Navigator::occurrence_at` answers a `Token` carrying the id it names, not an `Occurrence`: no caller reads a
  holder or a role there.
- **D4.** The graph keeps edges only. `Graph::written`, `add_written`, the verify compare of where edges are written and
  `unwritten_edges` go. `Edge` stays `Copy`.
- **D5.** Debug builds assert one way: every edge is held by some token. The reverse is not an invariant: an E002 or W060
  loser's reference is a token with no edge, on purpose.
- **D6.** The finder's `kind_of` closure stays: navigation and a delete over planned texts are two adapters.
- **D7.** A default no block takes stays out of MCP `find_references` (it has no holder); rename and the LSP's references
  include it. No schema change.
- **D8.** A reference written in a duplicate declaration is listed, held by the block's id.

## Rejected

- Spans on `Edge` (ADR 0127's reason stands); keeping `Graph::written` as a cache (two records again); the fan-out in
  navigation; `FileDefaults::holders` (the derived self-rule in a module that knows no derived fields); a `GraphBuild`
  method for the finder; an `Option` holder on `Occurrence`; listing a default no block takes in MCP.

## Consequences

- prepareRename answers on every token rename edits. MCP `find_references` lists a duplicate declaration's references.
- `find_references` walks every compiled file per call (about 40 ms in a debug build over the repository spec).
- About 210 lines go, the parallel vector and one of three guards among them.

## Reopen when

A project where `find_references`' walk is slow (then an index of the finder's tokens per compile), or an out-of-process
consumer of positions (ADR 0127 D6).

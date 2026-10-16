# Navigation is one module, answering in spans

**Status:** accepted (2026-10-05). D1's parenthesis is reopened and D3's `Entity` wording amended by [ADR 0116](0116-navigation-reads-the-text-the-project-was-compiled-from.md): the navigator
reads its view's compiled text. D2's sources of a token are refined by [ADR 0127](0127-the-linker-records-where-each-edge-is-written.md): the token is the graph's written span, recorded by the linker; the block scan is gone.

The LSP and MCP answer the same five questions about the same project: where an entity is declared,
what references it, which entities match a text, which diagnostics are about an entity, and how a
diagnostic is fixed. Each surface had its own answer to each, and they disagreed:

- **"References" had four meanings.** LSP `textDocument/references` returned the declaration plus
  the entities on *both* ends of the entity's edges, ignored `includeDeclaration`, and answered
  whole entity blocks (VS Code's CodeLens, which asks without the declaration, counted 2 references
  to an entity referenced once). MCP `find_references` returned incoming edges as blocks; MCP
  `search references=` returned incoming edges in a third shape and skipped every other filter;
  MCP `inspect.references` mixed both directions in one unlabeled list. a7fef582 changed how the
  LSP read edges, not what it answered.
- **Fixes.** The LSP turned an E003/E025's data into a replace edit (219bec59, 87fdf33e); MCP
  `suggest_fixes` returned the suggestion text with `"edits": []`, always, though its spec type and
  contract promised edits and "the same quick-fix suggestions an IDE offers".
- **Attribution.** MCP decided which diagnostics belong to an entity by span (lines only) or, for a
  spanless one, by finding `'<id>'` in the message (dc77654d, reused by 5393e24f), against
  CONTEXT.md's rule that no consumer parses the message. The one spanless diagnostic about entities
  the compiler raises, W061 (a reference cycle), quotes no id, so `inspect` never listed it.
- **Ranking.** MCP search kept Jaro-Winkler above 0.6 over ids and titles (c714ca41 added contract
  text): the typo `sesion` matched 266 of this repo's 1,998 entities. LSP completion ranked prefix,
  substring, then JW ≥ 0.7 over ids only; workspace symbols matched substrings only. The spec said
  search "MUST use the same algorithm as LSP workspaceSymbol".
- **Rename** (06c1bcf5 shared its plan) still found its edits by scanning each block's text for the
  id, so it rewrote titles, guarantees, comments and `verify` texts; a changed verify text silently
  unlinks the tests that name it. prepareRename answered the whole block.

## Decision

**D1. One module.** `specforge_ops::navigate`, beside `rename`, over the `ProjectView` (ADR 0015) and
each spec file's text (`Navigator::new(view, text_of)`, a file read once per navigator: the LSP reads
its open buffer first, MCP the file under the spec root). Its interface is small: `definition`,
`references` (with a direction and the declaration on request), `occurrence_at`, `find_entities`,
`subjects`/`is_about`, `fixes`, `match_file`/`entities_of_file` and `outline`. The LSP is an adapter
that converts spans to UTF-16 ranges (`crates/surfaces/lsp/src/navigation.rs`), MCP one that renders JSON
(`tools::navigator(&Call)` over the call's view). `specforge-graph` could not host it (it has no
registries, which stub kinds, verify kinds and derived fields need).

**D2. A reference has one meaning**: an occurrence (the token as written) of an entity's id in
another entity's field that resolves to it, one edge of the graph. The token comes from data the
graph already holds: a reference-list item's `SpannedRef` span, a single reference's value span, the
names in a derived field's type expressions or method signatures; only for a graph of unknown
provenance a scan of the holder's block outside strings and comments. References *to* an entity are
incoming; what it *refers to* are outgoing; the declaration (its name token) only on request.

**D3. Answers are `SourceSpan`s** (1-based lines, 1-based byte columns, end exclusive: the parser's
convention, what E003 already points at) with a **precision**: `Token` when the text spells the id
there, `Entity` (the holder's block) when the text is unreadable or stale. The degraded case is
visible instead of silent, and rename refuses rather than guesses.

**D4. One ranking**: exact, prefix, substring, field text (search only), then Jaro-Winkler
similarity of at least `FUZZY_THRESHOLD = 0.80` over the lowercase id and title; within a tier the
closer first, then the id; filters before ranking, the limit after; scores are tier bands (1.0, 0.9,
0.8, 0.7, then 0.6 × similarity). At 0.8 `sesion` finds `session_limit` alone and `user_lgon` still
finds `user_login`. Completion and workspace symbols match names, MCP search names and string fields.
`find_close_match` (did-you-mean) is a correction, not a search, and keeps its stricter rule.

**D7. Attribution reads data, never the message.** A diagnostic is about the entities its data
names (`DiagnosticData::entities`: an unresolved reference's holder, a cycle's path, a pass
diagnostic's subject) that the graph holds, else the innermost entity whose block holds its span by
line and column, else none (a project-level diagnostic: E028, E045, E046 without a core, I098, W098,
W144). Two payloads are new: `ReferenceCycle { path }` on W061 and `Subject { entity }` on an
extension pass diagnostic that names its entity and carries no data (the name used to be dropped once
its span was borrowed, losing a diagnostic about an entity the graph lacks). The LSP publishes a
spanless diagnostic about entities at the first one's name, with related information at the others'.

**Fixes** are edits read from a diagnostic's data or the graph: an unresolved reference or import
with a close match becomes the match at its token (inside the quotes for an import); an unresolved
reference to an id no entity has, in a field targeting a kind, gets a stub of that kind at the end of
the file; an entity of a kind that takes verify statements and has none gets a verify stub inside its
block. Both surfaces offer the same fixes, filtered by entity, file, code and range.

## Consequences

- LSP references are incoming only, honor `includeDeclaration` and are tokens: VS Code's CodeLens
  shows the true incoming count. Definitions are a `LocationLink` (block, name selected) for a
  client declaring `linkSupport`, else the name; prepareRename answers the token under the cursor.
- MCP `find_references` keeps its keys; `source_span` is the token, one location per occurrence; it
  adds `referenced_entity_id`, `field`, `role`, `precision` and the arguments `direction` and
  `include_declaration`. `find_definition`'s `line`/`column` are the name's; it adds `source_span`,
  `name_span`, `precision`. `search`'s `references` is ANDed with the other filters; its scores are
  tier bands with `match_field`. `inspect` adds `referenced_by` and `refers_to` and kept
  `references` and `reference_count` as deprecated aliases until ADR 0048 D7 removed them;
  its diagnostics carry their `suggestion`. `suggest_fixes` returns the LSP's fixes with real edits;
  a diagnostic whose data names no fix contributes none. `outline` adds `name_range`.
- The LSP hover's outgoing heading is "Refers to"; workspace symbols are fuzzy and ranked;
  completion matches titles too and its cutoff rises 0.7 → 0.8; code actions are those overlapping
  the requested range; document symbols nest methods for a hierarchical client.
- Rename edits exactly the declaration and the references: in the `rn` fixture 2 edits instead of 6.
  Titles, comments and verify texts that mention the id keep the old name (the spec says so).
- A third-party spanless diagnostic that only *quotes* an id is no longer attributed to it: an
  extension should name its `entity`, which now survives as `Subject`.
- `navigation_parity.rs` (specforge-cli) asks both surfaces the same questions; its
  `EXPECTED_DIVERGENCES` table is empty and stays as the guard.

## Rejected

- **A `specforge-navigate` crate**: one more crate for `specforge-ops`'s dependency set.
- **Keeping block spans**: an agent then edits the wrong place, and the spec types already say where
  a reference is.
- **Keeping message parsing for third-party spanless diagnostics**: extensions pass `entity`.
- **0-based byte columns** (the `RenameEdit` convention) as the answer: a third convention is how
  off-by-ones happen. `RenameEdit` converts at the rename boundary only, so MCP's rename JSON is
  byte-identical.
- **Prose edits behind a rename flag**: nobody asked for it, and it reintroduces the scan.

## What would reopen it

An editor-only navigation need with no MCP counterpart (it would live in the LSP adapter, as
`use`-path definitions do), or a kind of reference that has no token (it would come back with
`Precision::Entity`).

ADR 0010 is unaffected: the only new dependency of `specforge-ops` is `strsim`, which is pure, and
`cargo tree -p specforge-lsp -e normal | rg -c "reqwest|keyring|ed25519"` finds none.

**Amended by ADR 0086 (2026-10-14):** navigation is one module at `specforge_ops::read::navigate`, beside `authoring::rename`; `not_found` and `EntityField` are the project view's.

# A file states a kind's shared fields once

**Status:** accepted (2026-10-18, syntax round, plan 03). Builds on [ADR 0127](0127-the-linker-records-where-each-edge-is-written.md) D3's rule (own, else the default) is answered by `FileDefaults::held` ([ADR 0182](0182-the-graph-answers-what-a-blocks-field-holds.md)).
(written spans), [ADR 0032](0032-one-graph-build.md) (the graph delta) and [ADR 0084](0084-a-crates-layer-is-its-folder.md) (layers). Amended by [ADR 0154](0154-a-typed-edit-asks-its-files-defaults-over-the-text-it-plans.md) (D8: the graph's defaults module answers a typed edit too: which default applies, and whether a field restates it). D11 amended by [ADR 0156](0156-a-delete-finds-references-by-the-one-reference-rule.md): navigation answers each token once (`Navigator::tokens`); rename and the editor ask it. D11 extended by [ADR 0163](0163-a-written-token-is-found-by-one-finder.md): a default no block takes is one token, held by none; rename and the editor see it. D11's holder per block is the finder's `held_by` ([ADR 0176](0176-the-finder-is-the-only-record-of-where-a-name-is-written.md)). D3 amended by [ADR 0175](0175-a-typed-edit-changes-a-blocks-field-through-one-module.md) (`{}` says none for a block field; a typed edit copies an inherited block field before it changes an entry). D12 amended by [ADR 0202](0202-a-blocks-doc-comment-is-where-the-parse-read-it.md): the hoisted block goes above the first form's doc comment, never above the format header. D12 amended by round 22 plan 04 ([ADR 0186](0186-a-migration-is-judged-before-it-is-written.md) D7): the hoist asks `FileDefaults` under the environment's field forms, so the graph comparison is the safety net, not the proof.

A block that shares its references with its neighbours restates them: 1,511 tracked lines repeat word for word inside
one file, 948 of them reference lists. `defaults behavior { ... }` already parses, as an entity of the unknown kind
`defaults` named `behavior` (E024, E013), and the blocks it was meant for lose the fields.

## Decision

- **D1. Form.** `defaults <kind> { <field> <value> ... }` at the top level of a file. No grammar change: it is a
  generic entity block. It takes no title, no `verify`, no `method`.
- **D2. Scope.** Every block of `<kind>` in the same file; no other kind, file or importer.
- **D3. Replace.** A block that writes a field keeps its own value whole. `ports []` says "none".
- **D4. Declared fields only (E077).** A default sets a field the kind declares. An undeclared field, an undeclared
  kind or a structural kind is E077 and is not applied. A structural-only project checks nothing.
- **D5. One per kind, at the top (E078).** A second block for a kind is not applied; a block after the file's first
  entity block still applies.
- **D6. Nothing dead (W161).** A block field equal to its default, and a default no block takes.
- **D7. Reserved.** `defaults` is a structural keyword: never a kind, always E013 as an id.
- **D8. The graph build resolves it** (`specforge_graph`), not the parser: the parser has no registry, the formatter
  and the LSP read the written form. A node holds the resolved value, so every read view, export and check sees it.
- **D9. An inherited entry keeps the default's spans.** Diagnostics, references, rename and go-to point at the line
  where the value is written. An entry is inherited when its value span lies outside its node's source span. No flag.
- **D10. Diagnostics about an inherited value repeat per inheriting block** (each names its holder).
- **D11. find-references keeps a holder per block, the editor shows one location, rename edits once.**
- **D12. `specforge migrate` hoists, not the formatter.** The step moves a kind's most common one-line bare-item list
  when every block of the kind writes the field; the migrate graph comparison proves the graph is unchanged.
- **U1..U5.** Per-kind form; any declared field; hoist the most common bare-item list; before the first block; the
  round's KPI is 1,511 to at most 1,160 repeated lines and 948 to at most 600 reference lines.

## Consequences

E077, E078, W161; `structural::DEFAULTS`; `FieldForms::kinds()`; LSP reads a defaults header as the kind it names; a
migrate step; the next block written in a file inherits values nobody sees in it (every view shows the resolved list).

## Rejected

- A file-wide `defaults { }`: a grammar rule, and `features` means five different relations across kinds.
- Merging lists: a content decision. A stored `inherited` flag or an export mark: the span already says it.

## What would reopen it

A second scope (project-wide defaults) or a measured need for merging.

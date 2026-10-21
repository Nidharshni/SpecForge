# The graph answers what a block's field holds, and the value the build stores

**Status:** accepted (2026-10-21, architecture round 19, plan 05). Amends [ADR 0149](0149-a-file-states-a-kinds-shared-fields-once.md) D3 (the rule "own, else the default" is one function), [ADR 0154](0154-a-typed-edit-asks-its-files-defaults-over-the-text-it-plans.md) D3 (what an edit removes is judged on stored values), [ADR 0156](0156-a-delete-finds-references-by-the-one-reference-rule.md) (its reopen case is met), [ADR 0163](0163-a-written-token-is-found-by-one-finder.md) ("coerced once" is `FieldForms::stored`), [ADR 0175](0175-a-typed-edit-changes-a-blocks-field-through-one-module.md) D1 and D4 and [ADR 0176](0176-the-finder-is-the-only-record-of-where-a-name-is-written.md) D2.

## Context

"What a block's field holds" has two halves, and each was restated by its readers.

- *How the build stores a value* (coerced by its field form) was spelled four times: the build's coerce pass, the
  finder's `stored`, the typed edit's `copy` and `link`'s own-entry coerce. ADR 0156 foresaw it: "or a reader of parsed
  entries besides delete appears (then the coerce-then-ask pair belongs in the graph)".
- *Whether the block holds its own value, its file's default or nothing* was spelled four times: `resolve`, `takers`,
  W161's own-field lookup and `edit::defaults::change`.

Three of the restatements disagreed with the build, all reproduced on `main` @ `5956806d`:

- The finder held a default's derived type name by the blocks that do not write the *derived* field, not by those that
  take the default's *entry*. A debug build panicked (an edge no token holds) and `find_references` named a holder with
  no edge.
- `unlink` of a single reference the block inherits answered "nothing to write" (exit 0) while the graph held the edge;
  `unset` of the same field refuses.
- Over a default written as one value, `set` to its value, or a `link` and its `unlink`, left an own copy
  the graph stores identically; the edit reported the W161 it caused and no typed edit returned the block to the default.

Six fixes in two days each repaired the reader that was found broken; none moved the rule. Round 18's open item O2,
`FieldForms::reading` allocating two `String`s per call, is measured and closed here.

## Decision

- **D1.** `FieldForms::stored(kind, entry)` is the one answer for a value as the graph build stores it. The build's
  coerce pass, the finder, a typed edit's copy and `link` ask it; `Reading::coerce` goes.
- **D2.** `FileDefaults::held(block, field)` answers own (with the default it replaces), inherited or absent, as written.
  `resolve`, `takers`, W161 and `edit::defaults::change` ask it. A default's tokens, its derived names included, are
  held by the blocks that take the default's *entry* (amends ADR 0176 D2, which said "does not write the field").
- **D3.** A typed edit's verb is handed that answer. A value the module does not copy (one value) arrives inherited, not
  absent: `unlink` of an inherited single reference that names the target empties the field and is refused as `unset`
  is (amends ADR 0175 D1 and D4).
- **D4.** A typed edit removes an own field that holds what its default holds as the build stores both
  (`FileDefaults::repeats`). W161 keeps judging written values (amends ADR 0154 D3 for the edit; its Rejected entry
  stands). ADR 0175 D5's way back, `set` to the default's value, then holds for a default in any form.
- **D5.** `FieldForms` looks a field up by `&str`, without allocating (two allocations per call before).

## Rejected

- One `field_of(file, block, field)`: the build coerces every entry of every node in one pass and the finder reads each
  default once, not once per taker; the two halves have different shapes.
- W161 on stored values: it changes a published warning for every block that writes a list over a one-value default.
- Coercing at placement instead of `Graph::coerce_field_values`: changes what incremental rebuilds and hand-built nodes see.
- A key field on `WrittenToken`; `(Sym, Sym)` keys for the forms (the interner costs a lock and grows).

## Consequences

- CLI/MCP `unlink` of an inherited single reference exits 1 `not_editable` (was 0, "nothing to write").
- `set`, `link`, `unlink` and `new` over a one-value default no longer leave an own copy.
- MCP `find_references`, rename and the LSP name the right holders of a default's derived name; debug builds stop
  panicking on such a file.
- Rust: `specforge_graph::Reading::coerce` goes; `Held`, `FileDefaults::held`, `FileDefaults::repeats` and
  `FieldForms::stored` are added.

## Reopen when

A second defaults scope exists, or W161 moves to stored values (then `restates` and `repeats` are one).

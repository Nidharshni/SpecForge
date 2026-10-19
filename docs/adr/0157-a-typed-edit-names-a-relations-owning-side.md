# A typed edit names a relation's owning side

**Status:** accepted (2026-10-19, architecture round 15, plan 05). Amends [ADR 0150](0150-a-relation-is-written-on-one-side.md)
(its consequence for the typed edit's `link`) and [ADR 0151](0151-a-spec-file-is-changed-by-a-typed-edit.md) (D2: how a list
value is taken).

ADR 0150 made writing a relation's other side E079 and said the typed edit's `link` would use
`FieldRegistry::owning_fields`. It never did: the edit refuses a field its kind does not declare before anything
is compiled, so `specforge link checkout behaviors pay` was refused "kind 'feature' has no field 'behaviors'"
while `check` on the same text named `behavior.features` and the edit to make. `owning_fields` itself named its
owners in hash order, so E079's message changed between runs. How a list value is taken was decided three times:
the writer split one text at commas for a reference list only, `specforge new` split reference lists at the
command line and kept a scalar flag's last value, and `set`'s help said every list splits.

## Decision

- **D1.** An edit naming the other side of a relation (an undeclared field `owning_fields` answers for) is
  refused `other_side`, naming the owning fields in E079's words and suggesting the change on the owning side.
  A link or unlink also tells the side by its target's kind. The edit is not rerouted: an edit changes
  the entity it names.
- **D2.** A list value is taken by its field's type alone: a reference list's items are split at commas however
  they are given, a string list's never are; a field of one value takes exactly one. `specforge new` passes each
  flag's values as given, as `set` does.
- **D3.** `owning_fields` answers each group (the `inverse_of` matches, then the value kind's fields) in
  (kind, field) order.
- **D4.** What a block of kind K may hold stays the registry's existing answers (the kind's declared fields,
  `FieldForms::reading`, `owning_fields`), asked by the edit. There is no kind-vocabulary module.

## Rejected

- Routing a link to the owning side: the edit would change an entity it does not name, two owners make it
  ambiguous, and `set` and `create` would still need the refusal.
- A registry-level module for a kind's vocabulary: LSP completion lists fields by label, E079 already asks
  `owning_fields`, schema and infer-guide list by name; its one real caller is the edit.
- A diagnostic code for the refusal: E079 stays the compile's; a refusal carries no diagnostic.

## Consequences

`specforge link <id> <field> <target>` on the other side fails with `other_side` and says where to write it. A
scalar flag given twice to `new` is refused. `--path` after the kind in `new` acts like `--path` anywhere else.

## What would reopen it

A second reader of a kind's written order outside the edit (an LSP block scaffold in written order), or a
decision to route links to their owning side.

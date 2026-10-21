# A typed edit asks its file's defaults over the text it plans

**Status:** accepted (2026-10-19, architecture round 15, plan 02). Amends [ADR 0149](0149-a-file-states-a-kinds-shared-fields-once.md) D8 (the graph's defaults module also answers the edit path) and [ADR 0151](0151-a-spec-file-is-changed-by-a-typed-edit.md) D5 (how "copied" and "equal to the default" are decided). D3 amended by [ADR 0175](0175-a-typed-edit-changes-a-blocks-field-through-one-module.md) (every typed edit of a field, entries included, ends with the restatement check, in one module). D3's create sentence amended by round 19 plan 02 (ADR 0175 D7): a create removes a restating field as `set` does, one field at a time. D3's "judged on the written value" amended by [ADR 0182](0182-the-graph-answers-what-a-blocks-field-holds.md) for typed edits, create included (ADR 0175 D7): an edit removes an own field that holds what its default holds, as stored; W161 still judges written values.

## Context

`FileDefaults` was `pub(crate)`, so the typed edits answered "what does this block inherit, and does this value restate
it?" with their own code (the syntax round's plan 05 records why: "plan 03 adds no accessor"). Four answers existed
and one was wrong:

- W161 and `set` compare how the own and the default value are written (`field_form::write`);
- `link` and `unlink` compare the items' texts, which gives the same verdict;
- `create` compared the caller's input with the default value: an MCP create of a `string_list` item `"a, b"` over a
  default `["a", "b"]` was left out (the input was split on commas, the writer does not split a `string_list`), and an
  integer equal to its default was written, so the edit's own output was W161;
- `link` read what a block inherits from the compiled graph's node, which does not know a block created, or a field
  unset, earlier in the same edit list: a link after a create lost the default's items, a link after an unset started
  from the value just removed (and silently dropped a target already in it).

## Decision

- **D1.** The graph's defaults module is the one answer: `specforge_graph::FileDefaults::entry(kind, field)` says which
  default applies (E077 and E078 honoured), `restates(kind, own)` says whether an own field restates it. W161 asks the
  same function.
- **D2.** An edit reads the defaults from the text it is planning, never from the compiled graph.
- **D3.** "Restates" is judged on the written value. A create writes every field it is given, lays the block out, then
  removes each field that restates its default; `set`, `link` and `unlink` end the same way. The defaults never see an
  `Input`, so the input shape (split a `reference_list` on commas, never a `string_list`) is decided by the writer
  alone.
- **D4.** What a block inherits, for `link`, is the default as the graph stores it: the entry coerced by the field's
  form (`FieldForm::coerce`), so a default written `refs "a"` is the list `["a"]`.

## Rejected

- Fixing `is_default` in place (mirror the writer's comma rule, parse integers): a third spelling of the writer's input
  shape and a second copy of W161's comparison; it leaves the stale inheritance.
- A defaults module in the registry: the registry has no business with a file's text.
- Keeping `Site::resolved` and falling back to the text: two sources for one fact.
- W161 on stored (coerced) values: a change of a published warning, not of who asks it.

## Consequences

`FileDefaults` is public. `Site` loses `resolved`, `Located` loses `node`, ops loses `default_entry` and `is_default`.
A create writes then removes, which is one more format pass per given field and per link. One difference between stored
and written equality remains: a default in a list field's single-value form (`refs "a"`) is the list `["a"]` in the
graph, but a block's `refs ["a"]` is not W161, so a create writes it.

## What would reopen it

A second defaults scope (project-wide), or W161 on stored values.

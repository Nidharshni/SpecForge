# A value is spelled by one module

**Status:** accepted (2026-10-19, architecture round 15, plan 01). Amends [ADR 0147](0147-a-value-is-written-in-its-fields-one-form.md) D1 (a scheme ref id on a reference field is bare) and D4 (the rule is read from one module, the typed-edit writer reads it too) and [ADR 0151](0151-a-spec-file-is-changed-by-a-typed-edit.md) D2 (a multi-line value is laid out with the file's format configuration).

## Context

ADR 0147 D4 gave one rule the decision of how a value is written. The syntax round then spelled values in five modules
on one day: the typed-edit writer re-stated the bare rule and escaped text with its own table; the formatter's
`in_form`, W159's message and migrate's `titles` step each quoted a value; the CLI kept a copy of `is_word`. The
merge was planned (the typed-edit plan said the formatter's rule should call the writer's) and not done.

The copies had started to disagree. The writer wrote `true` bare on a reference field, which the compile reads as a
boolean, where the rule never writes `true` bare. Separately, the writer laid a triple-quoted value out with a width of
100 and an indent of two spaces, so in a project whose `.specforgefmt.toml` says `use_tabs`, `indent_width = 4` or
another `max_width`, a typed edit left a multi-line value indented with spaces, off the indent unit, or with its closing
quotes left of its key. `format --check` cannot see it, because the formatter keeps triple-quoted text verbatim.

Not every choice the writer makes is redundant after the touched block is formatted: of its nine table rows `format`
would make two (an unquoted word, an unquoted reference id). The rest (a string's one-line or triple form, an integer,
a scheme ref, a member type, a block's entries) it would not.

## Decision

- **D1.** One module spells a value: `specforge_graph::field_form::spell` holds the escape table (`quoted`), the bare
  rule (`holds_bare`) and each declared type's one-line spelling (`value`). `FieldForms::rewrite` reads `holds_bare`;
  the typed-edit writer, `format`, W159's message and migrate's `titles` step spell through it; no other code quotes a
  `.spec` value. A test holds every spelled value to read back and to be left as it is by `rewrite`.
- **D2.** The typed-edit writer lays a multi-line value out with the file's format configuration (ADR 0021): a text goes
  triple-quoted when it holds a line break or its one-line form passes the configuration's `max_width`; its lines are
  one indent of the configuration past the key's, its closing quotes at the key's.
- **D3.** On a `reference` or `reference_list` field a scheme ref id is written bare, as a word is: `format` unquotes it
  and W159 reports it quoted. Elsewhere the bare rule is unchanged (a word, never `true` or `false`).

## Rejected

- The writer writing every scalar quoted and leaving the form to `format`: seven of nine rows stay, and the writer's
  output would depend on a later format pass and on its forms being loaded.
- The speller in the formatter (the registry cannot reach it, so W159 stays a copy) or in the parser (the bare rule needs
  the field form, which is the graph's).
- A graph-side layout type for the writer's one multi-line caller: it would mirror `FormatConfig` for one caller.
- A CLI dependency on the graph for `is_word`: a new crate edge for seven lines; `Obligation::parse` in ops is the seam.

## Consequences

A typed edit never produces W159, and a multi-line value follows `.specforgefmt.toml`; with the defaults the output is
byte for byte what it was. `format` unquotes a quoted scheme ref id on a reference field (no tracked value changes: every
builtin reference field declares a target kind, so a scheme ref there is already E022).

## What would reopen it

A second code path that writes multi-line values (an LSP code action, a migration step that writes text values): the
layout half would then move beside `spell` with a configuration type.

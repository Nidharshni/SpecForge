# A value is written in its field's one form

**Status:** accepted (2026-10-18, syntax round, plan 01). Amends [ADR 0064](0064-a-declared-field-type-decides-one-wire-form.md) D2 (the admission table gains `identifier`), [ADR 0093](0093-a-fields-declared-type-decides-what-its-value-references.md) D3 (a quoted id still references; it is no longer the written form, W159) and [ADR 0021](0021-formatting-is-one-operation.md) (format reads the project's declarations; a value's quotes are layout of the declared type). Amended by [ADR 0153](0153-a-value-is-spelled-by-one-module.md) (D1: on a reference field a scheme ref id is bare too; D4: `rewrite` reads the speller's bare rule, which the typed-edit writer reads too).

The grammar takes a field value quoted or bare, the graph stores both the same way, and neither `check` nor `format`
says which is right, so the corpus holds both. The cause is in the declarations: ten fields whose values are words
from a set (`category`, `risk`, `family`, `direction`, ...) are declared `string`, the type of a contract's prose, so
no rule keyed on the declared type can say "bare here, quoted there". Measured with the parser: 952 tracked values
are written in the other form than a declared-type rule would give them; after the retype below, 8.

## Decision

- **D1. The form table.** A value's form is decided by its field's declared type. `enum`, `identifier`, `reference`
  and each `reference_list` item are bare; `string` and each `string_list` item are quoted; `integer`, `bool` and
  `block` have one spelling. A bare-form value that is not a word (a path) or is `true`/`false` is never unquoted.
- **D2. An `identifier` field type.** One word from an open set (letters, digits and `_`, starting with a letter or
  `_`), written bare. A quoted non-word on it is E061. It is the open counterpart of `enum`, which cannot be reused
  for an open set (an enum without values is W021).
- **D3. The retyped fields.** Eight open tag fields become `identifier` (`behavior.category`, `.status`, `.severity`,
  `.diagnostic`; `event.category`, `.channel_type`; `constraint.category`; `module.family`). `invariant.risk` and
  `port.direction` become enums of the closed sets the extension's own spec models (all 189 tracked values fit).
  `port.category` (slash paths) and `event.channel` stay `string`.
- **D4. One rule.** `FieldForms::rewrite` in `specforge_graph::field_form` answers "write this value the other way".
  `check` reads it for W159, `format` for the rewrite; nothing else decides a form.
- **D5. Member kinds.** On a kind that takes members (`type`, `port`) a bare word is left alone: it may be a member's
  type.
- **D6. Format reads declarations.** `format` loads each project's extensions to read the forms; without a runtime or
  outside a project it rewrites nothing.
- **D7. The spec header keeps its quoted name.** `spec "shop"` is a display title that nothing references.

## Rejected

- Closing every tag into an enum: the extension spec's unions do not cover the corpus and every user's own tag would
  become an error.
- Quoting all 952 words: the opposite of the form words and references already have.
- Deciding by the value (a single word bare, anything else quoted): one `string` field would hold two forms.
- A `--forms` flag: the canonical form is not optional.

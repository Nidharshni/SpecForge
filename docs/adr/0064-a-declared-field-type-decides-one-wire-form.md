# A declared field type decides one wire form

**Status:** accepted (2026-10-13, architecture round 8, plan 01). Amends ADR 0034, ADR 0051 (D1, Rejected),
ADR 0040, ADR 0007 ("Where things live") and ADR 0031 (the value shape's home). Amended by [ADR 0094](0094-obligations-are-an-entitys-own-member.md) (D5 restored) and extended by [ADR 0093](0093-a-fields-declared-type-decides-what-its-value-references.md) (D1 reaches what a value references).

A field's declared type was read four ways. E061 refused only scalars on integer, bool and enum fields and lists
on single-value fields, and admitted everything else. The coercion table sat in the registry and its
application in the graph. The JSON writer ignored the type and serialized the parser's expression tree, spans
included, onto the Graph Protocol, MCP query and every extension command's input. The published JSON Schema was
300 hand-written lines of `if`/`then` that the writer never read. The builtins declared types their own specs and
passes contradicted. This repository's export failed its own published schema with 1091 errors while `check`
reported none; the default context and brief exports failed theirs too.

## Decisions

- **D1. One module reads a declared type** (`specforge_graph::field_form`): `FieldForm::coerce` (how the graph
  build stores a written value), `FieldForm::admit` (which stored values the field takes; E061), `write` (the
  JSON every output writes for any field value; `FieldJson`) and `FieldForm::json_schema` (the schema of what
  `write` writes for an admitted value). A property test holds them together: a value E061 admits is a value the
  published schema admits. The graph crate depends on `specforge-protocol-types` for `FieldType`.
- **D2. The admission table.** string: text, a bare word, a date, a type union, an expression group; enum: a
  declared value; integer; bool; reference: one id; string_list and reference_list: a list; block: a block. A
  list's every written scalar and a mixed list of scalars are stored as a list of strings.
- **D3. An expression group is written as its conjuncts, one per line**: the string form a proof-role field
  already takes (ADR 0009). No field type is added.
- **D4. A parsed value has no serde form.** Outputs write through `write`; the graph delta compares written
  values (a quoting-only edit is no field change); the build's incremental check compares parsed values by
  equality.
- **D5. The published schema is the export document's own schema, narrowed by the project**: a node branch per
  kind an export can hold (declared kinds, `ref`, `spec`) whose fields are the field forms' schemas, the
  registered edge type labels, one `$id` per format. It uses only keywords `shape::violations` reads plus
  annotations (`shape::unchecked_keywords` is empty), so the in-repo checker validates it exactly; a test
  validates this repository's, its example projects' and its fixtures' exports against it, and the check list
  runs that test (`published-schemas`).
- **D6. The builtins declare what their specs and passes read.** Formal: `requires`, `ensures`, `maintains`, `sync`
  are blocks; the three condition edge types, which nothing wrote, are gone. Software: `event.payload` stays a
  reference to a type (the 106 inline payloads became named types); `type` declares no `fields` and no `kind`
  (its members are its body). Governance: `post_mitigation` is a block.

## Rejected

- **A ninth field type, `expression`**: a protocol change and a re-vendor for a content the prove pass reads as
  text.
- **A JSON Schema validator crate as the oracle** (ADR 0051): no such crate is in the lockfile, and a document
  inside the checker's subset is checked exactly by it.
- **Keeping `requires`/`ensures` as reference lists**: the conditions are named obligations, not invariant ids;
  963 of 963 written values are blocks.
- **Exempting open-fields kinds from E061** so `type` could keep `fields`: a second rule for one collision.

## Consequences

- User-visible: an expression field is a string in every export, MCP reply and command input; `schema --publish`
  documents change shape (`oneOf` per kind, `spec`/`ref` kinds, per-format `$id`, `format_version` optional for
  context and brief); E061 refuses blocks, verify statements, expressions and type unions where the type never
  takes one, and a scalar on a block; a single integer, boolean or date on a list field is exported as a list;
  `specforge.query`'s outputSchema defines `FieldJson` and `VerifyStatement`.
- The formal declaration has 10 edge types; the software `type` kind 4 fields; every builtin re-vendored once.
- The schema cache sees `FieldTypeChanged` for the retyped builtin fields: one major bump of a project's cached
  schema version.

## What would reopen it

- A field whose written JSON must depend on more than its value (a declared format, units): `write` then takes
  the form.
- An output with its own value vocabulary (as DBML has for types): it maps `FieldJson`, as DBML maps field types.

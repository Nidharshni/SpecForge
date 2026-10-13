# One field type, read from the declaration

**Status:** accepted (2026-10-07)

A field's type was defined once, `specforge_protocol_types::FieldType`, and copied twice on the host:
`specforge_registry::ManifestFieldType` (with an enum's values inside) and the emitter's
`ModelFieldType`. Six hand-written name tables sat around them. They disagreed: a bool field was `bool` in
the hover and E061 and `boolean` in `specforge schema`, the exports, the published JSON Schema and every
model diagram, and E061 told the user to check the type with `specforge schema --kind`. The emitter turned
the registry's type into a string and parsed it back, with a fallback (W146) that no registry-built schema
could reach. A field registry entry held parsed copies of what its embedded declaration said, and every
hand-built entry in the tests left the declaration's type empty. The schema dropped a declared default
value, and the infer prompt listed a kind's declared fields instead of its registered ones. The last
drift between two spellings of this vocabulary was a load bug (5eb023cd).

## Decision

**D1. The protocol's `FieldType` is the one field type.** `ManifestFieldType` and `ModelFieldType` are
deleted. The vocabulary answers `is_reference` and `is_list`, and holds `ProofRole` beside it. The
registry re-exports both.

**D2. A registry entry is built from its declaration.** `FieldRegistryEntry::new(kind, extension,
declared)` is the only constructor. It refuses a type the host does not read (the registry build reports
W019), names the type canonically in the descriptor it keeps, and reads the proof role (an unknown one is
none; W021). Its fields are private: `field_type()`, `enum_values()` (the declared values, for an enum),
`proof_role()`, `type_label()` (`enum (low, high)`) and `declared()` read the one declaration.

**D3. Every output names a field type by `FieldType::as_str()`.** That covers:
- the Graph Protocol schema (`specforge schema`, `specforge.schema`, `specforge://schema`, the schema a
  full export embeds, the schema cache);
- the published JSON Schema's `field_type` enum (generated from `FieldType::ALL`);
- the model's markdown, mermaid, dot and json;
- the hover, E061 and the infer prompt.

`bool` replaces `boolean` in the schema and the model. DBML writes its own column types (`varchar`,
`integer`, `boolean`, `text`, `json`). The schema and the model hold the type, not a string. The older
names are read, so a cache an older host wrote (`"boolean"`) reads as the same type: no
`FieldTypeChanged`, no W053, no version bump. `format_version` stays `2.0`. The content hash changes with
the bytes.

**D4. The schema carries what the declaration says.** A declared `default_value` reaches the schema and the
model.

**D5. W146 is retired.** A typed schema carries no type the model cannot name. `specforge_ops::model::model`
returns the rendered text. *(Amended by ADR 0007's round-5 amendment: it returns `ModelOutcome { document,
notices }`, the I020 notices of the model's `kinds` filter, and refuses an unknown root or extension. No
catalogued code was added.)* No code replaces it:
- a declared type the host does not read is W019 at the registry build;
- a schema cache that cannot be read, now also one naming a type this host does not read, is no previous
  schema, as before.

This amends [ADR 0015](0015-read-views-are-operations-over-the-project-view.md) D9.

**D6. Field help names a type as E061 does**, an enum's values included.

**D7. The kind-scoped infer prompt lists every field registered on the kind** (own, shared and
enhancement), sorted by name, typed by name.

## Rejected

- `boolean` as the canonical name. It changes the extension wire every SDK guest writes and the text the
  hover and E061 already show.
- Two named spellings in the one module (a declaration name and a schema name). E061 sends the user to
  `specforge schema`, which must use the same word.
- A breaking-change bump for the spelling. No field's type changed.
- W152 for an unreadable cache entry. The cache already has silent unreadable cases, and a declared
  unknown type is W019 before any schema exists.

## Consequences

- One name per type in every output. A consumer that matches `"boolean"` in an export, or validates new
  exports against a JSON Schema published by an older host, must accept `"bool"`.
- An inconsistent field registry entry does not compile. Tests build entries with
  `FieldRegistryEntry::new` or through `build_registries`.
- `specforge model` prints no warnings; `specforge explain W146` reports it retired.
- The SDK's `FieldBuilder::proof_role` still takes a string.

## What would reopen it

- A field type whose wire name and output name must differ (an output format with a fixed type
  vocabulary of its own, as DBML has): that output maps the type, as DBML does. The vocabulary does not
  gain a second name.
- An extension protocol major version: it may drop the older spellings.

## Amendment (round 8, plan 01)

A field's type also decides which values it takes, how a written value is stored and the JSON written for it,
through one field form (ADR 0064). D3's "published JSON Schema's field_type enum" is the derived `FieldType`
shape.

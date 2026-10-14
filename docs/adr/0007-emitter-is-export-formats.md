# The emitter is export formats; domain names come from manifests

**Status:** accepted (2026-10-02)

`specforge-emitter` held the compile's glue (graph checks, extension loading, Wasm custom rules),
the analysis passes, the host's coverage view, traces, plan validation, stats, source scanning,
diagnostic presentation and the schema cache's file I/O beside the export formats, and linked the
Wasm runtime for them. ADR 0004 D1-d wanted it free of the runtime. It also named domain concepts
the compiler must not know (principle 2): union `variants`, `abstract`, the `invariant` and
`property` kinds, the `type` kind, `contract` and `status`, and the builtin extensions by name.

## Where things live

- **specforge-emitter**: the export formats only (graph JSON, Graph Protocol V2 schema (the schema types,
  their generation and content hash; not their versioning) and
  exports, context, brief, scoped and budgeted exports, DOT, the model and outline diagrams).
  It depends on the graph, the registry, serde and sha2; no Wasm runtime, no file I/O. The
  renderers draw different things and keep their layouts; how declared text is written into each
  syntax is shared (`diagram.rs`): DOT strings, record fields and HTML labels, Mermaid strings and
  names, Markdown cells, DBML names and strings, and the bare identifier an extension name becomes.
  A DBML reference is one named `Ref`, written only between columns the output writes. The legacy
  flat re-exports are gone: callers use `emit`; a format module keeps an entry point of its own
  only where a caller needs what `emit` does not take (`dot::emit_dot` with `DotOptions`). `emit_brief` and `emit_context` are gone
  (round 5, plan 15).
- **specforge-project**: `compile` (graph checks, `load_extensions`, Wasm custom rules),
  `field_types`, `passes` (extension compiler passes), `coverage` (the host's coverage view and
  the test report format).
- **specforge-ops**: the built-in `contracts` pass, `trace`, `plan`, `stats`, `scan`,
  `schema_version` (the versioned schema: record, diff, version, reader check; ADR 0067).
- **specforge-common**: diagnostics JSON, the one-line form, the output cap and the exit code,
  beside `Diagnostic`. The catalog crate stays dependency-free (D6-d); common links it for the
  titles. The validator keeps the source-annotated rendering and the one summary line
  (`aggregate_diagnostic_summary`; the emitter's other summary had no caller).

  > Amended by ADR 0031: the source-annotated rendering, the summary line and `Counts` moved to
  > `specforge_common::present`; the graph checks and E061 run in `RegistryBuild::check`;
  > `specforge-validator` is gone.

## What extensions now declare

| Declaration | On | Replaces | Builtins that set it |
|---|---|---|---|
| `exempts_obligations` | field | `abstract` by name (D2-b) | formal's `abstract` enhancement |
| `headline` | field | `contract`, `status` in the context export | software `contract`, `status`; every product and governance `status` |
| `contract_target` | entity kind | `["invariant", "property"]` (A010) | software `invariant`, formal `property` |
| `declares_types` | entity kind | `kind == "type"` (custom validators' `declared_types`) | software `type` |
| `theme_color` | handshake / manifest | the model and outline palettes keyed by extension name | software, product, governance, formal |

A union body (`kind X = a | b`) is structural syntax: it owes no obligations because it has no
body to hold them. It is recognised by the key the parser itself gives it
(`specforge_parser::UNION_VARIANTS_FIELD`), not by a value's shape alone: the parser also turns
a user's `values [a, b]` into a variant list, and that exempts nothing. `title` (the entity
title) and `verify` (ADR 0002) stay structural. The flags are optional on the wire and in
`ManifestV2`; the Graph Protocol schema does not carry them, so exports are unchanged.

MCP `inspect`'s `contract` and the context prompt's `contract_text` are the entity's field
declared both `headline` and `normative` (`specforge_emitter::context::headline_statement`): a
behavior's or an event's `contract`; a `status` is headline only.

One visible consequence: the context export lifts only declared headline fields, so a struct
member that happens to be named `status` on an open-fields kind (a `type`) is no longer shown
as the entity's status, as `abstract` already exempts nothing unless declared; MCP's `contract`
likewise.

## The model and the outline are one call each (amendment, architecture round 5, plan 14)

`specforge_emitter::model::export(schema, declarations, options)` draws the logical data model:
build, theme colours from the declarations, the selection (`extension`, `kinds`, and a `root` with
its `depth`), the field level, and the format with its grouping. `specforge_emitter::outline::export(
declarations, options)` draws the extension outline: the cards, the dependencies `deps` selects (in
every format, JSON included), the enhancements and the cross-extension edges. Their intermediate
representations, builders, filters and renderers are private. The JSON format is still the model's
intermediate representation, serialized. Before, callers composed five public steps
(`ModelIntermediate_from_schema`, `with_theme_colors`, `filter_entities`, `filter_fields`, `render`),
and `render` took filters it never read.

They stay beside `emit`, not in its table. `emit` exports a graph's entities: scope, depth, kinds and
token budget over a `Graph`. The model and the outline draw what the extensions declare, from a schema
and the declarations, with options `emit` has no use for (grouping, field level, dependency depth). A
shared entry point would only dispatch between two unrelated inputs and option sets.

Both exports are total: a name the schema does not have selects nothing. Requests are checked by the
operation over the project view (`specforge_ops::model::model`, ADR 0015 Q3):
- a root kind no loaded extension declares is `unknown_kind`;
- an extension the project does not load is `extension_not_found`;
- a kind of `kinds` the project does not know is an I020 notice.

The operation returns `ModelOutcome { document, notices }`. `ModelOptions` cannot express a depth
without a root (`ModelRoot { kind, depth }`), and an empty `kinds` selects every kind.

What would reopen it: a third consumer that needs the model's intermediate representation itself
rather than a drawing of it (an editor view of the model, say), or a format that needs both the graph
and the schema.

## Known gaps

- ~~**The prove pass** (`specforge_ops::prove`) is host-side because z3 is native. It reads
  `constraint` entities' `metric` as bounds and any entity's `expression` as a claim, and the
  unknown-field check accepts `expression` on every kind for it. Moving that needs a manifest
  declaration of bound and claim fields; it waits for the shared prove code (D3-f).~~ Closed by
  [ADR 0009](0009-host-passes-read-declarations.md): fields declare a proof role, and the pass
  stays host-side.
- ~~**The coverage rule** (`specforge-coverage`, owned by `@specforge/testing`, D2-f) names
  `invariant` and its `risk` (A002, the risk tallies) and the `property` verify kind; the host
  passes `risk` by name into it. `contract_target` is not a substitute: it also marks formal's
  `property` kind, which would gain risk tallies and A002. It needs a declaration of the
  risk-graded kind and its risk field, sent to the testing pass.~~ Closed by
  [ADR 0009](0009-host-passes-read-declarations.md): `@specforge/testing` passes its risk
  grading to the rule. The `property` verify kind stays: it is testing's own vocabulary.
- ~~**The build cache** (`specforge_project::BuildCache`) records every entity's `status` for the
  product's transition checks (W087–W091). Recording declared fields instead changes the cache
  file and `PassBuildCache` (the SDK, so every blob), and wants a field flag of its own.~~ Closed
  by [ADR 0009](0009-host-passes-read-declarations.md): a kind declares its lifecycle field, and
  the cache file and `PassBuildCache` are unchanged.
- ~~**`specforge-cli/src/product`** implements product queries natively.~~ Closed by
  [ADR 0008](0008-extension-commands-run-over-the-graph.md): they are `@specforge/product`'s commands.
- The model's DOT cluster ids strip the `@specforge/` scope (snapshot-locked output).
- ~~**Implicit edge types name their first writer only.** An edge a field writes without its extension
  declaring it is registered once, with the first writing kind as its source and that field's target
  (`compilation/populate.rs`, `register_implicit_edges`). A second kind writing the same undeclared
  edge is in neither side, and the schema's inference does not run for a side that is set. No builtin
  has such an edge (round 6 plan 12, R5). Fixing it means deciding what an edge rule's peer scope is
  for an edge several kinds write.~~ Closed by the round 6 follow-up amendment below.

## Amendment (architecture round 5, plan 16): an extension's counts are what the model draws

An extension's `entity_count` is the kinds it declares that the model keeps, and its `edge_count` the edge types
it declares that name at least one relationship the model draws: one rule before and after a selection. A
standalone edge type (no source or target kinds) draws nothing and is not counted, and an extension that keeps no
kind still counts its enhancement edge types between kept kinds. The markdown summary's "N edge types" is the sum.

## Amendment (architecture round 6, plan 12): an open edge type takes the kinds of its writers

An edge type whose registry entry leaves a side open is given, on that side, the kinds whose fields
write it and the target kinds those fields declare, sorted (`schema::generate_schema`). A side it
declares is kept. ~~The inference stays in the emitter. `generate_schema` is its only reader (the model,
`specforge schema --kind` and the exports read the schema it returns), so moving it beside the
registries would relocate it without a second caller. Reopen this when another reader needs an edge
type's endpoints.~~ *(Superseded by [ADR 0060](0060-edge-types-resolved-once.md): the registry build resolves each edge type once; the schema prints its ends and infers nothing.)*

## Amendment (round 6 follow-up): an undeclared edge type names every writer

~~An edge label fields write without any loaded extension declaring it is registered with its label alone
and every field that writes it (`EdgeRegistryEntry::writers`: extension, kind, field, target kind), in load
order: a kind's shared fields it does not declare again, then its own. Its ends are its writers'
(`EdgeRegistryEntry::source_kinds` and `target_kinds`, each kind once, in load order), and the first writer's
extension owns it (`source_extension`, which the model's per-extension edge count reads). It is recorded after
every declaration's edge types, so a label any extension declares is that declaration, whichever field wrote it
first, and no W018 is reported against an undeclared use.~~

~~An edge rule's peer scope is its edge type's far end: the declared kind, else every writer's kind (or, outgoing,
every writer's target kind) that is loaded, and an edge counts when its peer is any of them
(`EdgeScope::Peers`; the registry snapshot's `edge_peer_kinds`). A rule none of whose far-end kinds is loaded is
inert, as before; an open far end counts every edge. The schema names every writing kind through the open-side
inference above (both sides of an undeclared edge type are open); each of those kinds names its own extension.~~
*(Superseded by [ADR 0060](0060-edge-types-resolved-once.md): the registry build resolves each edge type once; the schema prints its ends and infers nothing.)*

## Amendment (architecture round 8, plan 01): a field value is written by the graph's field form

How a field value is written is the graph's field form (ADR 0064); the emitter writes the envelopes and nodes
around it, and derives the published schema from the export document.

## Amendment (architecture round 9, plan 10): a node's coverage status is an export input; the export stays text

A query with coverage wrote its document in three steps: the emitter wrote the export as text, ops parsed it, wrote
`coverage_status` into each node and printed it again (in alphabetical key order), and MCP's outputSchema had the key
patched in by JSON path. The published schema never had it: `specforge query --include-coverage` documents
referenced `graph-protocol-v<version>.json` and failed it on every node (brief documents failed theirs too).

- The emitter writes the status. `EmitOptions::coverage` carries each entity's verdict
  (`ProjectCoverage::verdicts`); every node of the graph, context and brief formats whose entity has one carries
  `coverage_status` (`specforge_coverage::Status`, last in the node), and a token budget counts it. The derived
  document schema, MCP query's outputSchema and every published schema admit it as an optional key, with no patch.
  The emitter depends on `specforge-coverage` (pure); `Status`'s shape is `specforge-common`'s, beside `FieldType`'s.
- `specforge_ops::query` asks for it (`export::Request::coverage`) after looking the entity up, so an unknown entity
  is E003 before an unreadable report is E045.
- `emit` keeps returning text. A typed document was considered and rejected: five of six consumers want text and DOT
  is text; MCP's structured result is a `serde_json::Value` checked by `shape::violations`, built from a typed document
  exactly as from the text; the token budget measures text. It would have widened the emitter's interface by the
  envelope and node types to remove one `expect`.

What would reopen it: a consumer that edits an export before it is written (a second per-node key an operation adds),
or an MCP reply that can be sent without building a `Value`.

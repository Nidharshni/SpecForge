# A core tool's reply is one typed definition

**Status:** accepted (2026-10-09); D8 extended by ADR 0058 to every sent document; D1's "inline", D3's checked keywords, "Rejected: schemars" and
"What would reopen it" amended by ADR 0051 (2026-10-11). Supersedes ADR 0033's consequence "Output schemas stay
hand-written". Amends ADR 0004 D2-c, ADR 0015 I5 and ADR 0016 (the inspect aliases). D2's query sentence amended by ADR 0007 (round 9, plan 10). D2's tool table and D9 are refined by ADR 0049: a tool's module names its reply (`type Returns`).

Every core MCP tool's reply was written three times: a spec type in `spec/types/mcp.spec`, a
hand-written output schema in `tools/table.rs` (25 schemas, 199 lines) and a `json!` in the handler
(45 sites), plus six ops `to_json` documents passed through. Nothing compared them. The schemas
stated no `additionalProperties` and left 48 arrays without `items`, so a key added or dropped passed
the one probe per tool, and the checker ignored `additionalProperties` anyway. The spec types had
drifted: `McpDoctorReport` listed 4 of doctor's 11 keys, `McpExtensionInfo` said "absent" where the
reply sent `null`, `McpInspectResult` required a `title` sent `null`. Replies carried diagnostics in
four shapes. Search, list, coverage, outline and suggest_fixes answered arrays, which MCP cannot
structure. `operations/mod.rs`, the most-changed file of the workspace, held 11 handlers.

## Decision

- **D1. One derive, beside serde.** A reply document derives `Serialize` and `Shape`
  (`specforge_common::shape`, proc-macro crate `specforge-shape-macros`). The schema is read from the
  same fields and serde attributes as the serialization (rename, rename_all, skip_serializing_if,
  flatten, tag, untagged; `#[shape(as)]`, `#[shape(names)]` for a field serde writes by other
  means), inline, without descriptions. An object is closed (`additionalProperties: false`), an
  array typed, a name set an `enum` from its one definition (an option table, a serde enum).
  Content the project's extensions define is an open value (`serde_json::Value`), listed by a test.
- **D2. One reply type per tool.** A core tool's handler answers its module's `Reply` (`Answered<R>`,
  a mutation `Mutation<R>`) or `Text`; the tool table names it once (`typed!(h, Args => Reply)`), and
  the outputSchema is derived from it. A mutation's reply is `WrittenReply<R>`: `files_written` is a
  derived field (sent as that type, with rename's fresh diagnostics a derived field beside it: ADR 0022,
  round-6 amendment). The documents both surfaces print (analyze, collect, providers, the extension
  entry, inference progress and gaps, trace, schema) are typed once in ops; query's schema is the
  emitter's derived document schemas, whose nodes carry the optional `coverage_status` (ADR 0007, round-9
  amendment).
- **D3. A structured result its schema refuses is never sent.** Every core reply with an
  outputSchema is checked before it is sent, in every build; a violation is `schema_mismatch`, as
  for extension tools. The checker (`shape::violations`) reads `additionalProperties` and `minimum`.
- **D4. An `Mcp*` reply never carries `null`**: an optional value is absent, the spec's
  `@optional`. The diagnostics JSON keeps its documented `null`s.
- **D5. A JSON reply is an object.** Search, list, coverage, outline and suggest_fixes answer
  `{results|entities|entries|fixes: […]}`. Validate keeps its bare array (ADR 0018 D4); export,
  model and outline_extensions answer text.
- **D6. One diagnostics shape** (`DiagnosticList`, the `DiagnosticJson` array) in every reply:
  inspect's diagnostics, analyze's findings, migrate's post-migration errors.
- **D7. No deprecated or constant reply keys**: `inspect.references`, `inspect.reference_count`,
  `stats.coverage_pct`, `remove_extension.success`, `migrate.changes` are gone.
- **D8. The spec is checked, not generated.** A test compares each `Mcp*` type with the schema its
  tool's reply derives (fields, `@optional` ↔ not required, element and literal types, unions ↔
  `oneOf`) and proves each type's `schema is valid` verify. Domain types a reply embeds from other
  spec files (`Diagnostic`, `FieldMap`, `MigrationResult`, …) are compared by identity.
- **D9.** Each core tool's handler is its own module under `tools/`; `operations` is no MCP module.

## Consequences

- User-visible over MCP: every outputSchema is strict and states what was undeclared (query's
  `schema_ref`, every array's items); a reply that breaks its schema is `schema_mismatch`; an
  extension tool's output with a key its closed output schema does not allow is `schema_mismatch`;
  `null` values become absent keys (inspect, search, outline, find_references, suggest_fixes,
  find_spec_for_source, explain, stats' `proof_pct`, add_extension, remove_extension, migrate's
  `rollback`, extensions' `version`, trace's `edge_type`, analyze's `near`); search, list, coverage,
  outline and suggest_fixes answer objects with `structuredContent`; inspect's and migrate's
  diagnostics gain the full diagnostic keys; `references`, `reference_count`, `coverage_pct`,
  `success` and `changes` are gone; `infer_gaps` with nothing served answers the document's own
  keys; `files_written` loses its schema description; `tools/list` grows (see the plan's numbers).
- CLI: `specforge analyze --json` findings gain the full diagnostic keys and `near` is absent when
  none; `specforge extensions --format json` omits an unknown `version`; `specforge trace --format
  json` omits `edge_type` when the field names none.
- The invariant `mcp_type_schema_versioning` is deleted: replies are versioned by their listed
  schemas, and the project is pre-release.

## Rejected

- **schemars**: every schema needs a transform (root `title`, `$schema`, `format`, `$ref`/`$defs`),
  as ADR 0033 found for arguments.
- **A derive for MCP structs only**: the six documents both surfaces print would be defined twice.
- **A derive that also generates `Serialize`**: every ops document would leave serde for an MCP
  vocabulary.
- **Checking only in debug builds**: release servers would send non-conforming structured content,
  which MCP forbids.
- **Generating the spec's types from Rust**: the spec is authored prose; a test keeps it exact.
- **Wrapping validate's array**: ADR 0018 D4.

## What would reopen it

An MCP revision whose outputSchema or structuredContent model JSON Schema's core subset cannot state
(it needs `$ref`, a non-object root, or annotations clients require); a reply whose shape depends
on the project (an extension-defined key a closed schema must list); or `tools/list` size becoming
a client limit.

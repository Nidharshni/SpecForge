# The read views are operations over the project view

**Status:** accepted (2026-10-05); P7: tools are named by their typed constants (ADR 0049).

`specforge-ops` handed the read views out as building blocks and each surface chained them on its
own: the CLI's `stats` built a `CoverageRegistries` by hand and called `compute_project_stats`, MCP
did the same through its own `project_coverage`; each surface built `TraceExpectations` and picked
single or every trace; each ran `generate_schema` and versioned it, and each chained
`ModelIntermediate_from_schema → with_theme_colors → filter_entities → filter_fields → render`. Every
fix landed twice (stats 56f2bbce + 4d71702c, schema version 6feb6ab1 + 1d0a1db1, eight coverage
commits), and five drifts were live: the test report was read at three roots (`stats <sub>` read
`<sub>/specforge-report.json`, `analyze --path <sub>` walked up to the ancestor's, MCP read its
root); the schema cache at two (`specforge export <sub>` walked up to the project's cache, versioned a
schema compiled without the project's config against it, printed 20 W053 and overwrote it with
0 kinds); the MCP coverage tool listed the union types stats leaves out; MCP trace invented a
`gaps: ["no upstream links", ...]` list beside the real missing links; and MCP answered an unknown
schema kind with an empty schema where the CLI refused it. Model warnings reached the CLI's stderr
and were never read by MCP.

`analyze` was already the deep shape: one call over a `ProjectView`, one typed outcome, 40-line
adapters. Architecture plan 02 makes it the pattern for every read view.

## The project view

`specforge_ops::view::ProjectView` borrows what every read operation reads: the graph, the whole
`RegistryBuild` (kinds, fields, edges, rules, the extension declarations and their ordered passes,
ADR 0012), the root the project was compiled from, and its owner's coverage memo. Three constructors: `ProjectView::of(&CompiledProject)`
(the CLI), `ProjectView::of_session(&ProjectSession, root)` (the LSP), `ProjectView::new` (tests and
graphs built in memory; an extension command runs over `ProjectView::of` in the CLI too, ADR 0011 "One operation runs a command"); *(ADR 0047: two. `ProjectView::of` takes a one-shot compile or a session's compiled project (`ProjectSession::project`); `of_session` is gone.)* MCP builds every view through `ProjectRef::view()` of its call target (ADR
0014), or `Call::view()` for a call that may have no project. The view owns the recorded test
report (`test_report`), the coverage computed from it (`coverage`) and the versioned schema
(`versioned_schema`, `schema_cache`).

The operations, each one function over the view returning a typed outcome:
`stats::stats` (`Stats`), `coverage::{coverage, row}` (`CoverageOutcome`, `CoverageRow`),
`trace::trace` (`TraceOutcome`, which serializes as the document `specforge trace` writes),
`plan::check` (`PlanOutcome`), `schema::schema` (`SchemaOutcome`), `export::export`,
`model::{model, outline}` (`ModelOutcome`), `inspect::inspect` (`EntityFacts`, section
"Inspect"), `query::{query, list, search}` (`QueryOutcome`, `Listing`, `SearchOutcome`, section
"Query").
The CLI and MCP map arguments in and render the outcome, and the LSP hover renders inspect's; MCP
has one coverage-row presenter (`tools::coverage::row_json`) and one gap presenter
(`tools::trace::gap_json`).

## Decisions

- **D1. The view's root is the root the project was compiled from**, and the recorded test report
  (`<root>/specforge-report.json`) and the schema cache (`<root>/.specforge/schema-cache.json`) are
  read there, never in an ancestor. The compile never walks up (`Environment::load` reads
  `<root>/specforge.json`), so a walk-up read a report and a cache belonging to a project whose config
  was not loaded. `specforge analyze --path <sub>` now reads `<sub>`'s report, as `stats <sub>`
  already did, and `specforge export <sub>` versions against and records in `<sub>/.specforge/`.
  **Follow-up, not done here:** the CLI half of ADR 0014's call target, resolving `--path` to its
  project root once for every command, would change what `check`, `format`, `collect` and every
  other command compile for a sub-path. Until then `specforge collect --path <sub>` still writes the
  report at the project root, where views over `<sub>` do not read it.
- **D2. Coverage is memoized per compile and per report content.** `RecordedCoverage`
  (`specforge-project`) holds the parsed report and the coverage computed from it, keyed on the
  report's path and a hash of its bytes: one file read per call, no parse and no assessment of every
  entity when nothing changed. mtime is not the key (a rewrite within the same second would be
  missed). A `CompiledProject` and a `ProjectSession` each own one; *(ADR 0047: the compiled project owns it, one-shot or a session's.)* the session starts a fresh one on
  every update, re-check and reload, and MCP replaces its session when it serves a graph built in
  memory, so no invalidation can be forgotten. Errors are never memoized. The versioned schema is
  not memoized: it is a function of the registries and one small file, and its formats are the
  emitter's (ADR 0007).
- **D3. The unfiltered coverage view lists the entities that count toward coverage**, extending
  ADR 0004 D2-b to MCP's listing: `rows = stats' testable count`, covered rows = the proven count. An
  `entity_id` still reaches any entity; every row says whether it is `exempt` (a testable-kind
  entity that owes no obligations and declares none). An unknown `status_filter`, outside the tool's
  declared enum, is `invalid_input` with a did-you-mean.
- **D4. "Unverified" has one definition**: counts toward coverage and is not proven
  (`ProjectCoverage::is_unverified`, `CoverageRow::unverified`). Rejected: "declares no `verify`"
  (ignores proof, flags non-testable kinds) and "testable kind and not covered" (flags exempt unions).
  The trace prompt adopts it with plan 07.
- **D5. The gap vocabulary.** A **missing link** is an expected edge (from the registries) a traced
  entity lacks: `missing` in every trace document, the only gap a trace reports. A **plan gap** is
  how an agent plan falls short (`unresolved_entity`, `missing_plan_entry`, `ordering`). A dangling
  edge is E003, `check`'s business, never a gap. `ops::trace::Gap` is the presentation union of the
  first two (`McpTraceGap`). MCP's invented "no upstream/downstream links" is gone: isolation is an
  empty `upstream` or `downstream`, and `specforge.trace` returns exactly the CLI's document (its
  output schema is a `oneOf` of that document and `McpTracePlanResult`).
- ~~**D6. Stats keeps two JSON shapes**, `ProjectStatistics` and `McpStatsResult` (both published), as
  ADR 0004 D1-c accepted for diagnostics. The numbers live once, in the operation; a parity test
  holds the two shapes to the same numbers.~~ Superseded by
  [ADR 0052](0052-an-operations-outcome-is-one-document-both-surfaces-print.md) D1: stats' outcome
  (`specforge_ops::stats::Stats`, `McpStatsResult`) is the one document `specforge stats --format
  json` and `specforge.stats` print; `ProjectStatistics` is deleted.
- **D7. Stats' diagnostics are the surface's**: the CLI's view reports what `check` reports, MCP's
  its project's diagnostics plus its surface-registration notices (`ProjectView::reported`, see
  "Management operations"; `StatsRequest` is gone).
- **D8. An unknown schema kind is refused on both surfaces**: `unknown_kind` naming the closest
  kind; the CLI keeps its message (exit 1) and adds a help line, MCP answers `invalid_input` on
  `kind`. *(Amended by ADR 0029 D5: the CLI's refusal is `error[unknown_kind]: …` with a `hint:`
  line.)* ~~The CLI's `--kind` still prints the kind's entry alone; the operation returns the filtered
  schema MCP serves.~~ Amended by [ADR 0027](0027-an-enumerated-argument-is-one-option-table.md)'s round
  (architecture plan 2026-10-06 12, D4): `specforge schema --kind` prints the operation's outcome, the
  document `specforge.schema` returns (the kind and the edge types that touch it), and `--publish`
  goes through `ops::schema::json_schema`; `--kind` cannot be combined with `--publish`.
- ~~**D9. Model warnings are W146 on both surfaces**: CLI stderr `warning[W146]: model: …`, MCP the
  tool result's diagnostics. A registry-built schema only carries known field types, but the model
  accepts any Graph Protocol schema, and a catalogued code reaches `explain` and the docs.~~
  Superseded by [ADR 0034](0034-one-field-type-read-from-the-declaration.md) D5: the schema's field type
  is typed, so no schema carries a type the model cannot name; W146 is retired and `model` returns the
  rendered text.
- **D10. Only `specforge export` records the schema cache**, at the view's root (D1). MCP only
  reads it.
  *(ADR 0067: it records the versioned schema with its content hash, after a successful export of any
  format; every reader versions against the record through `SchemaCache::version`.)*
  *(Round 8 plan 11: every document that names a schema version names the view's versioned schema's: the
  export in every format, schema attached or not, trace, the model, and the graph an extension command
  receives. No output carries a hand-kept version constant: `SCHEMA_VERSION = "0.1.0"` is gone.)*
- **D11. Trace errors are typed**: `TraceError::EntityNotFound { entity_id, near }` replaces the
  emitter error that carried `E003: …` in its message for MCP to re-parse. It converts to an
  `OpError` E003 with a did-you-mean; the CLI prints `error[E003]: …` with a help line.
- **D12. The CLI's read commands hold a `CompiledProject`** (`pipeline::compile_project`), since
  `ProjectView::of` needs the registry build and the memo. `check` and the other commands are
  untouched; the management commands hold one too since "Management operations".

## Consequences

- MCP clients see: `specforge.coverage {}` and `status_filter: uncovered` without union types,
  abstract entities and governance entities that declare nothing; an `exempt` field on every row;
  no `gaps` on an entity trace; an error for an unknown schema kind or coverage status; W146 in
  model results (retired by ADR 0034).
- Sub-path invocations change (D1); the R1/R2 reproductions of plan 02 are tests.
- No ADR conflicts: D3 extends ADR 0004 D2-b, D4 is consistent with D2-a, D6 mirrors D1-c (both superseded by ADR 0052), and ADR
  0004 is silent on roots (27c48e54 had already chosen the view's own root for MCP).
- Found on the way: `specforge outline` printed a different document on most runs (a HashMap
  transitive closure); it is deterministic now.

## What would reopen it

A read view that needs more than the project view (the build cache, another project), or a third
surface that needs a shape of its own.

## Management operations (amendment, architecture round 3, plan 05)

The read views took the project view; the management operations still took the project in
pieces. `extension::list(root, enabled, loaded, kinds, graph)` read `specforge.json` again, `remove`
took an eight-field request (five of them project pieces) and read it a third time, `doctor`,
`providers`, `collect` (six parameters; both callers built `KnownEntities` themselves) and
`infer::{progress, gaps}` each took their own slice. Every change to the Environment's shape touched
seven CLI adapters and eight MCP handlers (714144a7, 6e040512, 94bbdeb3, aad9160d), and the slices
drifted: 94bbdeb3 gave `list` and `remove` the `enabled` entries and not `doctor`, which called a
`.wasm` file entry "builtin"; `remove` uninstalled the binary and emptied the lock before it found
`specforge.json` unreadable.

### The view, completed

`ProjectView` also borrows the **Environment** it was compiled in (`env()`: the config, what each
`extensions` entry enabled, the spec root, the registry build; `registries()` is its one accessor,
the view keeps no copy) and says **what its
surface reports** for the project (`reported()`): a `CompiledProject`'s diagnostics, a session's
plus MCP's I017 notices (`also_reporting`), or a listed slice (`reporting`, for graphs built in
memory). The view's fields are private: `graph()`, `env()`, `registries()` and `root()` are the
way in. `ProjectView::new` takes `&Environment` (`Environment::with_registries` for a bare build).
`project_root()` is the root for an operation that reads or writes the project on disk, `no_project`
without one.

### The operations

Each is one function over the view and a request: `extension::list(&view) -> ExtensionListing`,
`extension::providers(&view) -> ProviderListing`, `extension::remove(&view, &RemoveRequest { name,
force, dry_run })`, `doctor::diagnose(&view)`, `collect::collect(&view, Request { runner,
mode, consent, announce })`, `infer::progress(&view)`, `infer::gaps(&view)`, `infer::session(&view, SessionStep)` and `infer::lint(&view)`. `stats::stats(&view)` reads `reported()` (D7 amended). The CLI compiles once
(`pipeline::compile_project`) for every command; `CompilationContext` is deleted.

### Decisions

- **M1. The view borrows the whole Environment**, not chosen fields: every owner of a view owns
  one, and the next Environment field reaches the operations without touching a constructor. It
  adds no dependency: the Environment is `specforge-project`, which the LSP links; the `Registry`
  port (ADR 0010) is not in the view.
- **M2. Reported diagnostics are the view's, asked of its owner on demand.** The surface still
  decides what it reports, by how it builds its view. `check` keeps its `reported` parameter (ADR
  0018).
- **M3. The extension runtime is a parameter** of the operations that run extension code
  (`collect`, `gaps`, as `analyze`), never part of the view. *(Amended by "The runtime travels
  with the environment": the runtime is the environment's, so the view reaches it and these
  operations take none.)*
- **M4. Disk is the view root's.** `specforge.lock`, installed binaries, source files and the
  recorded report are read and written at `root`; `remove`, `collect`, `progress` and `gaps` refuse
  a rootless view (`no_project`); the listings and doctor answer from what the view enabled and
  loaded, doctor skipping the installation checks.
- **M5. `remove` decides every refusal from the view before it writes, then writes
  `specforge.json` first**, then the lock and the binary. A `specforge.json` the compile could not
  parse refuses every removal with `config_invalid` and changes nothing.
- **M6. One source rule** (`Origin::of`) names where an extension comes from in the listing and in
  doctor: `file:<path>` for a `.wasm` file entry, the lock entry's source, `builtin`, else `unknown`.
  *(Amended by ADR 0028 "The project's extensions are one inventory": the rule is the inventory's,
  `Origin::of` is gone, and it follows the load's order: a builtin before a lock entry, a named
  entry before a `.wasm` file entry.)*
- **M7. One presenter per listing** (`ExtensionEntry::to_json`, `ProviderListing::to_json`); doctor
  keeps two published shapes (ADR 0004 D1-c).
- **M8. `add`, `update`, `init` and `migrate` are not view operations**: they run before or instead
  of a compile, and `add`/`update` reach the `Registry` port. `add` and `update` read the config
  through `config::usable`/`config::required` (`read_project_config`, the function the compile reads
  it with) and the installed extensions through `Installed::at` (M10, ADR 0028), never a reader of their own
  (see M11).
- **M9. A `specforge.json` not used as written is E069, an error.** The Environment keeps every way
  the file is not used as written (`config_problems`: unreadable, not JSON, not an object, a key of
  the wrong type, a non-string `extensions` or `exclude` item) and
  reports each as E069, first; when nothing could load, I002 names the file instead of "no
  extensions configured". `remove` refuses from those reasons without reading the file again (M5),
  when one blocks the edit. It is an error under ADR 0018's unchanged verdict rule ("no error among
  what was reported"), as E028 is for one extension that does not load: a project whose config
  loads nothing must not pass `check`. Doctor reports it as an error, and reports a missing
  `specforge.json` as the warning finding `config_missing`.
- **M10. `specforge.lock` is read once, by the Environment.** `Environment::installed` holds a typed
  `LockState` (`specforge_installed`: `Absent`, `Read`, or `Unreadable` with its E033 problem), read at
  `Environment::load` and reloaded when the file changes (it is an environment input); the view's
  `lock()` hands it to `list`, `doctor` and `remove` (none without a root), so they read what the
  compile read, not the disk again. It is a typed result, not a diagnostic: a corrupt lock fails
  `check` only through the installed extensions it leaves unloaded (its E033 once, then their E028s,
  ADR 0028), and `doctor` lists it as the error finding `lock_unreadable` naming E033.
  `Installed::lock_path` is the one definition of where the lock lives, used by the Environment, the
  extension load and the root-based `add` and `update` (M8), which read it with the same
  `Installed::at`.

- **M11. One refusal for an unusable `specforge.json`.** `add`, `update` and `remove` refuse a
  config that `ConfigProblem::blocks_edits` names with `config::refusal`: code `config_invalid`
  (kind `schema_mismatch`), the problem's text, which is E069's reason, as the message; before they
  install, write or delete anything, dry runs included. `remove` builds it from the compile's
  problems (M5); `add` and `update`, which run without a compile (M8), from `read_project_config`.
  `config::edit_extensions`, the one writer, refuses through the same function, so a config that
  changed after the compile is refused too. A missing `specforge.json` is `config_not_found` for
  `add` (hint: `specforge init`) and not refused by `update`, which only reads the lock. `add` used
  to refuse with E032 ("extension install or uninstall failed"), a code about something else.
- **M12. The inference session steps are a management operation that writes** (architecture round
  4, plan 06; ADR 0022 "Inference sessions"). `infer::session` takes the view, refuses a rootless
  one (`no_project`, M4) and writes only `<root>/specforge-infer.json`. The inference manifest is not
  part of the view: it is not a project input, and each operation reads it once.
- **M13. The `inferred` lint is computed in ops.** `DiagnosticPolicy::apply` asks its caller for a
  profile's diagnostics; `check` answers `inferred` with `infer::lint(&view)`, which reads the
  manifest through the one reader and the density threshold from the view's Environment, not from a
  second read of `specforge.json`. `specforge-project` reads no inference file.
- **M14. An unusable inference or anchors manifest is E071**, an error, for every operation that
  reads it: progress, gaps, the session steps, `infer::lint` and `navigate::source_anchors`. The infer
  prompt's plan no longer counts from scratch when the manifest cannot be used (`progress_or_fresh` is
  gone).

### Consequences

- `specforge doctor` and `specforge.doctor` report a `.wasm` file entry's source as
  `file:<path>`, and an extension of unknown origin as `unknown`, not `builtin`.
- `specforge remove` and `specforge.remove_extension` with an unreadable `specforge.json` change
  nothing (they used to uninstall first); `specforge add`, `specforge.add_extension` and `specforge
  update` refuse it the same way (`config_invalid`, not E032).
- A `specforge.json` that is there and not used as written is the error E069 on every surface
  (`check`, watch, the LSP, MCP validate and doctor), and I002 says the file could not be read when
  nothing loaded; `check` fails on it where it passed, and so does `specforge doctor`.
- `specforge doctor` where there is no `specforge.json` reports the warning finding `config_missing`.
- No other output changes; the parity harness holds the CLI and MCP listings, doctor, inference and
  collect equal as before.

### What would reopen it

An operation that needs a project the view cannot describe (two projects, or the build cache), or
a management operation that must run without a compile.

## Inspect

*(Added 2026-10, architecture plan 07.)*

MCP `specforge.inspect` and the LSP hover answered "what is this entity" from two sources: inspect
from navigate's references, the coverage row, the headline statement and the diagnostics' data;
hover from raw graph edges and the kind registry entry. Of 27 commits to inspect, the one that
touched hover renamed a heading by hand (61f38250). Hover showed no coverage and none of the entity's
diagnostics unless they were under the cursor, and nothing held its `testable` badge to inspect's
field (ADR 0004 D2-d, which the spec states "as hover shows it"). A third copy, the MCP context
prompt, read the same headline, edges and obligations.

- **I1. Inspect is a read view**: `specforge_ops::inspect::inspect(view, entity_id) ->
  Result<EntityFacts, OpError>`. `EntityFacts` borrows the node and its kind's registry entry and
  carries the headline statement, the standing (the entity snapshot's `snapshot::Standing`,
  borrowed: `testable`, `obligated()`, `exempt()`; inspect keeps no standing type of its own), the obligations, the
  references (`navigate::References`, one per edge, in edge order, with the peer's kind and the
  field), the coverage (`Result<EntityCoverage, OpError>`, classified by `ops::report`, ADR 0029 D1) and the
  diagnostics the view reports
  about it (`navigate::is_about`). MCP inspect, the context prompt and the hover only render it.
- **I2. A reference list without spans** is navigate's (`References::of`), selected by the same
  `reference_edges` as `Navigator::references`. Inspect reads no spec file.
- **I3. The standing does not depend on the recorded report**, and the coverage carries the report's
  error instead of failing the view: MCP still fails `inspect` on E045 (ADR 0004 D2-e), and the hover
  shows the error on its Coverage line.
- **I4. The hover is editor-neutral markdown**: no raw `lsp_icon` word (a SymbolKind name, which
  document symbols use); the VS Code client adds its codicons (for a kind it does not know, the
  codicon of the SymbolKind the server reports for the entity's workspace symbol) and finds the
  entity header on any line.
- **I5. Additive MCP fields**: `exempt` (the coverage row's meaning), `obligated` and `source_extension`;
  `references` and `reference_count` were deprecated aliases, derived from the same reference list,
  until ADR 0048 D7 removed them.

Consequences: hover gains the headline as a summary, a Coverage line and the entity's diagnostics not
already shown, and a long non-ASCII field value no longer crashes the server. A parity test holds
hover and inspect to the same facts on every fixture entity. The reported diagnostics are the
view's (section "Management operations"): MCP's view reports its call target's, the LSP's reports
what it published (`ProjectView::reporting`), so the hover's dedupe against the cursor's
diagnostics compares the same copies. While the LSP's session is out for an update, the hover says
coverage is unavailable rather than reading a stand-in view with no root.

## Query

*(Added 2026-10, architecture round 4, plan 05.)*

`specforge query` had two implementations. The CLI called `specforge_emitter::query` over the raw
graph; MCP `specforge.query` built its own emitter options, read its format from `AGENT_FORMAT`,
reported I020 and injected coverage. Of 20 commits to the MCP handler none reached the CLI, which had
no format, no coverage, no I020 and printed `E003: …` raw; both answered Graph Protocol 1.0 where
`specforge://graph/{id}` answered 2.0 for the same request. Three modules decided what a known kind
is (two case rules, three wordings) and `specforge.list` reported nothing. "No such entity" was built
four ways, one by formatting `"E003: …"` and parsing it back, and the emitter's errors carried their
code in their text, which ops and MCP split off again.

- **Q1. Query, list and search are read views**: `specforge_ops::query::{query, list, search}`, each a
  typed request holding its defaults (`DEFAULT_DEPTH` 1, `DEFAULT_SEARCH_LIMIT` 20) and a typed
  outcome. `specforge query` and `specforge.query` render one query; `specforge.list` and
  `specforge://entities/{kind}` one listing; `specforge.search` one search. The CLI's query takes
  `--format` (`export::AGENT_FORMAT`) and `--include-coverage`.
- **Q2. A query is an export**: scope, depth, kinds and format under the export schema policy
  (`export::Request`, `Schema::Default`), so `specforge.query {entity_id}` is the document
  `specforge://graph/{entity_id}` serves; `include_coverage` adds each node's `coverage_status`
  (`coverage::STATUS`). The kind filter keeps an edge when both its entities are kept.
- **Q3. One known-kind answer** (`ProjectView::kinds`, `KnownKinds`): a filter over entities knows the
  declared kinds and the kinds entities are written with, and reports the others as I020 (the filter
  drops them); an argument that needs a kind's declaration (schema `kind`, the infer prompt's
  `kind:<name>`) knows the declared kinds and refuses the others with `unknown_kind`. Names are exact
  (keywords are case-sensitive); one wording, `unknown entity kind '<kind>'`; the suggestion is a kind
  equal ignoring case, else the closest. `specforge.list` reports I020 too and still lists nothing.
- **Q4. Emitter errors are typed**: `EmitterError::{ScopeNotFound, BudgetTooSmall, Serialization}`;
  `code()` is the variant's constant (E003, E062, none), `Display` carries no code. Ops maps the
  variants to `OpError` in one total `match` (`export::failure`); the kind is the operation's, the
  emitter does not link ops (ADR 0007). `SchemaVersionError` is read by its `reason` and `code()`.
  No adapter parses a message: `from_coded_message`, `split_code` and `without_code` are gone.
- **Q5. One not-found refusal**: `navigate::not_found(graph, id)`: E003, kind `EntityNotFound`,
  `unresolved entity '<id>' — not found in graph`, `did you mean '<closest>'?`. Export's scope,
  inspect, the navigator, rename and trace raise it; MCP tools and prompts convert it.
- **Q6. Search's field filter is a pair**: `field` with `value` keeps the entities whose field's text
  contains the value, ignoring case; one without the other is `invalid_input` naming the missing one
  (it was ignored).

Consequences: `specforge query` prints `error[E003]` with a did-you-mean, I020 on stderr, and takes
`--format` and `--include-coverage`; a graph-format query, CLI or MCP, is Graph Protocol 2.0 with
`schema_ref`; `specforge.list` reports I020; `specforge.schema`'s refusal loses its colon; the infer
prompt refuses `kind:Behavior` naming `behavior`, and lists capitalized keywords as declared;
`specforge export --scope` and the scoped resources say `unresolved entity '<id>' — not found in
graph`; MCP navigation refusals carry a suggestion; search refuses a lone `field` or `value`.

What would reopen it: a query that needs more than an export (a path query, a semantic search over
embeddings), or a third surface for list or search with a shape of its own.

## Prompt read views

*(Added 2026-10, architecture round 5, plan 04.)*

The MCP prompts computed what they showed. `prompts/explore.rs` ranked starting points and hubs and listed
entities with no edges; `prompts/review.rs` scoped the coverage view to a neighbourhood and flagged entities
with no edges; `prompts/infer.rs` (30 commits) merged an extension's inference guide with the project's,
wrote an example entity, counted entities per kind three times and paged the plan. These rules lived only in
MCP and were tested only through `prompts/get`. They carried live defects: the example wrote a required bool
or reference as a string (E061, or a reference that links nothing), the prompt sent the agent to `spec/`
whatever the project's spec root, it named `specforge_validate` (no such tool), explore applied its filters to
two of five lists, and an entity referencing only itself ranked as the most connected. "Orphan" meant five
things across the host and the extensions, and CONTEXT.md defined none.

- **P1. The prompts render read views.** `explore::explore` (`Exploration`), `review::review` (`Review`),
  `infer::guide` / `infer::kind_guide` (`InferenceGuide`, `KindGuide`) and `infer::inference_plan`
  (`InferencePlan`) are operations over the project view. A prompt maps its arguments, calls one (the file scope
  adds `navigate::anchors_of_file`), and adds only its instruction and the text that names tools.
- **P2. One JSON presenter per view, in ops** (`to_json`), the document both the prompt and the CLI print, as
  `Progress::to_json` is. `CoverageRow::to_json` is the one coverage-row presenter (it was MCP's `row_json`).
  The inference plan has one surface and its page marker names an MCP argument, so the prompt renders it.
- **P3. One connectivity rule.** `ProjectView::connectivity()` gives each entity's `Degree`, its edges to and
  from *other* entities. An entity is unconnected when its degree is zero: a reference that does not resolve is
  no edge, and an edge to itself links it to nothing else. Stats, the exploration and the review read it;
  `ProjectView::entities_by_kind` is the one count per kind. `ProjectView::neighbourhood` is the one
  `Graph::reach` the exploration and the review share, refusing an unknown entity with `navigate::not_found`.
- **P4. The exploration's selection.** `entity_id`, `depth` and `kind` select the entities every list is about;
  degrees count every edge of the project. Starting points and the most connected are connected entities only.
  An unknown kind is an I020 notice in the payload (Q3; a prompt has no `_meta`).
- **P5. One kind guide.** The kind's declaration is the one the kind registry registered it for (E026's
  first-wins); its fields are the registered ones, in the overview too; the example writes every field by its
  type; the spec directory is the project's spec root.
- **P6. The plan's kind order is derived.** Kinds with no entity first, then each after the kinds its reference
  fields target; the core names no kind. The spec's unbuilt phases are dropped.
- **P7. Tools are named from the tool table** (`tools::core_tool_name` over `CORE_TOOLS`), in the prompts and
  the server's instructions; a test scans every prompt reply for names no core tool has.
- **P8. Surfaces.** The CLI renders the exploration (`specforge explore`), the review (`specforge review`) and
  the guide (`specforge infer-guide`); the LSP documents each kind keyword's completion from its guide. The plan
  stays the prompt's (`specforge infer-status` shows the progress).
- **P9. "Orphan" is not a host word.** Unconnected entity (P3), unreferenced entity (no incoming edge: W012, the
  extensions' `no_incoming_edges` rules, which call one an orphan), stray test record (W097,
  `AnalyzeOutcome::stray_records`), stranded entity (`RemoveOutcome::stranded`, one typed list).

Consequences: `specforge stats` and `specforge.stats` say `unconnected_count` (and count a self-referencing
entity); the explore payload says `unconnected`, lists only the selection, drops unconnected entities from the
starting points and carries `notices`; the review says "is unconnected"; the infer prompt's examples are well
typed, its directories are the project's and its tool names are real; the overview lists registered fields; the
plan orders its kinds; `analyze` says `stray_records`; `remove` says `stranded`. Prompt tests keep only what a
prompt adds (arguments, refusals, instructions); the views' rules are tested in ops.

What would reopen it: a prompt that needs data no read view gives (it would get a view first), or a surface
that needs the plan.

## The runtime travels with the environment (amendment, architecture round 5, plan 09)

M3 kept the extension runtime out of the view, a parameter of each operation that runs extension
code. Every caller then passed the runtime the view's environment had been loaded in, by hand: the
CLI's `compile_project` returned the pair (11 of its 14 callers dropped the runtime), the session
and MCP's other-project compile kept a runtime field beside their environment, and the checks took
it once more. The pairing is what makes an extension call work at all (an extension is loaded in
one runtime), and it had slipped before (b122852f: extension resources and inference gaps ignored
the project's runtime).

- **R1. The Environment holds the runtime it loaded its extensions in** (`Environment::runtime`,
  `Option<SharedRuntime>`): `Environment::load(root, runtime)` and `from_read` take it and keep it,
  `run_checks` and the custom-rule probe read it, `CompiledProject::compile(root, runtime)` and
  `CompiledProject::of(env)` pass it through, and a session's runtime is its environment's.
- **R2. The view reaches it through the Environment** (`ProjectView::runtime`, M1: the next
  Environment field reaches the operations without touching a constructor). **M3 is amended:**
  the operations that call extensions take no runtime: `analyze(view, options)`, `collect(view,
  request)`, `infer::gaps(view)` and `command::run(view, command, given, format)`.
- **R3. An environment without a runtime loaded nothing.** Analyze then runs no extension pass
  (ADR 0013 D12's meaning); a command, a collector or a scanner called over it is E028, not loaded.
  No "unloaded" runtime adapter exists.
- **R4. What is not over a view keeps a runtime parameter**: `migrate::run(request, runtime)`
  compiles the project itself, twice, in the runtime it is given (M8), and reading a candidate
  extension's declaration has no environment.
- **R5. MCP's `ProjectRef.runtime` is read from the view**, for MCP's own extension adapters
  (`mcp__` tools and resources), which are not operations; `pipeline::compile_project` returns the
  compiled project alone (D12).

The extension command runs over `ProjectView::of` in the CLI too (ADR 0011, "One operation runs a
command"); `ProjectView::new` has no rooted production caller.

## Rewrites of the project's sources read the config the same way (amendment, architecture round 8, plan 03)

`format` and `migrate` read `specforge.json` through `load_project_config`, which dropped every problem: with a
truncated file they rewrote every `.spec` file under the root, `vendor/` included, and said `ok: true`, while
`check` failed with E069. A `spec_root` or `exclude` of the wrong type did the same, and `rename` rewrote such a
file from a compile that had reported the E069.

- **M15. One read for a rewrite.** `config::rewritable(root)` is `read_project_config` refusing with
  `config::refusal` (M11) the first problem that `ConfigProblem::blocks_rewrites`: the file can't be used, or
  `spec_root`, `exclude` or `extensions` has the wrong type: the run could not tell which files are the
  project's sources, or which extensions' migration hooks run. `format` reads every project it selects sources
  from through it, `migrate` reads its project once (the preview and the migration rewrite one list), `rename`
  refuses from the view's `config_problems()` as `remove` does (M5). Dry runs, `--check` and `--diff` refuse
  too, before any source is read.
- **M16. What a rewrite goes around, it reports.** Every other problem (a mistyped `name` or `version`, a
  non-string item, an exclude entry that can match no source) leaves the run's files and hooks as the file means
  them: the run goes on, and each problem is one E069 in its document (`ConfigProblem::diagnostic`, the text the
  compile reports), which fails its verdict (`format::Outcome::ok`, `migrate::Outcome::ok`). A named `.spec`
  file selects no config and reads none (ADR 0021 D3).
- **M17. `load_project_config` is gone**; every reader keeps the problems. The E069 builder is
  `ConfigProblem::diagnostic` in `specforge-common`.

M11 is amended: `config_invalid` is also format's, migrate's and rename's refusal, decided by `blocks_rewrites`
(a superset of `blocks_edits`).

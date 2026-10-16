# The registry build checks the entities; the validator crate is gone

**Status:** accepted (2026-10-07). Amends ADR 0007 (crate roles) and reopens ADR 0019's "structured
value" clause. D1's `RegistryBuild::files` sentence is reopened by [ADR 0107](0107-the-checks-read-through-the-disk-they-are-given.md): the session watches what the checks read. D1's tests sentence is completed by [ADR 0115](0115-the-entity-snapshot-is-made-in-the-registry.md): check tests start from `.spec` text through the build's own graph inputs.

Every check that runs over a built graph's entities sat in one of three places:

- `specforge-validator` held W012, E060 and E016, reading graph nodes against ADR 0019;
- five public `detect_*` functions in `specforge-registry` were called in an order, and behind a
  gate (`kinds.is_empty()`), that existed only in `specforge_project::compile::check_graph`;
- `specforge_project::field_types` held E061, reading graph nodes because the entity record had
  lost each value's shape.

Eleven test files called the steps directly, and six tests proved E024/E013 in a configuration
production never reaches. The two predicates for "structural-only" disagreed (`kinds.is_empty()`
for the checks, no declaration for I002), so a project whose extensions declare no kind lost its
kind, field and identifier checks with no notice. E016 matched `file_reference` fields by name
across every kind and read lists only: another kind's same-named field was checked (and
watched), while a single path was not. The validator's other half was the CLI's ariadne
rendering and a summary line, which re-tallied what `ops::check::Counts` and
`compute_exit_code` already counted. Provider registration was a second public entry into the
registry crate. It parsed `specforge.json`, and the providers listing re-ran it on every call.

## Decision

**D1. One entry runs every entity check.**

```rust
RegistryBuild::check(&RuleInput, &dyn CustomVerdicts) -> Vec<Diagnostic>
```

It runs, in order: W012, E016, then, unless `structural_only()`, E024, E013, E014, W020, E022 and
E061, then the rule set (ADR 0020). `RegistryBuild::files(&RuleInput)` is the one set of files
those checks read (E016's paths plus `file_exists` files), which the project session watches. The
checks are a private module, `specforge_registry::checks`. Tests run them through the build, as
ADR 0020's rule tests do.

**D2. One gate, announced.** `structural_only()` is "no loaded extension declares a kind". With
no extension loaded, I002 says so, as before. With extensions loaded, W151 names the entities
left unchecked. E013 and E014 stay behind the gate. Running them without extensions would fail
every structural-only project with one-character IDs, for a contract (the identifier length) that
only extension-backed projects have ever been held to.

**D3. The record carries what a check needs.** `FieldRecord` gains `shape: ValueShape` (one
variant per parsed value form) and `value_span`, as ADR 0019 foresaw ("add a structured value
beside the text"). The field text is unchanged, and neither new field crosses the protocol.
E061, W012 and E016 read records, so no check reads a graph node.

**D4. E016 follows the registry.** A `file_reference` field is one on the kinds that declare it.
Its value is a list of paths or a single path.

**D5. E060 is the linker's debug assertion.** `link_and_diagnose`, the one link of the cold build
and the update, ends by asserting in debug builds that every reference to an existing entity has
its edge. E060 is retired. `Graph::clear_edges` is private.

**D6. Presentation is one module.** The annotated rendering, the summary line and the CLI's plain
form for a diagnostic without a span (`render_plain`) move to `specforge_common::present`, beside the
text and JSON forms and the exit code. A spanless diagnostic is printed in that plain form, never
as a snippet of an unrelated first file. `Counts` moves
there too, and is the one tally (`ops::check::Counts` re-exports it).

**D7. Providers are configuration, registered once.** `specforge_project::providers::Providers`
reads `specforge.json`'s `providers` and registers each scheme against the loaded declarations in
`Environment::load`. I005 and the providers listing read it. The registry crate keeps only
`build_registries` and its build.

**D8. A rule whose target kind is undeclared is inert.** This enforces ADR 0020 D5 for target kinds
as it already held for edge types: such a rule is not registered and no longer fires on entities
written with that keyword (they are E024).

Rejected:

- the structural checks as host rules inside `Rules`, which would make `Rule` carry messages,
  data and a gate that have no target kind;
- providers as an input of `build_registries`, which would put `specforge.json` into the pure
  build and change 47 call sites;
- running every check without extensions (D2).

## Consequences

- `specforge-validator` is deleted. `specforge-common` depends on `ariadne`. Builtins do not link
  it, so nothing is re-vendored.
- New warning W151. E060 retired (never reused; `@specforge/formal`'s planned payload check takes
  another code).
- A project whose only `file_reference` field shares its name with another kind's field stops
  reporting (and watching) that field's values. A single-path file reference is checked.
- A rule targeting an undeclared kind stops firing, and stops obliging those entities in the
  coverage view. The registry-build snapshots of an extension loaded alone lose the rules whose
  kinds it does not declare.
- The CLI prints a diagnostic without a span as `severity[CODE]: message` with its `= help:` line,
  instead of a snippet of the first file.
- `check`, watch, the LSP and MCP report the same diagnostics in the same order as before, except
  for those changes.

## What would reopen this

- A check that needs more than one entity's record and the labelled edges (`RuleInput` grows a
  field, as ADR 0020 says).
- A project that wants the identifier contract enforced with no extension loaded (D2).
- A second producer of entity records that is not the snapshot (the record's fields then need a
  constructor that guarantees them).

## Amendment (architecture round 8, plan 01)

D3's `ValueShape` moved to `specforge_graph::ValueShape`, beside the field form that reads it (ADR 0064).

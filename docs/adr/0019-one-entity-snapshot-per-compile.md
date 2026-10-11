# One entity snapshot per compile

**Status:** accepted (2026-10-06)

Every check that runs after the graph build read graph nodes its own way. Three walks flattened a
node's fields: `entity_views` for the registry checks, `build_validation_entities` for the rules and
the compiler passes, and `stringify_field_value` for custom validators. Two shapes derived from the
second: the pass input's entities and the coverage rule's. Who owes obligations was decided in four
places: `obligated_kinds`, `obligation_exempt`, the rule engine's target filter and the verify-stub
fix. The copies disagreed:

- A variant list was `"a | b"` to rules and passes and `null` to custom validators; mixed lists,
  expressions and type unions reached no reader; an empty block was absent for rules and `""` for
  validators.
- A `no_verify_statements` rule without a target kind fired on entities of every kind, while the
  coverage rule, stats and the pass input's `exempt` treated it as obliging none.
- The verify-stub fix attributed W004 to entities W004 exempts: 71 union types on this repository,
  where the inserted stub broke the file (E001).
- `file_exists` resolved paths against the process's working directory.
- The rules' entity list was built twice per compile with a check pass, and again for the coverage.

Two exemption fixes in two days (57cb4e8c, c8e03fa0) touched these copies.

## Decision

`specforge_project::snapshot::EntitySnapshot::of(&Graph, &RegistryBuild, spec_root)` is taken once per
compile (`CompiledProject`) and per session check (`ProjectSession`) *(ADR 0047: by the compiled project, at each check, one-shot or a session's)*, and every reader after the graph
build reads it:

- the registry checks and the rules read its records;
- the pass input and a custom validator's context are adapters over it; the coverage rule reads the pass
  input's entities (`specforge_coverage::Entity::from(&PassEntity)`), on the host as in every extension
  pass *(amended, round 6 plan 12)*;
- the coverage view, plan validation, stats and the verify-stub fix read its standings.

Operations reach it through the project view (`ProjectView::entities`), seeded into the per-compile memo
that already holds the coverage. The memo is bound to its snapshot when it is made (`RecordedCoverage::of`
with the snapshot the checks read, or `RecordedCoverage::over(graph, env)` for a graph assembled
elsewhere, which takes it with `Environment::entity_snapshot`): it is never asked for a snapshot with
inputs of its own, so no caller can score a graph with another's registries or spec root. A session
whose update skipped the checks makes its memo over its own graph on first use; the LSP's stand-in
graph carries the snapshot its session held for that graph.

### Placement

The per-entity record is a plain, graph-free struct in the registry crate
(`specforge_registry::entity::EntityRecord`, with `RuleInput { entities, edges, spec_root }`). It
replaces `EntityView` and `ValidationEntity`. The project builds it, since only the project sees the
parse tree; the registry, which must not depend on the graph, reads it. There is no trait: a trait
would mirror the struct and need a test-only twin.

### Field text

Every field an entity writes has exactly one text (`snapshot::field_text`):

| Value | Text |
|---|---|
| string, identifier, date | as written |
| integer, boolean | its literal |
| list of strings or references, mixed list | items joined `", "` |
| variant list, type union | members joined `" \| "` |
| expression group | expressions (display form) joined `", "` |
| verify statements | texts joined `"; "` |
| block | keys joined `", "` |

An empty value is `""` and is written: `missing_required_field` does not fire on it and
`non_empty` does. A name written twice has its last value. Nothing is dropped or null. Every text that
existed before is unchanged.

A joined list cannot be split back when an item itself contains the joiner: `["a, b", "c"]` is
`"a, b, c"`. Readers inside the host never split a text; the record keeps a list's items unjoined
beside it (`FieldRecord::items`, never on the wire).

### Standing: the one obligation rule

`snapshot::Standing::of(record, registries)`:

- **testable**: the kind's registry entry says so;
- **rule**: the first `no_verify_statements` rule, in code order, that applies to the kind (a rule
  without a target kind applies to every kind, the reading every other check kind has; a kind that
  accepts no `verify` is then exempt, below, so no rule demands what it cannot hold);
- **exemption**: a union body, a set field whose registry entry declares `exempts_obligations`, or a
  kind that accepts no `verify` statements (nowhere to declare them; this one exempts only from
  statement obligations);
- **declared**: the number of `verify` statements.

It **owes** obligations when a rule applies and nothing exempts it; it **counts** toward coverage when
testable and owing or declaring; it is **exempt** (the coverage view's word) when testable and neither.

- The counting formula is stated once, in the coverage rule: `specforge_coverage::Counting` (testable,
  owes, declared) answers `counts` and `exempt`. `Standing::counts`/`Standing::exempt` (through
  `Standing::counting`) and the pass entity's `Entity::counts_toward_coverage` read it. Every other
  reader (inspect included) borrows the snapshot's `Standing`; none re-derives it. *(Amended in round
  6, plan 12. This bullet kept two formulas, pinned equal by a test, because sharing one re-vendors the
  builtin blobs that link `specforge-coverage`. That cost is paid once, and round 6 re-vendors every
  blob anyway; commit b6ac628e reverted the shared function for that cost alone.)*
- The pass input's and the coverage rule's `owes_none` (named `exempt` until protocol 1.2.0, ADR 0050) says the entity
  owes none. It is not the coverage view's
  *exempt* (`Counting::exempt`): an entity that owes none and declares obligations counts.
- The verify stub is offered to an entity that declares none and either owes obligations or is of a
  testable kind nothing exempts. It fixes the rule that reports it, if any.

The registry keeps one predicate, a rule's applicability to a kind, shared by the rule engine and
the standing.

### `file_exists`

A relative path resolves against the spec root, as core validation's `file_reference` check does.

### Wire

`PassEntity` and `ValidatorContext` keep their shapes and Rust types. Their values follow the rules
above, which extension authors see:

- pass entities carry every written field;
- a validator's field `value` is always a string;
- `exempt` follows the one rule.

The protocol version moves from `1.0.0` to `1.1.0`. A minor version moves when a payload's values
change meaning or an optional field is added; the major moves only when an older guest could no
longer be decoded or answered. The host still accepts every guest of its major, so guests built with
the 1.0 SDK keep loading; a guest built against 1.1 says so in its handshake and its registry
manifest. ADR 0013 D6 kept `1.0.0` for one additive optional field; from this ADR on, such a change
moves the minor too. `docs/extension-protocol.md` keeps the table of what each version guarantees,
and the SDK documentation states the field-text rule, including that a joined list cannot be split
back when an item contains the joiner.

## Consequences

- A custom validator sees `"a | b"` where it saw `null`, and text for mixed lists, expressions and
  type unions.
- A compiler pass sees keys it did not see before (empty variant lists and blocks, mixed lists,
  expressions, type unions).
- A project with an untargeted `no_verify_statements` rule counts those entities toward coverage.
- The LSP and MCP offer no verify stub on union types or exempt entities, nor on non-testable kinds
  no rule obliges. A stub's diagnostic code is that of the rule that reports its entity, else none.
- `file_exists` no longer depends on the directory the binary runs in.
- The builtins' outputs and this repository's `specforge check` are unchanged; their handshakes say
  protocol `1.1.0`, so their vendored blobs are rebuilt once.
- An untargeted `no_verify_statements` rule no longer fires on kinds that accept no `verify`.

## What would reopen this

- A reader that needs a field's structure, not its text (a validator that must tell a one-item list
  from a scalar). Add a structured value beside the text; do not change the text. Reopened by
  ADR 0031 for the host's own checks: `FieldRecord::shape` and `value_span` (never on the wire).
- A kind of obligation other than `verify` statements counting toward coverage.
- Graphs large enough that one snapshot per check costs more than an incremental patch.

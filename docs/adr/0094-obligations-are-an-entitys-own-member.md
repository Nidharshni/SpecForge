# Obligations are an entity's own member

**Status:** accepted (2026-10-15, round 10, plan 01). Extended by [ADR 0222](0222-what-an-entity-holds-is-written-once.md): the node's methods and annotations are written beside its `verify`. Amends [ADR 0002](0002-test-runner-extensions.md) (placement of
`verify` statements; W158 for W020), [ADR 0019](0019-one-entity-snapshot-per-compile.md) (field text has no statement row),
[ADR 0064](0064-a-declared-field-type-decides-one-wire-form.md) (D5 restored: the graph node's own `verify`; `FieldJson`
without `VerifyStatement`) and [ADR 0032](0032-one-graph-build.md) (the incremental check compares
obligations). The decisions of those ADRs otherwise stand.

The parser wrote an entity's `verify` statements into its `FieldMap` as a pseudo-field: the key `verify` with the value
`FieldValue::VerifyList`. The grammar also lets a user declare a real field named `verify` (`verify string @optional`),
on purpose, and this repository's own spec does (`TestResult`). The two collided:

- the graph export kept whichever was written last, so a real field was dropped, or, with only the real field,
  exported `"verify": "string"` where the published schema promised an array of `VerifyStatement`;
- W020 waved through any field named `verify` on a testable kind, and gave a real field on a closed kind the
  statements' help text;
- the hover, the delta, the MCP search and list filters, and the pass and validator wires each read the statements
  back through the field map; seven readers matched the variant, four the key, nine rode along as carriers.

Obligations were already apart in the parser (`parse_block_body` returns them beside the fields and the methods) and
in coverage (the entity snapshot's `obligations`). Only the three block parsers merged them back.

## Decision

- D1. Obligations are a member of `Entity` and of `Node`, next to `methods`: `obligations: Vec<VerifyStatement>`. The
  `FieldMap` holds only fields. `FieldValue::VerifyList` and `ValueShape::Verify` are deleted.
- D2. The change landed in an expand-and-contract order (member added; readers moved; pseudo-field deleted), with four
  temporary skips that the last commit deleted. Not one atomic commit of about 70 files.
- D3. The rule vocabulary keeps `verify` as the descriptor's name for the statements: `no_verify_statements` with
  `field: "verify"` and `missing_field_when_flag_set` with `field: "verify"` both mean "the statements". A real field
  named `verify` cannot be required by a rule. A `missing_field_when_flag_set` rule on `verify` that targets a kind
  accepting no statements is now W112, where it silently never fired.
- D4. The incremental-versus-cold check compares `obligations`; the graph delta reports a statement edit as the node
  aspect `obligations`, beside `methods`, never as a changed field.
- D5. `Node::new(id, kind, span)` builds test literals, so a member added later costs one line.
- D6. `Entity.raw_body`, `extract_brace_body` and `ParseError.expected` / `found` are deleted. The body parsers
  `raw_body` was kept for went with [ADR 0004](0004-architecture-migration-decisions.md) D5-a; nothing read the
  others (a test read `found`; it reads the message now).
- U1. Statements on a kind that accepts none are **W158** (warning, core), not W020, because statements are not
  fields. It runs for `open_fields` kinds too and skips an unregistered kind (already E024). A real field named
  `verify` on a closed kind is W020, like any undeclared field.
- U2. The graph export writes obligations under the node's own `verify`, the key the context export already writes, so
  the published schema's `node_<kind>.properties.verify` comes from `JsonNode`'s shape and `fields_<kind>` has no
  `verify` unless a kind declares one.
- U3. A pass's `PassEntity.fields` and a validator's `ValidatorField` lose their `verify` entry (the statements' text).
  A pass keeps `verify_kinds` and `verify_texts`. Protocol minor `1.4.0` to `1.5.0` ("a field removed"); no builtin
  guest read it.
- U4. `ParseError.expected` is deleted, not promoted to a suggestion: nobody read it, and two of its strings were
  broken literals.

## Consequences

A graph export gains a node-level `verify` and a field named `verify` is exported under `fields`. The published schema
loses `fields_<kind>.verify` and `FieldJson`'s `VerifyStatement` item. The hover lists an **Obligations** section
apart from **Fields**; MCP inspect's `fields` no longer repeats `verify_declarations`. A third-party pass or validator
that read `fields["verify"]` loses it. The protocol-types docs and version change, which is a blob input: the
coordinator re-vendors once for the round.

## Rejected

- A key no user can write (`"$verify"`) for the pseudo-field: every reader keeps its special case, and the field
  writer still lies.
- One atomic commit: unreviewable.
- Growing the validator wire with `verify_kinds` / `verify_texts`: no reader.
- Renaming the export key to `obligations`: the context export would have to follow, for no reader.
- Promoting `ParseError.expected` to E001 help: it changes every E001 snapshot and needs its strings reviewed; a
  separate change if the owner wants better help.

## Reopen

When a rule must require a real field named `verify`, or when a reader of raw body text appears.

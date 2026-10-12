# An edge type is resolved once in the registry build

**Status:** accepted (2026-10-12). Supersedes ADR 0007's round-6 amendments "an open edge type takes the kinds of
its writers" and "an undeclared edge type names every writer"; amends ADR 0019 (the record's edge counts), ADR 0020
(D1's edge-scoped rule, D2's input, D5's cycle rule) and ADR 0032 (the inverse pairs). Protocol 1.3.0: an enhancement
declares no edge types (D7).

Which edge type an edge is, and where an edge type starts and ends, was answered four ways: the rule set kept the
*names* of the fields that write a label; the rule check counted edges to or from the far end's kinds, whatever
field wrote them; the cycle check followed those field names from any kind; the schema walked the field registry
again to fill an open side. A `Mentions` edge satisfied an `Owns` rule, an `NDep` loop was reported as an `MDep`
cycle, a field writing a declared edge type from outside its declared ends was dropped without a word, an
enhancement's field could not write an undeclared edge type, and trace and the graph's bidirectional pairs each
derived a field's inverse differently. ADR 0007's round-6 amendment kept the inference in the emitter "until another
reader needs an edge type's endpoints"; the rule set had read them since 2026-10-06.

## Decisions

- **D1. The registry build resolves each edge type once** (`compilation::edge_types::resolve`, read through
  `RegistryBuild::edges()`): its declaration if any, its writers (every registered field naming it as its `edge`: a
  kind's own, shared and enhancement fields, by extension in load order, then kind and field), its ends (the declared
  kind on a side it declares, else its writers'; sorted) and, for every `(kind, field)`, the edge type it writes
  (`EdgeRegistry::written_by`). An edge label no extension declares is an edge type of its writers, owned by the
  first writer's extension (W021 on each field). The entry has no public field.
- **D2. A declared end is a contract.** A field writing a declared edge type from a kind other than its declared
  source, or to a declared target it does not target, is W021 and writes no edge of that type. A writer that
  declares no target is admitted.
- **D3. An edge's type is read once, in the entity snapshot** (`EdgeRecord.edge_type`, `EdgeCounts.by_type`), from
  `written_by` and the kind the edge starts at. An edge whose field writes no edge type has none and counts in the
  total only.
- **D4. Edge rules count the edges of their edge type; cycle rules follow them.** A rule is inert when no loaded
  field can write such an edge at its target kind (ADR 0020 D5, per writer). `Rule::describe` lists the rule's
  `edge_writers`.
- **D5. The schema prints the resolved ends;** `generate_schema(&RegistryBuild)`; the emitter infers nothing and
  its extensions are the build's.
- **D6. A field's inverse fields are resolved once** (`FieldRegistry::inverse_fields`: fields of the target kind);
  trace and the graph's bidirectional pairs read them.
- **D7. An edge type is declared in `edges` only.** `EntityEnhancementDescriptor::edge_types` is gone (protocol
  1.3.0): an enhancement's fields name an edge type as their `edge`, and `edges` declares it. A 1.2 guest's
  `edge_types` key on an enhancement is W138 and ignored. Doctor lists, for each enhancement, the distinct edge types
  its fields write, in field order.

Rejected: widening declared ends to their writers (the schema would publish ends no extension declared); typing
edges inside each rule (two readers, two mappings); a W021 for a dangling `inverse_of` (two builtin declarations
would warn); registering the enhancement's `edge_types` as a second declaration site (formal's seven would be W018
against itself).

## Consequences

- An extension whose rule's kinds are linked by several fields sees its rule fire where another field used to
  satisfy it. No builtin rule changes meaning; this repository's `check` is unchanged.
- New W021 cases: a writer outside a declared end; a shared or enhancement field naming an edge label its
  extension does not declare.
- An edge rule over an edge type nothing writes is inert (before, an open far end counted every edge).
- The registry snapshot lists each edge type's `sources`, `targets` and `writers`, and each edge rule's
  `edge_writers` (no `edge_peer_kinds`, no `edge_fields`).
- An `inverse_of` that names no field of the target kind is no inverse: it no longer exempts a pair from W061 nor
  counts as declared from the other side in trace.

- Doctor's enhancement listing names the edge types the fields write (software's `milestone` gains
  `edge MilestoneIncludesBehavior`, `module` gains `edge ModuleConsumesPort, edge ModuleDefinesPort`); formal's lines
  are unchanged. Every builtin is re-vendored once for protocol 1.3.0.

## What would reopen it

- A field that writes more than one edge type (by target kind): `written_by` then takes the far end's kind.
- An output that needs each writer pair rather than the ends (the model's relationships draw the product of an edge
  type's sources and targets).

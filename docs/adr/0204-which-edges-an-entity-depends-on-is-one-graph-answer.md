# Which edges an entity depends on is one graph answer

**Status:** accepted (2026-10-21, architecture round 22, plan 05). Amends [ADR 0076](0076-the-plan-check-reads-the-snapshot-and-takes-its-spec-type.md) D5 and
[ADR 0093](0093-a-fields-declared-type-decides-what-its-value-references.md) D7; affirms ADR 0076 D4 and
[ADR 0196](0196-which-entities-lie-on-a-cycle-is-one-answer.md) D1.

## Context

- The plan check read every graph edge as "the target comes first" and knew nothing of cycles: two mutually
  recursive types that compile with no diagnostic could not be planned in any order, nor could the repository's own
  `Block`, `FieldEntry` and `FieldValue` (six of six orders failed).
- "A recursive type is not a reference cycle" lived only in the build's step order: W061 was computed in the middle
  of `link`, over edges staged by add, check, clear and re-add, before the derived edges joined. `Graph::cycles()`
  meant the written references when `link` asked and every edge when anyone asked afterwards.

## Decision

- D1. Every edge `a -> b` says `a` depends on `b`, a written, single or derived reference alike (ADR 0076 D4).
  `Graph::cycles()` answers the cycles of that relation; asked of a built graph it has one meaning.
- D2. Entities on one cycle depend on one another: one component, which a plan lists in any order among itself.
  Every entry of a component comes after every entry of each component it has an edge to; a gap names the
  depending component's first entry and the needed component's last, and the edge. A component of one entity is
  the entity: on a graph without cycles the check is the edge-by-edge one, gap for gap.
- D3. W061 reads a narrower relation: the written references but a declared single reference's (ADR 0093 D7). The
  linker names it from each edge's own `single` flag; a derived edge is never in it. The graph is linked once, in
  written order; no build step order carries a rule.
- D4. `specforge-cycles` is unchanged; the components are `Cycle.members`.

## Rejected

- W061 over every edge: it would warn on every recursive type.
- The plan over W061's edges: it would drop the derived and single dependencies from the order.
- A provenance field on `Edge`: about 80 literals change for a fact only the linker needs.
- A label predicate over the finished graph: it misjudges a hand-written value of a derived field.
- A component order in `specforge-cycles`: no reader, and it costs a re-vendor.
- `dependencies` returning on `AgentPlan`: ADR 0076's reopen clause is for an order the graph cannot state, not for
  a cycle it does state.
- Skipping the edges inside a cycle and checking every other edge as before: it passes a dependent placed between
  the entries of a cycle it depends on.

## Consequences

- Plans through a cycle pass. A plan that places an entry between the entries of a cycle it depends on fails with a
  new `gap_context` form naming the late entry. No JSON shape changes.
- No edge, edge order or diagnostic of any build changes; no re-vendor.

## Reopen when

A plan must be ordered through entities it does not list (transitive ordering), or a surface needs W061's relation
from the graph rather than from the diagnostics.

# The graph owns directed reach and degree

**Status:** accepted (2026-10-20, architecture round 18, plan 04). Amends [ADR 0015](0015-read-views-are-operations-over-the-project-view.md) P3 (the degree is the graph's; the export's budget reads it) and its operations list (`trace::trace` answers `TraceOutcome::{Chain, Every}`; `trace::reach` is `trace::affected`).

## Context

- `specforge trace` with no entity kept its own breadth-first search that rescanned every edge of the graph for each
  entity it popped, beside the graph's indexed `Graph::reach`. Over the repository spec (2,213 entities, 6,011 edges)
  that was 381,337,840 edge reads and 1:05 in the debug build, against 1.1 s for `stats`.
- Two modules defined an entity's degree: the export's token budget counted a self-reference twice, the project
  view's connectivity not at all. With `feature selfish { depends_on [selfish] }` beside a linked pair, `export
  --max-tokens` dropped the pair before the entity `stats` calls unconnected.
- `28b02f5a` unified the subgraphs' breadth-first walk and left trace's; `bc1b6d74` unified the connectivity rule and
  left the budget's.
- `TraceOutcome` carried a `single: bool` that a hand-written `Serialize` and `into_chain` read, and MCP kept an error
  arm for a combination that cannot happen. `trace::reach` shared a name with `Graph::reach` and meant something else.

## Decision

- **D1.** `Graph::reach_directed(root, Direction)` walks one way over `source_index` / `target_index`, each entity's
  edges in written order, reading only the edges of the entities it reaches. `Graph::reach` (both ways, `(id, label)`
  order) and it share one private core, `breadth_first`. One function with one order would change either 55 trace
  labels of the repository spec or explore's golden paths, so the order stays per function.
- **D2.** `Graph::degree(id) -> Degree` counts the edges to and from other entities; a self-reference counts nowhere
  (CONTEXT.md "Unconnected entity"). The budget ranks by it and `Connectivity` maps it over the nodes.
  `specforge_ops::view::Degree` is `specforge_graph::Degree`.
- **D3.** A trace's outcome is the document it serializes as: `TraceOutcome::Chain` for an entity, `Every` for all;
  `trace::chain` and `trace::every` are the two operations, `trace` dispatches. `single`, `into_chain` and MCP's arm go.
- **D4.** `trace::reach` and `Reach { affected, .. }` are `trace::affected` and `Affected { entities, .. }`.

## Consequences

- The trace of every entity is linear in its output: 83,514 edge reads, 2.4 s, byte-identical documents.
- `export --max-tokens` truncates a self-referencing entity as its other edges say. No tracked project has one.
- Rust interfaces of `specforge-ops` change (`TraceOutcome`, `affected`, `Degree`); the CLI and MCP documents and
  MCP's output schema do not.

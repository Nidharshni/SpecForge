# A read selection decides what the read covers

**Status:** accepted (2026-10-21, architecture round 20, plan 07). Amends [ADR 0007](0007-emitter-is-export-formats.md) (`emit` exports the entities a selection covers; it no longer resolves a scope), [ADR 0015](0015-read-views-are-operations-over-the-project-view.md) (Q2: the kind filter keeps the queried entity because the root frames a drawn subgraph; Q4: `ScopeNotFound` is gone) and [ADR 0177](0177-a-reads-selection-is-checked-by-the-project-view.md) (S1: the selection also answers its entities, `Selection::selected`; `Selection::reach` is gone).

## Context

Since ADR 0177 the project view checks a read's selection once, but each read decided for itself which entities the
selection covers. Six spellings of one rule: the emitter walked scope, depth and kinds a second time (keeping the
scope's own entity whatever its kind); explore, coverage, review and list each filtered by hand; the induced subgraph
was written three times. The spellings disagreed in one live place: a scoped export under a token budget cut its own
scope entity (`export --scope leaf --max-tokens 80` held everything but `leaf`), against the export and query specs'
"the scoped entity always kept".

A selection's entity plays three roles, by design: the origin of a reach (export, query, explore, review), the subject
alone (coverage) and a reference target (search, which covers nothing). Explore and coverage drop the entity under a
kind filter it does not match (specified, ADR 0015 round-18 Q3); export and query keep it.

## Decision

- **R1** `Graph::select(root, depth, kinds)` is the one rule: what the root reaches within its depth, or every entity,
  of the kinds kept. The root is a member only when its kind is kept, and `Selected::drawn` always includes it.
- **R2** `Selection::selected(default)` is every read view's call; coverage reads it at no hops.
- **R3** The emitter takes `Selected` (`EmitOptions::selected`) and never resolves a scope; `EmitterError::ScopeNotFound`
  is gone, the selection refuses a missing scope (E003) first.
- **R4** A token budget never cuts the root. A budget below the root alone is E062, naming the scope entity.
- **R5** `Graph::induced` is the one induced subgraph.

## Rejected

- The root always a member (explore would list the entity under another kind; reverses ADR 0015's round-18 Q3).
- The root never kept when its kind is not (a query would lose its own subject; breaks ADR 0015 Q2).
- Resolving twice (ops and emitter): the emitter would still re-resolve and keep a refusal only tests reach.
- Dropping the root at the budget floor: the defect at the floor.
- The names "cover" and "anchor": coverage is the test-coverage view and anchors are source anchors (ADR 0140).

## Reopen when

A read needs the root in its rows under a kind filter, or a budget strategy ranks by distance from the root.

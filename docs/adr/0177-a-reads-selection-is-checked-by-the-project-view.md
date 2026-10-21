# A read's selection is checked by the project view

**Status:** accepted (2026-10-21, architecture round 19, plan 01). Amends [ADR 0015](0015-read-views-are-operations-over-the-project-view.md) (Q3: every kind filter is the view's `KindFilter`, export included; Q5 and P3: the view's `select` and `entity` raise `not_found`, and `neighbourhood` is `Selection::reach`; P4: the depth refusal is the selection's) and [ADR 0101](0101-an-operations-request-owns-its-meaning.md) (D1: a depth without its entity is one refusal about the entity, export's included). Amended by [ADR 0190](0190-a-read-selection-decides-what-the-read-covers.md) (S1: the selection answers the entities it covers, `Selection::selected`; `Selection::reach` is gone).

## Context

- The rule "resolve the entity a request names, refuse a depth without that entity, report each kind of a filter the
  project lacks (I020)" was spelled at 24 sites in ops, and each read view had to remember every part. Eleven commits in
  four rounds added the rule to one more operation each; the last five landed on one day.
- It drifted. Export's `kinds` went straight to the emitter, which drops an unknown kind by design, so
  `specforge export --kinds`, `specforge.export` and the graph resources reported no I020 where query, explore, coverage,
  search, list and model did. An empty kind meant "every kind" to `list` alone. A depth without its anchor was refused
  under three codes about three fields (export about `depth`, explore and review about `entity_id`).
- An entity's coverage had three readers (`coverage`, `coverage::row`, `inspect::coverage`) that answered a missing
  entity three ways. No surface reached the divergence: every one inspects first.

## Decision

- **S1.** `ProjectView::select(&Select { entity, named_by, depth, kinds })` is the one check of the entity a read names,
  its depth and its kinds. It refuses a depth without an entity first, then an entity the graph lacks (`view::not_found`),
  both about `named_by`, the request field that names the entity; kinds never refuse. It reads no report and no disk.
- **S2.** `ProjectView::entity(id, named_by)` is the lookup for a read of one entity: its node, record and standing.
- **S3.** Every kind filter is a `KindFilter` (`ProjectView::kind_filter`), which carries its I020 notices;
  `KnownKinds::unknown_in` is private. Export reports I020 like every other read, after the versioning notices; the
  emitter is unchanged (it does not link ops).
- **S4.** One code, `depth_without_entity`, for a depth without its entity, about the entity's field. Export's moves from
  `depth` to `scope`; `DEPTH_WITHOUT_SCOPE` and `ExportField::Depth` go. The model keeps `depth_without_root`: its depth
  counts hops from a kind of the schema, not from an entity.
- **S5.** An empty kind is a kind the project lacks: I020 `unknown entity kind ''`, keeps nothing, on every read.
- **S6.** `inspect::coverage` reads the `EntityFacts` `inspect` returned; `coverage::row` and `coverage::selected` go.

## Rejected

- A `Select` on the requests: surfaces keep spelling `&str` fields and the operation builds the selection (ADR 0101 D1).
- I020 in the emitter (ADR 0015 Q4, ADR 0007).
- The typed-edit planner on the view: it answers from the plan's overlay (created and deleted entities) first.
- Refusing an empty kind: every request field enum with a kind filter would need a `Kinds` variant, for a value no client
  sends on purpose.
- One depth code for all four reads, and the depth as the refused field everywhere (the latter reverses ADR 0101's
  round-18 amendment a day after it landed).

## Consequences

- `view::not_found` is called from four places (the selection, export's total error mapping, the anchors, the typed-edit
  planner) plus MCP's `entity_not_found`, down from fourteen.
- `specforge export --kinds`, `specforge.export` and `specforge://graph|context|brief?kinds=` carry I020.
  `specforge.export {"depth": N}` and `specforge://graph?depth=N` without a scope name `scope`. `specforge.list {"kind":
  ""}` lists nothing, with I020.
- Rust: `view::{Entity, Select, Selection, KindFilter}` and `ProjectView::{entity, select, kind_filter}` are new;
  `ProjectView::neighbourhood`, `view::depth_without_entity`, `coverage::row` and `export::DEPTH_WITHOUT_SCOPE` are gone.

## Reopen when

A read needs a selection the graph cannot answer (an id known only to the anchors, a plan's overlay), or a surface spells
a selection the operation does not hold.

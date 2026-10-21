# The plan check reads the snapshot and takes its spec type

**Status:** accepted (2026-10-13, architecture round 9, plan 06). Amends ADR 0015 (I1, I3, the operations list), ADR 0029
(D1, D2) and ADR 0033 (D1, D2). Amended by round 14, plan 12 (the plan check's key). Amended by [ADR 0204](0204-which-edges-an-entity-depends-on-is-one-graph-answer.md) (D5: entities on one cycle in any order). Amended by round 22, plan 11 (D1: the missing entries are the entities that count toward coverage).

The plan check (`specforge_ops::plan::check`, the plan mode of `specforge.trace` and the trace prompt) asked the
coverage memo for the project's coverage to learn two facts the entity snapshot holds: an entity's kind is testable,
and it declares obligations. Asking read `specforge-report.json`, so a report the check never needed failed it with
E045 — every report written before protocol 1.4 (ADR 0069) did. The plan was an untyped JSON value of which only
`entries[].entity_id` was checked: a plan without `plan_id` or `action`, with keys the type lacks, or with a
`dependencies` list nothing read, was accepted, though the spec says it MUST be an `AgentPlan`; the MCP listing said
`{"type": "object"}`. The spec named `specforge trace --plan`, which did not exist.

## D1. The check reads the graph and the entity snapshot

Its missing entries are the testable entities whose standing declares obligations
(`Standing::declares_obligations`) and that no entry names. It reads no report and cannot fail: `PlanError` is gone.
`ProjectCoverage` keeps what the recorded tests prove (verdicts, summary, `is_unverified`); its `entities()` and
`standing()` are gone, so every snapshot read goes through `ProjectView::entities` and an operation's dependence on the
report shows in its calls. Inspect's coverage is `inspect::coverage(view, id)`, asked for by the surfaces that show it
(MCP inspect, the hover); the context prompt reads no report.

## D2. The plan is its spec type, read against its listed schema

`specforge_ops::plan::AgentPlan { plan_id, entries: [AgentPlanEntry { entity_id, action }] }` derives `Shape`; a value
is checked against that schema (`shape::violations`) and then decoded (`AgentPlan::read`, `parse` for JSON text). A
value it does not admit is refused listing every place it departs (`plan must be an AgentPlan: plan: missing required
plan_id; plan.entries[0]: missing required action`). serde alone was rejected: it decodes a struct from a JSON array
and stops at the first error. The schema checked is the schema MCP lists for the argument.

## D3. A plan is refused where it is read

MCP's `Arg` for `AgentPlan` (an object, or a string holding its JSON, as a prompt's arguments are) and the CLI's
`--plan` value parser call `AgentPlan::{read, parse}`; the refusal is argument reading's (ADR 0033 D2: `invalid_input`
on `plan`; the CLI's exit 2). No operation refusal and no refusal subject is involved (ADR 0072 has nothing to name
here).

## D4. `dependencies` is not part of the type

The graph's edges are the dependencies and the entry order is the plan's order; a second, declared order would have to
be reconciled with both. An entry that still sends it is refused (an undeclared key).

## D5. One document with a verdict, on two surfaces

`PlanOutcome` is the `McpTracePlanResult` document (ADR 0052): `ok`, `affected_entities`, `gaps` (`McpTraceGap`).
`ok` is false when an entry names no entity (E003) or comes before what it depends on; a missing entry is a warning.
`specforge trace --plan <FILE>` prints it (json or human) and exits 0, 1 by `ok`, 2 when the file cannot be read or is
not an `AgentPlan`; `specforge.trace` answers it.

## Consequences

- A stale or malformed report no longer fails the plan check; it still fails stats, the coverage view, inspect, query
  with coverage, the review and the trace prompt's unverified entities, which read what the report decides.
- Plans without `plan_id` or `action`, or with other keys, are refused; `specforge.trace`'s plan reply gains `ok`.
- `PlanValidationResult` and `PlanValidationEntry` leave the spec; `plan_validated`'s payload is `McpTracePlanResult`.

## What would reopen it

A plan that must state an order the graph cannot (two independent entities an agent must sequence for its own reasons):
then `dependencies` returns, with the gaps that reconcile it with the entry order. A second plan format (a file format
for humans): then the decode takes a format, still checked against one schema.

**Amended by ADR 0086 (2026-10-14):** the plan check is `specforge_ops::read::plan::check`.

## Amendment (architecture round 14, plan 12)

`PlanOutcome`'s entries that resolve are `resolved_entries` (was `affected_entities`); "affected" names the reach of a trace (the trace prompt's `affected_entities`).

## Amendment (architecture round 22, plan 11)

D1's missing entries are the entities whose standing counts toward coverage (`Standing::counts`) and that no entry
names. They were the testable entities that declare obligations, so a behavior W004 reports (it owes obligations and
declares none) got no gap although the coverage view, the review and the trace prompt all name it. Every reader of
"who owes proof" now asks the standing's one answer. A missing entry is still a warning: no plan's `ok` and no exit
code change. `Standing::declares_obligations` stays for inspect and the hover, which only display it.

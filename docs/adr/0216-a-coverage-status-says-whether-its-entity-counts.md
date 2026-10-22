# A coverage status says whether its entity counts

**Status:** accepted (2026-10-22, architecture round 24, plan 04). Extends [ADR 0015](0015-read-views-are-operations-over-the-project-view.md) D3; amends [ADR 0007](0007-emitter-is-export-formats.md) (round-9 amendment: `EmitOptions::coverage` carries statuses) and [ADR 0019](0019-one-entity-snapshot-per-compile.md) (the counting bullet: `Standing::status`, no `Standing::exempt`); ADR 0015 I5's `exempt` field is gone.

## Context

- The coverage rule answered an entity's status as `covered`, `partial` or `uncovered` from its verdict alone, with no value
  for "this entity does not count toward coverage". Six readers each added the standing back, or dropped it, on their own:
  the coverage row carried `exempt` (false for a kind that is not testable) and a `testable` nobody read; inspect wrote
  `exempt` beside a verdict-only `coverage_status`; query and export stamped every node with the verdict-only status; the
  LSP hover re-derived "exempt" and "not testable" in three branches; `ProjectCoverage::status` was a fourth, unread copy.
- On the shipped todo-app, a query with coverage stamped 11 of 27 reached entities that do not count as `uncovered`; a
  feature and a W004 behavior got identical rows; `status_filter: uncovered` left a union out while the same filter with its
  `entity_id` returned it; a proven entity of a kind that is not testable read `covered` everywhere but the hover.
- Eleven commits over five crates had moved the status or the standing beside it; none put them in one answer.

## Decisions

- **D1. The coverage rule answers one status per entity**, `Counting::status(&Verdict)`: `not_testable` when its kind is not
  testable, `exempt` when it is testable and owes and declares no obligations, else the verdict's `covered`, `partial` or
  `uncovered`. `Verdict::status` is the rule's own (crate-private).
- **D2. Every reader writes it**, asked through `Standing::status` (the coverage row, inspect's coverage) or
  `ProjectCoverage::statuses` (export nodes, `EmitOptions::coverage`). No document carries `exempt`; inspect keeps `testable`,
  `declared` and `obligated` (the reason an entity is exempt).
- **D3. The coverage view**: no status filter lists the counted entities, or the named one; a status filter selects the
  entities of that status, named or not. Extends 0015 D3 and honours D4's rejection of flagging exempt unions.
- **D4. The hover renders the status for every entity**, and the report's error for every entity when the recorded report
  cannot be read (ADR 0015 I3: the coverage carries the error; the standing shows in the badge).

## Rejected

- The status beside `Standing` in the registry: it leaves the verdict-only status public in the leaf, so a host could keep
  writing it.
- A `counts` flag beside a three-valued status: two keys to read, and the status still says `covered` for what does not count.
- `not_counted` for both reasons: a new word beside **exempt**, hiding why.

## Consequences

- Breaking (MCP `specforge.coverage`, `specforge.inspect`, `specforge.query`, the review prompt, CLI `--include-coverage`,
  `specforge review --format json`): `status` and `coverage_status` take five values and no document carries `exempt`;
  `status_filter` accepts `exempt` and `not_testable`.
- The builtins that link `specforge-coverage` (formal, product, testing) are re-vendored once (ADR 0019's precedent).
- No new diagnostic code.

## What would reopen it

A sixth reason an entity does not count, or a surface that must tell "unproven" from "owes nothing" without the status.

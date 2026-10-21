# A typed edit is judged by the graph change it makes

**Status:** accepted (2026-10-21, architecture round 21, plan 07). Amends [ADR 0151](0151-a-spec-file-is-changed-by-a-typed-edit.md) D3 (a second condition for writing). Cites [ADR 0167](0167-a-typed-edit-is-compared-with-the-checked-report.md) D1 (E080 is unchanged and judged first), [ADR 0186](0186-a-migration-is-judged-before-it-is-written.md) D1 (the migration's declared-change pattern) and [ADR 0032](0032-one-graph-build.md) (the delta's invariant).

## Context

A typed edit's verb promises one change ("this field of this entity", "this obligation", "this title"), but its only judgement was E080, "no new error": the edited compile's graph was built and dropped after its diagnostics were read. A positional bug that writes the right text in the wrong place passes it silently: a remove-verify deleted a port's `methods` entry and a retitle overwrote a union's variant, both exit 0 with nothing added. About 3 of the 26 typed-edit defects of the module's history were of this class. A migration already closes it: its graph delta, less the changes its steps declare, must be empty (ADR 0186).

The whole-graph delta costs a third of an edited compile on this repository's spec (187 files, debug). A delta over the files the edit changed, read through the graph's edge indexes, was equal to it on every edit measured and costs about 0.4% of the compile.

## Decision

- **D1.** Each typed edit declares the graph change it may make: set and unset their field (the part of a `field.entry` path before the dot), link and unlink their field, retitle the title, add-verify and remove-verify the obligations, create and delete their entity whole, a delete with `unlink_references` each referrer's field. A list's declaration is the union of its edits'.
- **D2.** The plan takes the delta of the edited compile over the files it changes (`compute_graph_delta_over`), removes the declared changes and refuses any remainder as `undeclared_change` (`CompilationFailed`, data `{"delta": remainder}`). E080 is judged first.
- **D3.** An edge is declared by a declared field of its source, or by a created or deleted end (derived edges carry the derived field's label, not the written one).
- **D4.** The report's entities stay textual (ADR 0184, amended by round 20 plan 09).

## Rejected

- The full `compute_graph_delta`: a third of the compile per plan.
- Label-precise edges: more interface (the registry's derived fields) for a case no defect has shown.
- A "declared change happened" check: a set whose value is graph-equal to the old one would be refused wrongly, and a no-op answers "nothing to write" by design.
- A test-only oracle: the defects were user-shaped files no corpus held.
- A catalog code: the refusal is about the tool, never a diagnostic `check` reports.
- Taking the report's entities from the delta: it would stop naming a block moved to another file and reverse ADR 0184 the day it was written.

## What would reopen it

A typed edit that legitimately changes another entity than it names (a verb that writes an inverse side, ADR 0157), or a release preview that measures the judge above 10 ms.

## Consequences

- A new refusal on CLI (exit 1) and MCP (`compilation_failed`), reached only by a defect of a typed edit; nothing is written, a preview refuses it too.
- One edited compile as before, plus a delta over the changed files' nodes.
- `specforge_graph::compute_graph_delta_over` is public: its `files` must include every file whose text differs between the two builds.
- No new diagnostic code; no blob input changes.

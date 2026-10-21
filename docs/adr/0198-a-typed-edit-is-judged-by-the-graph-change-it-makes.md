# A typed edit is judged by the graph change it makes

**Status:** accepted (2026-10-21, architecture round 21, plan 07). Amends [ADR 0151](0151-a-spec-file-is-changed-by-a-typed-edit.md) D3 (a second condition for writing). Cites [ADR 0167](0167-a-typed-edit-is-compared-with-the-checked-report.md) D1 (E080 is unchanged and judged first), [ADR 0186](0186-a-migration-is-judged-before-it-is-written.md) D1 (the migration's declared-change pattern) and [ADR 0032](0032-one-graph-build.md) (the delta's invariant). Amended by round 22 plan 07 (2026-10-21): a rename declares a substitution; a capture refuses it.

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

## A rename declares a substitution (amendment, architecture round 22, plan 07)

### Context

A type name written in a member field or a method signature is open-world: a name no entity declares links nothing and
reports nothing. Renaming `Session` to `Token` therefore made an unrelated `value Token` name the renamed block (a new
edge `ApiKey -[composed_types]-> Token`), and renaming it to `string` made every `user: string` parameter name it with
no new edge to show for it, because the port already pointed at `Session`. A rename had five precondition guards, each
added after a bug, and no rule for what it may change. An edited compile to judge it would cost about 1.0 s (debug) on
this repository's spec, per editor rename.

### Decision

- **D3 split.** A create declares every edge into its entity, the names already written for its ID included (a forward
  reference, E004's remedy, the E003 create stub): that is what declaring the entity means, now stated in
  `validate_typed_edit`. A rename declares a substitution of its new ID for its old one and nothing else.
- A name the project writes for no entity that the new ID would make name the renamed entity is a **capture**. The
  rename asks the graph's finder (ADR 0163) over the compiled parses, with the renamed entity's kind for the new ID, and
  refuses any before planning: `name_captured`, a conflict about `new_name`, naming each `file:line (holder, field)`.
  A name in the renamed block's own body is not one (a block derives no edge to itself).
- A source that does not parse and spells the new ID refuses too (`source_unparsed`), as it does for the old ID.
- E080 stays off for rename (ADR 0151, round-15 amendment); the rename's graph delta is not its judge.
- The answers to the plan's open questions: U1 = a (a capture guard only; the edited-compile judge is not built);
  U2 = a (a typed create's capture of an open-world name is intended language).

### Rejected

- The graph delta as the capture test: the primitive case adds no edge (an edge is a (source, target, label) set).
- E080 modulo the substitution: it would rewrite diagnostic prose.
- One judge module shared with migrate: the edited compile cannot see every capture, and migrate's judge differs.
- A graph helper for the hypothetical walk: two callers of a three-line idiom, each filtering differently.
- Refusing a host primitive's name in the core: the core has no notion of a primitive type.

### What would reopen it

A rename defect that changes the graph and that no precondition can name ships (then judge it by an edited compile), or
the core gains a notion of a primitive type.

### Consequences

- MCP `specforge.rename` and LSP `textDocument/rename` refuse such a new ID (`conflict` / `invalid_params`); no schema
  change. `specforge_ops::authoring::rename::CAPTURED` is public.
- There is no way to capture on purpose: rename to a free ID, then retype the field.
- A `use` binding left naming nothing by a rename is not this decision's (`use` is not a finder token).

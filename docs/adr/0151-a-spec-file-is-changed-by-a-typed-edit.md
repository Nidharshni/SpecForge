# A `.spec` file is changed by a typed edit that writes the canonical form

**Status:** accepted (2026-10-18, syntax round, plan 05). Extends [ADR 0128](0128-one-project-change-over-held-files.md)
(a project change) and [ADR 0021](0021-formatting-is-one-operation.md) (format) to a new writer.

## Context

SpecForge's direction is that nobody hand-writes `.spec`. Until now only `rename` wrote entity text; every other
change was a hand edit, checked after the fact by `specforge check`: a misspelt field was a warning, a dangling
reference an error found only once the file was saved.

## Decision

- **D1.** One operation, `specforge_ops::authoring::edit`, plans a list of typed edits (create, set, unset, retitle,
  link, unlink, add or remove a verify obligation, delete) and writes them as one project change.
- **D2.** A value is written in the form its field's declaration decides (the one-value-form rule, ADR 0147), and
  each touched block is laid out by the formatter. Bytes outside the touched blocks do not change.
- **D3.** Nothing is written unless the edited project reports no error it did not report before (**E080**). The
  edited project is compiled from the planned texts; diagnostics are matched without their positions. Warnings are
  written and reported.
- **D4.** The CLI spells one edit per verb (`new`, `set`, `unset`, `link`, `unlink`, `delete`); MCP takes a list
  (`specforge.edit`) applied all or nothing. A preview (`--dry-run`, `dry_run`) plans and validates the same way.
- **D5.** A field a block inherits from its file's defaults (ADR 0149) is copied into an own field before it is
  changed, and an own field equal to the default is removed. No edit writes a `defaults` block.
- **D6.** `specforge new --extension` is removed; `specforge extension init` is the one scaffolder, and `new` creates
  an entity.

## Rejected

- Six MCP tools, one per verb: no atomic multi-edit, six reply types.
- Validation by hand in the planner: it would repeat the compile's checks and drift from them.
- Formatting the whole file: it rewrites unrelated text in a file that is not canonical.
- A positional `title` field: `title` is a legal type member name, so the header title is `--title` / `retitle`.

## Consequences

Every create, change and delete of an entity has a typed route; the todo-app is rebuilt from its graph by typed edits
alone (27 of its 30 blocks; the rest are the `spec` block, a union and a port with methods). Unions, port methods and
doc comments are not covered yet, and a create writes no annotation (a set does).

## What would reopen it

A project large enough that a second compile per edit is too slow: validation would then move to the session's
incremental update.

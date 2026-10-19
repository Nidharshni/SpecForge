# A `.spec` file is changed by a typed edit that writes the canonical form

**Status:** accepted (2026-10-18, syntax round, plan 05). Extends [ADR 0128](0128-one-project-change-over-held-files.md)
(a project change) and [ADR 0021](0021-formatting-is-one-operation.md) (format) to a new writer. Amended by [ADR 0153](0153-a-value-is-spelled-by-one-module.md) (D2: a multi-line value is laid out with the file's format configuration), by [ADR 0154](0154-a-typed-edit-asks-its-files-defaults-over-the-text-it-plans.md) (D5: an edit reads the defaults from the text it plans, and removes a field that restates its default by W161's comparison of what it wrote) and by round 15 plan 06 (2026-10-19): rename stays its own operation. D5 extended by [ADR 0156](0156-a-delete-finds-references-by-the-one-reference-rule.md): a delete whose entity is named in a `defaults` block is refused (`not_editable`), naming where. Amended by [ADR 0157](0157-a-typed-edit-names-a-relations-owning-side.md) (the other side of a relation; how a list value is taken). Amended by [ADR 0158](0158-an-edited-compile-and-a-migration-read-no-source-past-the-bound.md) (2026-10-19): D3: the edited project is compiled from what the project holds and reads no source.

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

> **Amended by round 15 plan 06 (2026-10-19):** rename is not a typed edit. Its plan is token positions: the LSP
> hands them to the editor, and MCP returns them (`McpRenameEdit`). A typed edit's plan is file texts. E080's
> position-free match would count an error the project already has about the renamed entity as a new one, since
> its message names the ID. What a rename can add that `check` refuses is a reserved word (E013), so rename and
> create refuse one before anything is planned (`KindRegistry::reserves`). The write path is shared below both
> (ADR 0128, and the change entry point of round 15 plan 03). Reopen if a second ID-dependent error class
> appears, or if typed edits must rename inside a list.

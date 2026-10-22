# A parsed block answers its body shape, union or braced

**Status:** accepted (2026-10-22, architecture round 24, plan 07). Extends [ADR 0093](0093-a-fields-declared-type-decides-what-its-value-references.md) D1 (no field-name rule: the union answer is the parser's), [ADR 0151](0151-a-spec-file-is-changed-by-a-typed-edit.md) (Consequences: a union takes retitle and delete; every member edit refuses it) and [ADR 0019](0019-one-entity-snapshot-per-compile.md) (the union exemption reads the parse's answer).

## Context

- Whether a block is a union (`kind name = a | b`) was decided twice, by two rules that disagreed, and not at all by most
  typed verbs. The registry decided from the structure (the parser's variant list under its own key); the typed add-verify
  decided from a field's name (`block.entry("variants")`), the rule ADR 0093 D1 abolished. A braced
  `type Feature { variants string[] }` owed obligations (W004) and was refused as "a union" by the verb that W004's stub
  plans: MCP `suggest_fixes` dropped the stub (ADR 0161 D1), a resolving LSP client was listed it and refused with `-32602`.
- On a real union, `set`, `unset`, `link`, `unlink` and `remove_verify` never asked, and answered an E080 syntax error,
  `block_unreadable`, "takes any text; use set", "nothing to write", or rewrote the variants under the parser's private key.
- The same defect had been fixed module by module (`c8e03fa0d`, `57cb4e8cc`, `097601479`).

## Decisions

- **D1. The parse answers the body shape**: `specforge_parser::FieldMap::is_union()`, true when the map holds the parser's
  variant list (`parse_union_block` is its only producer). Never a field's name: a braced block's member named `variants`
  is a field, and so is a type union written as a member's value.
- **D2. The standing's union exemption reads it** (through the graph node's `FieldMap`); the registry no longer rebuilds the
  answer from the parser's key.
- **D3. Every member edit of a union is refused `not_editable`**: set, unset, link, unlink, add-verify and remove-verify,
  the no-ops included, before the verb reads its field argument (`Planner::on_members`, `block::refuse_union`), read from
  the text being planned. Retitle and delete take a union.
- **D4. `set <union> variants <text>` is refused with the rest**: no field-name exception anywhere.
- **D5. The fix module keeps ADR 0161 D1's drop**, and a verify stub the standing wants plans (a test holds it).

## Rejected

- A `body: BodyShape` field on the parser's `Entity` and the graph's `Node`: dozens of struct literals for an answer the
  `FieldMap` already carries; a bool needs no enum.
- Moving the variants out of the `FieldMap`: it would change the field text, inspect, export and hover output.
- The guard inside `Block::open` or at the top of each verb: path refusals come before any block is opened, so it could not
  be the one refusal; four calls to rebase over is worse than one site.
- Surfacing refused stubs in `suggest_fixes`: one broken file would turn every call into an error.
- Keeping `set <union> variants` as the one member edit a union takes: a field-name rule on purpose, with a raw text value.

## Consequences

- Breaking (CLI, MCP `specforge.edit`): the member edits above on a union exit 1 / `isError` with `not_editable`, including
  the former "nothing to write" and `set <union> variants`.
- W004's verify stub for a braced block with a member named `variants` is offered, and plans, on MCP and LSP.
- `FieldMap::is_union` is new and additive. No re-vendor, no schema change, no new diagnostic code.

## What would reopen it

A typed verb for a union's variants, or a second producer of `VariantList`.

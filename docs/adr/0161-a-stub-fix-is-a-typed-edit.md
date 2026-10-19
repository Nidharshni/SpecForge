# A stub fix is a typed edit, offered as the editor's position edits

**Status:** accepted (2026-10-19, architecture round 16, plan 04). Amends [ADR 0016](0016-navigation-is-one-module.md)
(the Fixes paragraph and D1's interface list), extends [ADR 0151](0151-a-spec-file-is-changed-by-a-typed-edit.md), amends
[ADR 0086](0086-ops-is-four-module-groups-over-a-base.md) (the fixes are in `authoring`) and, for D5,
[ADR 0023](0023-the-lsp-reads-a-document-through-one-module.md). Amended by round 17 plan 07 (2026-10-19): D5's list request chooses nothing over a buffer typed since the compile.

## Context

The LSP's code actions and MCP's `specforge.suggest_fixes` offered two fixes that add text, an entity stub and a verify
stub, both spelled by hand in `read::navigate::fixes`: the entity stub a format string appended at the end of the file with
a 2-space comment, the verify stub a format string with a 2-space indent placed by a brace scan. ADR 0151 said "until now
only `rename` wrote entity text" and missed them. They skipped everything the typed edits already do, so an applied fix
could leave the file non-canonical under `indent_width = 4`, put a block in the wrong place, and turn one error into two or
three ("Create invariant stub for a" turned an E003 into E014 + E006; a port stub added E006 for its required `direction`).
Each rule the typed edits learned since (reserved words, the file's format configuration) was missing from the stubs.

## Decision

- **D1.** A fix that adds text (the entity stub, the verify stub) chooses a typed edit (`Create`, `AddVerify`) and is
  planned by `edit::plan`: placement, layout, ID validation and E080 are the typed edit's. A fix whose plan is refused is
  not offered. The replacement fix stays a token edit.
- **D2.** A create stub writes `TODO` in each field its kind requires. A kind whose required field cannot hold it (an enum,
  a reference, a block) is refused by E080 and offers none: 15 of the 20 kinds a reference field targets keep a stub. The
  fix never switches on a field type; the plan decides.
- **D3.** A plan's position edits are the formatter's minimal diff of the text it was planned on and the text it will hold
  (`EditPlan::edits`, ADR 0021 D8): the editor keeps its cursor and marks, and MCP does not send the whole file.
- **D4.** The fixes live in `specforge_ops::authoring::fix` (ADR 0086: `read` may not use `authoring`, and widening the row
  makes a cycle), beside rename. A project with no root, or one whose configuration blocks rewrites, offers no stub
  (a typed edit is refused there); replacements are still offered.
- **D5.** The LSP makes a stub's edits on `codeAction/resolve` for a client that declares `resolveSupport` for `edit`
  (`fix::offers`, `Offer::fix`): the list request only chooses. MCP, and a client that does not resolve, get the edits at
  once.

  > **Amended by round 17 plan 07 (2026-10-19):** the list request first asks whether its document is the text the
  > project was compiled from, as `codeAction/resolve` does; when it is not, it answers no action, typed or not, and
  > plans none (ADR 0023 D3). Before, a resolving client was listed a stub whose resolve was then refused, and a
  > client that does not resolve paid one compile per stub only to drop it.

Rejected: a draft plan without E080 (a stub could again add an error; ADR 0151 rejects validation by hand); splitting the
choice (`read`) from the plan (`authoring`) across groups (a description type that restates `Edit` for one caller); a
whole-file replacement edit; keeping the hand stubs for detached projects (a third writer of entity text for an edge case).

## Consequences

Planning an edit costs one compile of the edited project: about 1 ms on a 2-entity project, 1.4 to 1.8 s (debug) on this
repository's own spec (2,214 entities), against 3 ms for the whole fix list before. Without D5 that is one compile per
stub a cursor move touches, under the state's read lock; with it the request stays at its old cost and the compile is
made when the user picks the fix. Release timing is not measured. MCP `suggest_fixes` with no filter plans every stub.

The LSP capability `codeActionProvider` becomes `{"resolveProvider": true}`. A detached project offers no stub. The
comment `// TODO: fill in fields` is gone; a kind with no required field gets an empty block. A stub is deduplicated per
target when chosen, so a target unresolved in two files offers one stub.

What would reopen it: an incremental E080 (ADR 0151's own reopen condition), which would let the LSP plan eagerly again.

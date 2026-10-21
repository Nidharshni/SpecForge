# A typed edit and a rename write one planned change

**Status:** accepted (2026-10-21, architecture round 19, plan 07). Amends [ADR 0164](0164-a-text-edit-is-one-shape.md)
(D2), [ADR 0116](0116-navigation-reads-the-text-the-project-was-compiled-from.md) (D3),
[ADR 0151](0151-a-spec-file-is-changed-by-a-typed-edit.md) (the round-15 amendment) and
[ADR 0124](0124-what-the-caller-lends-an-operation-is-one-value.md) (D2).

## Context

- A typed edit's plan (`EditPlan`) and a rename's plan (`RenamePlan`) carried the same facts in two shapes: the spec
  root, each file's text as the plan read it, and what the file becomes. Each had an `apply` that opened a
  `ProjectChange`, resolved every file through the caller's containment and staged it. The two had converged by hand
  three times (the spec root, the text-edits answer, the failure wording).
- A rename kept only the planned-on texts and its token edits, so it computed the written text at `apply` time, and
  two refusals lived there ("no text was planned", "its edits do not lie in the text planned"). Neither is reachable
  from a surface: MCP re-plans every call, the LSP never applies, and the edits are positions in the very compiled text
  the plan keeps (ADR 0116 D1). Only a Rust caller mutating the `pub` edits could reach them.

## Decision

- **D1.** `authoring::planned::PlannedChange` holds, per file relative to the spec root, the planned-on text (`None`:
  new) and the after text, and the operation's `doing`; no unchanged file is in it. It answers the affected files, the
  after texts, the text edits (`format::between`, ADR 0021 D8) and the unified diff; `apply(caller)` writes it as one
  project change (ADR 0128) through the caller's containment (ADR 0104), worded by the change (ADR 0155).
- **D2.** A typed edit's plan is its report and a planned change; a rename's plan is its token edits (read-only) and a
  planned change made with `apply_edits` when it plans. `edit::apply` and `rename::apply` are deleted.
- **D3.** ADR 0164 D2's refusal moves into `rename::plan`: an edit that does not lie in its compiled text refuses the
  plan (`file_unreadable`, Internal). It guards a navigator invariant (ADR 0116 D1); no surface reaches it.
- **D4.** A rename's edits stay the tokens (`mcp-operations.spec`), never a diff of its change.

## Rejected

The module in `ops::change` (base may use nothing); a rename built as a typed edit (ADR 0151); rename's edits derived
from the change; delegating accessors on `EditPlan`; a `debug_assert!` for the plan-time check; a rename `diff` in
`McpRenameResult`.

## Reopen when

A third planner of source texts appears (migrate stays on `ProjectChange`, since its backups are owned names, ADR
0171), or a plan must be carried from a preview to an apply (ADR 0116's rejected re-plan).

# A typed edit and a rename write one planned change

**Status:** accepted (2026-10-21, architecture round 19, plan 07). Amends [ADR 0164](0164-a-text-edit-is-one-shape.md)
(D2), [ADR 0116](0116-navigation-reads-the-text-the-project-was-compiled-from.md) (D3),
[ADR 0151](0151-a-spec-file-is-changed-by-a-typed-edit.md) (the round-15 amendment) and
[ADR 0124](0124-what-the-caller-lends-an-operation-is-one-value.md) (D2). Amended by round 20 plan 09 (2026-10-21):
D1, a missing file reads as empty, and the change answers the entities its blocks change.

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

> **Amended by round 20 plan 09 (2026-10-21):** D1's "no unchanged file is in it" reads a new file's planned-on
> text as empty, the way the diff and the text edits already did, so a list that creates a file and then empties
> it writes nothing (it wrote a 0-byte file and its directory). The change also answers the **entities** its blocks
> create, change or delete: every ID whose blocks (file and whole lines, as the parser reads each side) differ; a
> typed edit's report takes its entities from it, as it takes its files and its diff, and the planner's per-step
> record is deleted. A list whose later edits put things back names nothing. Rejected: a module for "the project
> as the earlier edits left it" (the planner's overlay is private to the planner, has one reader and no fix in its
> history; round 19 reached the same verdict); comparing parsed entities instead of their lines (a re-layout the
> diff shows would go unnamed). Reopen if the overlay needs a fix, or a second planner needs it.

# An MCP call previews by its arguments, whatever refuses it

**Status:** accepted (2026-10-22, architecture round 25, plan 09). Extends [ADR 0022](0022-a-mutation-reports-what-it-wrote.md) M2 and [ADR 0162](0162-a-mutations-preview-is-a-fact-of-its-outcome.md) P3 to the refusals before the handler. Amends [ADR 0015](0015-read-views-are-operations-over-the-project-view.md) D5 (the trace gap row).

## Context

- ADR 0022 M2 and `spec/events/mcp.spec` say a call whose arguments ask for a preview "is no mutation and emits nothing
  ... whatever its operation answered". That held when the operation refused the call, not when the call was refused
  before its handler ran: the target resolves before the arguments are read, and `mutation::refused` answered every such
  refusal as a failed run (`data.files_written: []`, `mcp_mutation_completed` with `success: false`).
- Reproduced on all six previewing tools (seven argument shapes: format's `check` and `diff`): with no project served,
  or a `path` naming none, `{"dry_run": true}` listed `files_written: []`; refused by the operation (E027) it listed nothing.
  In a real server only the reply differs (events go to `Discard`); in process, the event too.
- The trace prompt's `coverage_gaps` was written by `trace::Gap`, an enum whose four accessors existed only for MCP's
  `Gap::of`, which copied them into a struct that spells `McpTraceGap` again, in the trace tool's module. The prompt's
  composition of `plan::check`, `trace::chain` and `trace::affected` is ADR 0015 D4's; the presenter was accidental.

## Decisions

- **D1.** A call refused before its handler runs previews when its arguments read and ask for one: the refusal is sent as
  it is, with no `files_written`, and nothing is recorded. `Surface::refused` receives the invocation and
  `ToolSpec::refused` reads the arguments as `ToolSpec::run` does (`args::read`, after the undeclared-name check).
- **D2.** Arguments that do not read (an undeclared name, a value not its type) still cannot say they asked for a preview:
  a failed mutation (ADR 0022's standing consequence).
- **D3.** `specforge_ops::read::trace::Gap` is the `McpTraceGap` row, built `From<&MissingLink>` and `From<&PlanGap>`; MCP
  renders it and defines none.

## Rejected

- Documenting the exception in ADR 0022: the rule a client learns would gain a stage ("which refusal came first").
- Reading the arguments before the target resolves: inverts ADR 0025's order, so a call with bad arguments and no project
  would answer `invalid_input` instead of `precondition_failed`.
- An ops `GapReport` read view for the one caller: ADR 0015 D4 already chose the composition.
- Reading raw `dry_run`: format's preview is `check` or `diff` without `write`, and a raw read would disagree with the typed one.

## Consequences

- MCP refusals of a preview for no project, or a `path` naming none, carry no `data.files_written`, and
  `mcp_mutation_completed` is not recorded for them. Every other refusal and reply is byte-identical.
- Rust API: `ToolSpec::refused` takes the arguments and `Handled::refused` a `preview` flag (`specforge-mcp`);
  `read::trace::Gap` is a struct without accessors (`specforge-ops`).

## What would reopen it

A refusal stage that cannot read the arguments at all, or a client that needs a refused preview to list `[]`.

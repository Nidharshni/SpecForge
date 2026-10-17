# What the caller lends an operation is one value

**Status:** accepted (2026-10-16, architecture round 13, plan 02). Amends [ADR 0104](0104-containment-is-applied-where-the-operation-opens-the-file.md)
D3 (the spelling of the field), [ADR 0110](0110-who-can-be-asked-is-one-value-handed-to-an-operation.md) D2's request
sentence and D5's builders, and [ADR 0112](0112-every-project-file-an-operation-opens-goes-through-the-requests-containment.md)
D2 (collect's request and `SessionRequest`) and D4 (`ProjectRef::caller`). Their other decisions stand, and so does
ADR 0112's Rejected line on `AnalyzeOptions.containment`. Amended by round 14, plan 12 (D2: `rename::apply`).

## Context

Two facts belong to the surface and the call, not to any one request: what the call may reach on disk (`Containment`:
the CLI reaches everything, ADR 0092 D6; MCP reaches the root of the call's project, ADR 0112 D4) and who can be asked
(`Asking`: the CLI's terminal when someone can answer, else nobody; MCP nobody, ADR 0110 D5). Yet eight ops requests
each declared `pub containment:`, three of them also `pub asking:`, rename took the containment as a parameter, and
analyze carried it inside `ReportSource::Named`.

Threading them took nine commits in rounds 11 and 12, each touching ops, cli and mcp in lockstep: `c8671d2e`,
`3da57fb3`, `00ea1740`, `ceff78dc`, `19075b2a`, `5000fefc`, `aa55f6b6`, `20c6da88`, `1776aba1` (and `f6131414` for the
inference session). The ops tests spelled `Containment::` 100 times in 28 files and `Asking::` 39 times in 18 (29 of
them `asking: Asking::Nobody,` in a request literal), with five private copies of `static UNBOUNDED`. No site lent a
wrong value: the cost was locality and drift risk.

## Decision

- **D1. One value in the ops base.** `specforge_ops::Caller<'a>`: a `&Containment` and an `Asking`, `Copy`, no
  `Default`, private fields, `containment()` and `asking()`. Named after "the caller" CONTEXT.md already uses; not
  *host* (the process that loads extensions), not *reach* (ADR 0104 D1).
- **D2. Every request that opens a project file or can ask carries it,** required, as `caller`: `AddRequest`,
  `UpdateRequest`, `RemoveRequest`, `collect::Request`, `format::Request`, `migrate::Request`, `init::Request` and
  `SessionRequest`; `rename::apply` takes it as its third parameter. The operation reads `containment()` where it opens
  a file (ADR 0104 D2) and `asking()` where it asks (ADR 0110). `TrustPolicy.asking` (ADR 0110 D2) stands.
- **D3. What it does not carry.** The runtime, the registry, the solver and collect's machine are ports, passed beside
  the request. Analyze's `ReportSource::Named` keeps its own containment, filled from `caller.containment()` (ADR 0112's
  Rejected line stands, for `AnalyzeOptions.caller` too: `Default`, `Clone`, `PartialEq`, one variant reads it).
- **D4. Who builds it, once.** The CLI in `caller::of(yes, format, terminal)`: every path, and `asking::of`'s asker.
  MCP in `target::call::lent`, behind `ProjectRef::caller` (the call's project root) and `NewProjectDir::caller` (init's
  request bound): nobody to ask. A test on each surface holds every other module to it
  (`only_the_caller_names_a_reach_or_an_asker`, `every_caller_a_tool_uses_comes_from_the_call`).
- **D5. Tests lend `testing::caller()` or `testing::bounded(&c)`,** both asking nobody: no ops test lends another asker.

## Rejected

- A parameter of each operation beside the request (`op(view, request, caller, ..)`): it changes nine signatures and
  every call, and would pull analyze (plans 10 and 11) into this plan.
- An owned containment in the value: a `PathBuf` clone per MCP request, and `SessionRequest` loses `Copy`.
- `Caller::cli()` and `Caller::mcp()` in ops: ops must not know its surfaces.
- The runtime inside the value: a second source of truth beside `view.runtime()`, and every test caller would need one
  for operations that load no extension.
- A clap `CallerArg` flattened into each command: `--yes` has a different help line per command and five commands have
  none.

## Consequences

The next fact a surface lends (a write policy, a time budget) is one field of `Caller` and two builders, not N requests
by two surfaces by about a hundred test literals. `Caller` is shallow (two getters): its leverage is locality, not hidden
behaviour. An MCP elicitation asker (ADR 0110's "what would reopen it") would change `lent` alone.

## What would reopen it

A request that must lend a different reach or asker for part of one call (for example a narrower bound for one file); an
operation whose runtime is neither its view's nor a port its caller passes.

## Amendment (architecture round 14, plan 12)

`rename::apply(plan, caller)`: the plan keeps the spec root it was planned from, so the caller is its only other parameter.

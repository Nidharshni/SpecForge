# An operation refuses where it fails

**Status:** accepted (2026-10-16, architecture round 13, plan 10). Supersedes ADR 0029 D2 (with its round-9
amendment) and the last sentence of ADR 0072 D1; amends ADR 0015 D11 and ADR 0018 ("The operation").

ADR 0029 D2 kept `CheckError`, `TraceError`, `PlanError` and `AnalyzeError` "where a surface names the argument a
variant is about". Round 9 moved that job to the refusal's subject (ADR 0072) and removed `PlanError`; the other
three stayed and grew a second `From`, into `Refusal<XField>`. At `34844bd9` no surface and no other crate named a
variant: their only readers were their own two `From` impls (CLI: `OpError::from`, MCP: `Refusal::from`) and their
tests. Each refusal's text, code, kind, suggestion and subject were decided in four matches over the same variants,
10 to 200 lines from the condition, while every other operation (19 files) wrote them at the raise site.

## Decision

- **D1. No operation keeps an error enum of its own to convert into a refusal.** An operation builds its `OpError`
  where it fails and states the subject there (`OpError::new(…).about(Field)`, ADR 0072 D1). `parse_severity` and
  `parse_lint_profiles` return `Refusal<CheckField>`; `check` returns `OpError` (its one refusal, `no_project`, is of
  the project); `analyze` returns `Refusal<AnalyzeField>`; `trace` returns `Refusal<EntityField>` with
  `view::not_found`, as `inspect` does; `reach` returns `OpError`.
- **D2. A did-you-mean stays beside its name list**: check's folds case (`ERRORS` suggests `error`); a pass name and
  an entity id are matched as given.
- **D3. The wire did not change.** Every refusal's kind, code, message, suggestion, entity and subject is pinned in
  ops (`check_and_trace_refusals_are_pinned_field_by_field`, `analyze_refusals_are_pinned_field_by_field`) and on
  each surface (the `refusal_wire` snapshots in `specforge-mcp` and `specforge-cli`), before and after.
- Kept from ADR 0029 D2: ops decides the kind and code, and the spelling of an argument is a surface's
  (ADR 0072 D3).

## Rejected

- **Keeping the enums private, or behind one `From`**: still four matches per variant, and nothing reads a variant.
- **Severity and lint profiles as option tables (ADR 0027)**: it changes the codes `unknown_severity` and
  `unknown_lint_profile` and the message, and a table matches case-sensitively.

## What would reopen it

A caller that must act on *why* one of these operations refused beyond its code and subject (retrying differently
per cause, or an operation running another and branching on its failure): then the reason is data the outcome or
`OpError::data` carries, not a returned enum.

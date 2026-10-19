# A failed run is an outcome's fact

**Status:** accepted (2026-10-19, architecture round 17, plan 06). Amends [ADR 0029](0029-operations-decide-their-failure-and-verdict.md)
(D3: "failed runs" is defined here).

## Context

ADR 0029 D3 says MCP "keeps `isError` for refusals and failed runs" and never says what a failed run is. The two MCP
tools whose operations keep going after a problem each decided it with their own predicate: `specforge.format` failed
the call when a file could not be read or written (`!outcome.succeeded()`), `specforge.migrate` whenever its verdict
failed (`outcome.failure()`, `Some` exactly when `!ok()`). A problem in `specforge.json` the run went around (E069) was
therefore a successful call from format, edit, rename, add and remove, and a failed call from migrate, coded
`config_invalid`/`schema_mismatch`, the refusal that reads and writes nothing, even after the migration rewrote three
files and even for a dry run. Each tool also assembled the failed call's `data` and writes by hand.

## Decisions

**F1. A failed run is a run that did not do all it was asked**: a target it could not read, write or migrate, or a
change it undid (a rollback). What a run finds (a problem in `specforge.json` it went around, E069; a region left
unformatted, W142; a change under `--check`) fails its verdict and is no failed run. This is ADR 0029 D3's "failed
runs" and ADR 0004 D4-a's "a run that finds something is a successful call", applied to the two operations that keep
going after a problem. A failed run always fails the verdict.

**F2. The outcome answers it.** `format::Outcome::failed(diffs)` and `migrate::Outcome::failed()` return it as an
`OpError` whose kind is the run's, whose `data` is the run's document and whose `writes` are what the run left changed
(ADR 0022 D4); it carries no suggestion. A surface does not decide it: MCP returns it as the failed call through
`From<OpError> for Failed`; the CLI exits by the verdict (ADR 0029 D4), which a failed run fails.

**F3. Migrate's E069 is a verdict.** `Failure::ConfigNotUsedAsWritten` is gone; `config_invalid` again names only the
refusal decided by `ConfigProblem::blocks_rewrites` (ADR 0015 M11, M16).

**F4. The other writing operations need nothing.** Edit, rename, add, remove, init and the inference steps fail through
their operation's `Err`; none has a verdict.

## Rejected

- An E069 is a failed run too: it makes format over an E069 `isError` after writing files, contradicts ADR 0015 M16
  and M11, and leaves rename, edit, add and remove succeeding over the same E069.
- A failed run is a failed verdict: every finding would fail the call (`format --check` over a change, `analyze` below
  its gate), against ADR 0004 D4-a.
- A trait over writing outcomes: two implementations, one taking an argument; the six other writers fail through `Err`.
- A failed run as `run`'s `Err`: the CLI prints the whole run and exits by the verdict, so it needs the typed outcome.

## Consequences

MCP `specforge.migrate` over an E069 alone is a successful call whose reply is the migration document with `ok: false`
(`mcp_mutation_completed.success` is true for it). Every other failure, and `specforge.format`, is unchanged. The CLI
is unchanged.

## What would reopen it

A writing operation whose run can fail in a way its verdict does not show; a client that needs a finding to fail the
call (it reads `ok`).

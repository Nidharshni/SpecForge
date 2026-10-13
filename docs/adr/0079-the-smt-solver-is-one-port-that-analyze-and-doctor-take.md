# The SMT solver is one port that analyze and doctor take

**Status:** accepted (2026-10-13, architecture round 9, plan 09). Leaves ADR 0004 D3-f and ADR 0009 A's
"Next step" as they are.

`specforge analyze --prove` asks z3 whether the declared bounds are consistent and entail the declared claims;
`specforge doctor` says whether z3 is installed. The solver sat behind three seams: a private `Solver` trait in
the prove pass, a `ProveFn` that analyze took only so its tests could reach that trait, and a bool that doctor
took instead of a solver. Two copies of the probe worded a missing z3 two ways (W098, `z3_missing`). The probe had
no timeout, the solve's timeout did not hold when the solver left a child process, the timeout warning advised
raising a timeout no flag set, a z3 that failed was called "not found on PATH", and sixteen tests passed or failed
with the developer's PATH.

## D1. One port

`specforge_ops::solver::Solver` (spec port `SmtSolver`): `availability()` is `Ready { version }` or
`Unavailable(W098)`; `solve(script)` is the solver's stdout or a `SolveFailure` (timed out, failed). Its adapters
are `Z3` (production) and `testing::ScriptedSolver` (tests). The prove pass and doctor are its callers.

## D2. The caller passes it

The solver is a fact about the machine, not the project: neither the environment nor the project view carries
it, and the compile never asks it. `analyze` takes `AnalyzeOptions.prove: Option<&dyn Solver>` (naming a solver
asks for the prove step); `diagnose(view, solver)` takes it as an argument. The CLI builds `Z3` per command; the
MCP server holds one (`McpServer::with_solver` for tests) and hands it to tools through `ProjectRef`.

## D3. The adapter words "z3 does not run", once

`Z3` builds the W098 for a z3 that is not on PATH or does not run, saying which and why. The prove pass reports
it; doctor's toolchain finding is it (code W098; check, its message; remediation, its suggestion). The
`z3_missing` code is gone. `z3_available` in doctor's document is derived from the findings.

## D4. The adapter owns its timeouts

A solve ends at the adapter's timeout (`--prove-timeout`, 30 s by default); the probe at 5 s. The deadline holds
for the process and for its output: a child process left holding stdout does not extend it. Availability is
probed once per operation and not cached.

## D5. A test chooses the solver it sees

In process, a scripted solver; for a `specforge` child process, a PATH the test builds. Tests of what z3 answers
use the real z3 and fail with how to install it when it is missing.

## Rejected

- **The solver in the environment or the view** (like the extension runtime, ADR 0015): the compile never asks
  it, and it is the machine's, not the project's.
- **A solver crate**: one consumer crate.
- **The prover in `@specforge/formal`** (ADR 0009 A, "Next step"): its preconditions (a second solver, a
  non-arithmetic logic, a domain encoding) still do not hold. The port is what that move would replace.
- **Caching availability**: a server would not see a z3 installed while it runs.

## Consequences

- ADR 0004 D3-f's preconditions for an MCP `prove` argument (shared prove code, a z3 timeout) now hold. Adding
  it is a spec change with its own latency question (a solve per claim inside one call) and is not decided here.
- Doctor's JSON and MCP replies say `W098` where they said `z3_missing`; the human report prints the toolchain
  finding in a "Toolchain:" section.
- The prove summary's `solver` is `null` when no solver runs.

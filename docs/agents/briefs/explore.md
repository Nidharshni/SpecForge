# Explorer brief

Context: `docs/agents/briefs/_shared.md` rules, `CONTEXT.md`, the ADRs for your area. You explore ONE area of the
codebase at `<base SHA>` and report architecture friction, with the vocabulary of the `codebase-design` skill
(module, interface, depth, seam, adapter, leverage, locality).

For each candidate, in your final reply:
- the files and `path:line` evidence, quoted at the base SHA;
- the deletion test: would deleting the module concentrate complexity, or only move it;
- a repro you ran for every live bug (the command and its output);
- whether an ADR already decided it.

Verify each claim by running or reading the code. Drop what does not hold. Done when every candidate carries
evidence you checked.

# Plan-writer brief

Context: `docs/agents/briefs/_shared.md` rules, the round's `README.md` and `FINDINGS.md`, `CONTEXT.md`, the ADRs
for your candidate. You write ONE plan, `NN-<slug>.md`, in the planning directory, and change nothing else in the
repository.

The plan has: Summary, Evidence (`path:line` at the base SHA, with a "Corrections to the review" part for claims you
dropped), Target design, Decisions (options and the recommendation), Tickets, Verification, Risks, Out of scope.

- Every ticket is one commit with a conventional-commit title, the test it adds or flips, and the command that
  proves it.
- Every live defect gets a failing test first (a pin ticket) and a ticket that flips it.
- A plan may conclude its candidate is not worth doing; then its tickets are only what is.
- Name the diagnostic codes and ADR numbers the README gives you, and no others.

Done when a reader who has not seen the review can implement each ticket from the plan alone.

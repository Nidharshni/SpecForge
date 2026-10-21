# Plan-writer brief

Context: `docs/agents/briefs/_shared.md` rules, the round's `README.md` and `FINDINGS.md`, `CONTEXT.md`, the ADRs
for your candidate. You write ONE plan, `NN-<slug>.md`, in the planning directory, and change nothing else in the
repository.

The plan has: Summary, Evidence (`path:line` at the base SHA, with a "Corrections to the review" part for claims you
dropped), Target design, Decisions (options and the recommendation), Tickets, Verification, Risks, Out of scope.

- Every ticket is one commit with a conventional-commit title, the test it adds or flips, and the command that
  proves it. Run each command's selection at the base (`cargo nextest list` with the same `-p` and `-E`) and keep only
  commands that list at least one test; name test binaries as `cargo nextest list` prints them.
- Every live defect gets a failing test first (a pin ticket) and a ticket that flips it. The ticket that adds a
  linked test also adds its `verify` text to the spec: the pre-commit hook's `check-links` refuses a test whose text
  is not there yet.
- The plan ends with the tracking block of `docs/agents/round-tracking.md`.
- A plan may conclude its candidate is not worth doing; then its tickets are only what is.
- Name the diagnostic codes and ADR numbers the README gives you, and no others.

Done when a reader who has not seen the review can implement each ticket from the plan alone.

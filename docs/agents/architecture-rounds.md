# Architecture rounds

One round is: explore the code with `/improve-codebase-architecture`, write one plan per candidate, implement the plans
in parallel worktrees, merge them, re-vendor, gate, open the PR. The coordinator is the session that runs the steps;
the agents it launches follow a brief in `docs/agents/briefs/`. Plans live in `.planning/architecture-<date>/`
(gitignored); the coordinator copies the folder to `../SpecForge-planning/` (a plain local folder, not version-controlled)
when the plans are written and again when the round merges.

Every step has a done condition. Do not start the next step before it holds.

## 0. Base

`scripts/unlanded-prs.sh` exits 0, then branch the round from `origin/main` and run `cargo gate full --only audit` on
it: `audit` reads the live advisory database, so `main` can turn red without a commit. Done when the base SHA is in
the round's `README.md` and the audit is green, or its failure is a named open item there.

## 1. Explore

Launch one explorer per area with `briefs/explore.md`. Explorers return their findings as their final reply; the
coordinator saves them to `FINDINGS.md`. Done when every candidate has `path:line` evidence at the base SHA, and its
card in the review carries a Mermaid before/after (beside any hand-drawn one): the PR report of step 7 reuses it.

## 2. Plan

One agent per candidate, `briefs/plan.md`. Each writes its plan into the planning directory (that write works) and
returns a summary. The coordinator writes `README.md`: order, shared files, reconciliations, open questions. Done when
every plan has numbered tickets with test commands and the README orders them in waves. Each plan ends with the
tracking block of [`round-tracking.md`](round-tracking.md); the coordinator gathers the blocks into `tracking.json`.

## 3. Implement

One agent per plan or ticket range in its own worktree (`isolation: worktree`), `briefs/execute.md`, waves from the
README. Agents report inline. Done per agent when its branch holds the integration tip, passes clippy and nextest
for the crates it touched, and its final reply is in.

A dead agent (API error) resumes with `SendMessage`; its worktree survives.

Each worktree agent builds into its own target directory, about 30 GB for the workspace. Before launching a wave,
compare `df -h ..` with `du -sh ../SpecForge-targets/*`. When the disk holds fewer target directories than the wave
has agents, run the plans **serially** instead: one agent at a time, in the README's merge order, committing straight
onto the integration branch in the main checkout and sharing one target directory. Nothing is left to merge, so
step 4 is skipped.

## 4. Merge

For each finished branch, in wave order: `git merge --no-ff`, `cargo clippy --workspace --all-targets -- -D warnings`
and `cargo nextest run --workspace`, and `git status` shows no `*.snap.new`. Then `cargo gate full --only file-size`; when it reports a listed file gone
and a new one over the cap (the branch moved it), `cargo gate baseline --only file-size --since HEAD^1` and commit
`chore(gate): carry file-size rows after merging <plan>`. Then `git worktree remove <path>`
(`-f -f` only when the agent's final reply is in; a finished agent's worktree stays locked). Done when every plan is
on the integration branch.

## 5. Re-vendor

Once, after the last input changed: `cargo run -p xtask --bin revendor` (see `docs/agents/revendor.md`), review the
declaration diff, commit what it prints.

## 6. Gate

`cargo gate full` is green, then the PR. It already runs the build, clippy and nextest, so a serial round runs it
alone, not after a separate workspace clippy and nextest. A baseline that fell is lowered with `cargo gate baseline` in
the same commit (`--only specs` or `--only file-size` for one section). `--only` takes a comma-separated list.

## 7. PR

One PR for the round, one commit group per plan. Every commit body (`briefs/execute.md` rule 2) carries boxed ASCII
diagrams of the problem and the before/after. The PR body is the round's report: a Mermaid graph of the waves and
dependencies, a table of plans (card, ADR, commits, defects fixed), a Mermaid before/after per plan (taken from the
review's cards), the security fixes, the breaking changes, the decisions taken, the verification output and what
stays open. Done when CI is green; after the merge, delete the round's target directories.

## Pitfalls round 7 paid for

- A PR stacked on a branch that had already merged never reached `main`: step 0 exists for that.
- A hook-less repo found its unformatted specs, stale `verify` texts and `.snap.new` files at step 6:
  run `scripts/setup-hooks.sh` in the main checkout (worktrees share the config).
- Agents' scratch files collide in a shared scratchpad: each uses `<scratchpad>/<its label>/`.

# Implementer brief

Context: `docs/agents/briefs/_shared.md` rules, the round's `README.md` (order, reconciliations; they override a
plan's text) and your plan, in full.

1. Work only in your worktree and branch. `git log --oneline -3` shows the integration tip first.
2. One ticket, one commit, in the plan's order, with the plan's title. Breaking changes go in the commit body.
3. Pins first: a characterization test asserts today's behaviour; the ticket that fixes the bug flips it in the same
   commit. A test links its spec obligation with `#[specforge_test_macros::test(behavior = "…", verify = "…")]`, and
   the `verify` text exists verbatim in the spec (`cargo run -p xtask --bin check-links` checks it).
4. After each ticket: the plan's targeted `cargo nextest run -p <crate> -E 'test(<filter>)'`, its deletion checks,
   `cargo clippy -p <crate> --all-targets -- -D warnings`, `cargo fmt --all`. CI also lints on Linux: beware
   macOS-only `cfg` code.
5. Builtin blob rebuilds do not run in worktrees: a re-vendor ticket changes the source, stops before the rebuild,
   and says so.
6. Code that moved since the plan: adapt and record the deviation. A ticket that needs another plan's missing ticket:
   stop before it and say which.

Final reply: branch name, commits (hash and title), test and clippy results verbatim, deviations, the interfaces other
plans depend on, and what the coordinator must do next. Done when every ticket in your range is committed and the
checks of rule 4 pass on the merged tip.

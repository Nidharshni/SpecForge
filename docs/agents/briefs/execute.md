# Implementer brief

Context: `docs/agents/briefs/_shared.md` rules, the round's `README.md` (order, reconciliations; they override a
plan's text) and your plan, in full.

1. Work only in your worktree and branch (in a serial round, the integration branch in the main checkout).
   `git log --oneline -3` shows the integration tip first.
2. One ticket, one commit, in the plan's order, with the plan's title. The body is a short paragraph, then boxed
   ASCII diagrams (`┌─┐│└┘ ▶ ▼`, lines of at most 100 characters): PROBLEM (the modules or calls that were wrong),
   SOLUTION (one sentence), BEFORE / AFTER (the call sites before and after this ticket); a pin gets a small diagram of
   what it pins and what flips it. Then a `Breaking:` line when it breaks something, then the trailer. Write it to a
   scratch file and `git commit -F` it. Leave the round's `tracking.json` to the coordinator.
3. Pins first: a characterization test asserts today's behaviour; the ticket that fixes the bug flips it in the same
   commit. A test links its spec obligation with `#[specforge_test_macros::test(behavior = "…", verify = "…")]`, and
   the `verify` text exists verbatim in the spec (`cargo run -p xtask --bin check-links` checks it).
4. After each ticket: the plan's targeted `cargo nextest run -p <crate> -E 'test(<filter>)'` and the whole crate's
   `cargo nextest run -p <crate>` (source-scanning tests such as `ops::remedy_wording` read every file, not the
   ticket's), its deletion checks, `cargo clippy -p <crate> --all-targets -- -D warnings`, `cargo fmt --all`. Before
   the final reply, `cargo gate quick`: it runs the changed crates and their dependents, where a changed ranking or
   message breaks another surface's snapshot. CI also lints on Linux: beware
   macOS-only `cfg` code. A ticket that shrinks, splits or moves a file listed in `gate/baseline.json`'s `file-size`
   section runs `cargo gate baseline --only file-size` and commits the file with the ticket; no ticket adds lines to a
   listed file.
5. Builtin blob rebuilds do not run in worktrees: a re-vendor ticket changes the source, stops before the rebuild,
   and says so.
6. Code that moved since the plan: adapt and record the deviation. A ticket that needs another plan's missing ticket:
   stop before it and say which.

Final reply: branch name, commits (hash and title), test and clippy results verbatim, deviations, the interfaces other
plans depend on, and what the coordinator must do next. Done when every ticket in your range is committed and the
checks of rule 4 pass on the merged tip, `cargo gate quick` included.

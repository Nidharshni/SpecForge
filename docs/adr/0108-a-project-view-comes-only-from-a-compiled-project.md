# A project view comes only from a compiled project

**Status:** accepted (2026-10-16, architecture round 11, plan 05). Amends [ADR 0047](0047-the-compiled-project-is-the-sessions-core.md) (D5 and E5), [ADR 0078](0078-a-tests-project-is-one-fixture.md) (D1), [ADR 0019](0019-one-entity-snapshot-per-compile.md) (the memo), [ADR 0015](0015-read-views-are-operations-over-the-project-view.md) (hover while the session is out) and the wording of [ADR 0100](0100-a-session-takes-a-change-from-disk-only-as-changes.md) D4. D4 amended by [ADR 0121](0121-a-compiled-projects-root-answers-rooted-or-detached.md) (an assembled project is rooted at a directory, not a project root). D4 amended by [ADR 0123](0123-a-graph-is-read-only-with-the-registry-build-it-was-built-in.md) (an assembled source is built in its environment when compiled). D2 is extended by [ADR 0139](0139-a-compiled-project-reads-its-recorded-report-through-the-disk-it-is-handed.md): only a compiled project reads its recorded report, at its own root, through the disk it is handed.

`ProjectView::new(graph, env, root, recorded)` carried an invariant no type enforced: `recorded` had to be the memo of
whoever owned `graph`, and `root` the root `env` was loaded at. Its one production caller, the LSP's hand-kept
`StandIn` (a five-field copy of a compiled project), dropped the root, so hover said "coverage unavailable while the
project rebuilds" for the length of every rebuild (about 150 ms on a 3001-entity project, reproduced over stdio).
Four in-memory test fixtures (`view::testing::Fixture` with 61 uses, `view_support::Project` with 52, analyze's
`Project`, the LSP completion `Fixture`) and 26 hand-built `RecordedCoverage::over` pairs made views production cannot
make: a view rooted in one directory over an environment rooted in another, or in none. 31 call sites named `new`, and
the stand-in grew a field each time `CompiledProject` gained a reader (`a345731f`, `621dfbd0`, `c48474b0`, `ffe4e521`).

## Decision

- **D1.** `ProjectView::of(&CompiledProject)` is the only constructor. The view holds the compiled project, plus an
  optional listed report (`reporting`) and what the surface adds (`also_reporting`). `ProjectView::new` and `Reported`
  are gone; `ProjectView::project()` reaches the project.
- **D2.** Only a compiled project makes its memo: `RecordedCoverage::of` and `over` are crate-private.
- **D3.** A compiled project copies (`Clone`). The graph is copied; the environment, the texts, the report and the
  memo are shared, and the graph build shares its parses (`Arc<SpecFile>`).
  - While its session is out, the LSP holds such a copy, taken when the session goes out. That is ADR 0100 D4's
    stand-in, now a compiled project instead of five fields. The rejection of a shared read snapshot with
    copy-on-write stands, and so does its reopen condition (a second reader of a session during an update), which is
    still unmet: MCP answers one message at a time, watch owns its session on one loop.
  - Hover shows the copy's coverage: the copy is rooted and holds the session's memo. `CompiledProject::unread` is
    what readers see before the first open's sources are read.
- **D4.** A test's project in memory is `specforge_project::testing::Assembled`: a graph, an environment rooted where
  the project is, and the diagnostics the test lists. No check runs. It is the in-memory counterpart of `TestProject`
  (ADR 0078 D1 now reads "one fixture on disk, one in memory"). `Environment::with_registries` is gone, and the
  builder's bare build is rooted at its directory. `specforge_ops::testing::Viewed` keeps `fixture.view()` as the
  spelling in tests.

## Rejected

- An `Arc` snapshot with `Arc::make_mut` in the graph build (ADR 0023, ADR 0100 D4).
- An enum for a hand-built graph inside `CompiledProject`: every session method would gain an arm that cannot happen.
  `GraphBuild::holding` sits behind a `testing` feature instead.
- Running the checks over an assembled project: every test reading `reported()` would change; a test that needs real
  check diagnostics uses `TestProject`.
- Keeping `ProjectView::new` behind a `testing` feature: it keeps four free parameters that can disagree.
- Converting every hand-built graph to sources.

## What would reopen it

A second reader of a session during an update (then the copy becomes a shared snapshot in `ProjectSession`); a profile
in which the copy shows (then measure `GraphBuild`'s clone first).

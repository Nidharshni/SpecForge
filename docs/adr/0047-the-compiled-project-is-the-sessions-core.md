# The compiled project is the session's core

**Status:** accepted (2026-10-09). Amends [ADR 0032](0032-one-graph-build.md) ("One cold pipeline"); notes in
ADRs 0014, 0015 and 0019. D1: the core also memoizes its report (ADR 0074). D6 amended by [ADR 0109](0109-reopening-a-project-in-place-is-one-session-operation.md) (`McpState::source` is the one runtime rule; the served project is reopened in place).

`specforge-project` had two types with the same six fields, the environment, the sources, the import
diagnostics, the graph, the check diagnostics and the snapshot and coverage memo, held in different types
(an owned `Environment` and a `Graph` in `CompiledProject`; an `Arc<Environment>` and a `GraphBuild` in
`ProjectSession`). Each wrote the order that defines what `specforge check` reports, and the check pipeline
(snapshot, checks, memo) was written in both. Twenty commits in five weeks changed both files; the orders
drifted once (the session listed graph diagnostics by file while `check` listed them in build order, fixed
by 4f3f82fc). Every caller branched on which type it held: the project view's `Reported::{Compiled,
Session}`, and MCP's call target, whose other project was a `CompiledProject` compiled again after every
call that wrote files (a second environment load and cold build, read only by rename), while the served
project was brought up to date with `ensure_fresh`. `OtherProject` also re-implemented the runtime rule
`RuntimeSource` already states.

## Decisions

- **D1. `CompiledProject` is the core.** It holds the environment (shared), the source cache, the graph
  build, the imports' and the checks' diagnostics, the snapshot and coverage memo, and whether it is on disk.
  `CompiledProject::diagnostics` is the one report order. A one-shot compile (`CompiledProject::compile`)
  is one and keeps no stamp (ADR 0030); a `ProjectSession` holds one (`ProjectSession::project`) plus its
  runtime source, its inputs and its stamps (its runtime is its environment's: ADR 0015, "The runtime
  travels with the environment").
- **D2. The session exposes its compiled project; it does not mirror it.** Every reader goes through
  `ProjectSession::project()`; the session's fifteen pass-through readers are gone, the three test-only
  ones (`graph_diagnostics`, `file_diagnostics`, `diagnostic_files`) with them.
- **D3. The check pipeline is the core's, the stamps the session's.** The core takes a snapshot
  (`snapshot_now`) and runs the checks over it (`check_over`); the session renews and stamps its check
  inputs between the two (ADR 0030 D3), a one-shot compile does nothing between them.
- **D4. A detached project is a compiled project with no root** (`root()` is `None`, no import is
  resolved), so a view of it is rootless with no parameter (ADR 0030 D5).
- **D5. One view constructor for a compiled project.** `ProjectView::of(&CompiledProject)` serves a
  one-shot compile and a session's; `of_session` and `Reported::Session` are gone. `Reported` keeps
  `Compiled` and `Listed`: the LSP hover and MCP's context prompt report a listed slice.
- **D6. MCP opens another project as a session.** `CallTarget::Other` holds a `ProjectSession` opened by
  `McpState::open`, the one rule for the served project and another one (the host's runtime, else the
  project's own per load); after a call that wrote files it is brought up to date with `ensure_fresh`,
  exactly what changed, as the served project is (ADR 0014 D1, ADR 0022). `OtherProject` is gone.
- **D7. The equality is proven, not assumed.** After every update that runs the checks, a session reports
  exactly what a fresh compile of the same sources reports, in the same order: a property test over random
  sequences of every update kind holds it (`a_session_reports_what_a_fresh_compile_reports_in_order`). An
  editor update that skips the checks while a file does not parse is the one exception.
- **D8.** The verify flag lives in the graph build only (`GraphBuild::verifies`); an `Update` is assembled
  in one place (`Update::of`); `field_types` and `verdicts` are private modules.

## Rejected

- A private `ProjectCore` held by both types: `CompiledProject` would forward every call to it.
- One type with optional session state (`Option<Freshness>`): the hidden modes ADR 0030 removed.
- `Deref<Target = CompiledProject>` for the session: it hides which methods are whose.
- Deleting `Reported`: its `Listed` arm has production callers; two fields would encode the same two cases.
- Keeping MCP's other project a one-shot compile compiled again after a write: a second environment load
  and cold build per writing call, and a second runtime rule.
- Refreshing another project only when the mutation's reply reads diagnostics: it would make ADR 0022's
  "bring the target up to date after any write" depend on the target, for a saving the session already makes
  small.

## Consequences

- No reply, diagnostic or exit code changes; a write to another project over MCP costs a stat per input and
  an incremental update instead of a second compile.
- A one-shot compile keeps each file's parse for the life of the command (it was alive at the build's peak
  already).
- ADR 0032's "one cold pipeline" stands: `Environment::build_sources` is still the one cold build;
  its result is a `CompiledProject` (`CompiledProject::read`) for a compile and for a session's open.

**What would reopen it.** A surface that needs a compiled project's state without its graph build (a
snapshot published to readers, which ADR 0023 rejected), or a one-shot compile whose kept parses show in a
profile.

## An environment assembles from its loaded parts (amendment, architecture round 6, plan 06)

The environment is the first part of a compiled project, and it had no constructor that took its parts.
`Environment::from_read` read the lock, ran the extension load, then derived E069, the registry build, the custom
rules' probes (pushed into the build's own field from outside it), the providers' registration, I002 and the spec root.
Its twelve fields were public, so tests built an environment with `with_registries` or `empty()` and wrote fields: 51
writes in 11 files. Those fixtures built states no load makes (providers registered before the declarations existed,
passes declared by an extension the runtime did not serve, a runtime swapped under an environment), and every new
field swept them (the runtime field: 64 files).

- **E1. `Environment::assemble(read, installed, load)` derives everything.** From the config read, the installed
  extensions and the extension load it builds E069, the registry build, the probes (in the load's runtime, through the
  build's `probe`), the providers' registration, I002 and the spec root. It reads nothing from disk; its one
  effect is the probe's calls in the runtime it is given. The root is `installed.root()`.
- **E2. "No runtime" is a type.** `ExtensionLoad` is `NoRuntime` (each entry enables what it names, nothing loads) or
  `In { runtime, loaded }`; the environment holds that runtime (ADR 0015 R1, R3).
- **E3. `from_read` is the disk adapter.** It reads the lock (`Installed::at`), loads the configured entries into the
  runtime it is given (`Installed::load`), and assembles. `Environment::load` reads `specforge.json` first.
- **E4. The fields are private.** Readers use accessors (`root`, `config`, `config_problems`, `config_found`,
  `installed`, `enabled`, `spec_root`, `registries`, `providers`, `runtime`); what the environment reports is read
  only through `diagnostics()`. No mutator exists: a test that needs another environment assembles another one.
- **E5. Test environments.** A fixture assembles (`Environment::assemble`) or, when it serves an extension in
  process, takes the production load (ADR 0028 D3). `empty()` is the detached project's; `with_registries(registries,
  runtime)` is the bare build of a graph made in a test. `from_declarations` is gone. A specforge.json text parses with
  `parse_project_config`, the parse `read_project_config` runs after reading the file.

**Rejected.** Running the extension load inside `assemble` (it reads modules from disk); a `root` parameter beside
`installed` (two values that must agree); `Loaded` plus `Option<SharedRuntime>` (an environment without a runtime that
holds declarations); a `testing` feature for `with_registries` (no invariant to protect).

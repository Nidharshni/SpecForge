# A test's project is one fixture, and a behaviour is tested in the crate that owns it

**Status:** accepted (2026-10-14, architecture round 9, plan 08). Amends ADR 0029 (D9a), ADR 0040 (where its
diagnostics are proven) and ADR 0062 (D5a).

At `2d9df4aa` the CLI's 57 test files built their projects and their command lines each their own way: 26
functions returned a configured `specforge` command, 29 files built it inline, and `"--path"` was spelled 547 times
in 44 files, so one flag change (`ddf20d8c`) edited 32 test files. 36 of the 42 files that ran the binary ran it
under the developer's `HOME`: with an expired registry login, five `doctor` tests failed. Seven `e2e_*` files,
`field_types.rs` and `formal_diagnostics.rs` tested graph edges, queries, traces, schemas and the formal
extension's rules by spawning the binary and parsing stdout; `product_rules.rs` and `pipeline.rs` compiled in
process but lived in the CLI crate; seven files reached MCP replies by spawning `specforge mcp` (ADR 0062 D5
reserves that for the binary contract). Across the workspace, 26 helpers wrote a `specforge.json`, chose a runtime
and compiled it their own way.

## Decision

- **D1. One project fixture.** `specforge_project::testing::TestProject` (feature `testing`) is a project on disk
  (config, files, what an in-process runtime serves, installed as a project that enables it has it) with its own
  empty HOME beside it. It compiles as every CLI command does (`compile`) and opens as MCP and the LSP do
  (`session`). A directory a test builds apart from a fixture is compiled with `testing::compile_at`, and the
  production runtime comes from `testing::component_runtime`. No test writes a compile of its own with
  `CompiledProject::compile`, or builds the production runtime itself.
- **D2. One driver per surface.** In process: the fixture's `compile`/`session`. MCP:
  `specforge_mcp::testing::Served` (`rooted`: as `specforge mcp <root>` serves it; `initialized`: an `initialize`
  naming the project). CLI: `crates/surfaces/cli/tests/support::Cli`, the one place the suite spells the binary, a
  project (`--path <root>`), a document (`--format json`) and the environment: the project's HOME, the shared
  compile cache (`SPECFORGE_WASMTIME_CACHE`), no colour, token, consent or report variables, the root as working
  directory, stdin closed.
- **D3. A test lives where its behaviour does.** A CLI test asserts the binary contract (ADR 0029 D9): clap, the
  exit table, the streams, human rendering, the environment, process life, files a command writes; or compares
  surfaces (the CLI crate is the only one that depends on all of them). A test whose assertions are all facts of
  the compiled project is `specforge-project`'s; of an operation's document, `specforge-ops`'s; of an MCP reply,
  `specforge-mcp`'s. A command's own test file stays whole.
- **D4. A guard test holds it.** `crates/surfaces/cli/tests/isolation.rs` fails when a test outside the CLI's support
  module runs the binary or sets HOME, when any test calls `CompiledProject::compile` itself, or builds the
  production runtime outside the component crate's tests.

## Consequences

- A flag rename edits `support/cli.rs` and the tests of that flag; a graph or operation change is tested by
  `cargo test -p specforge-project` or `-p specforge-ops`, without the binary.
- The suite passes whatever the developer's HOME holds; no state persists between runs in a shared temp HOME.
- 106 tests moved to `specforge-ops` (graph edges, cross-extension references, multi-entity, verify statements,
  query, trace, schema, field types, the formal diagnostics), 5 to `specforge-mcp`, and the builtins' rules and the
  compile pipeline to `specforge-project`; the `published-schemas` gate check runs in `specforge-ops`.
- Not faster: a warm spawn is ~30 ms and the e2e modules finished in <= 0.1 s; the suite's time is the in-process
  LSP, cargo builds and watch debouncing (ADR 0029 D9's measurement holds).

## Rejected

- **A `specforge-test-support` crate** holding every driver: it depends on MCP, so `specforge-project`'s tests would
  build the MCP stack and form a dev-dependency cycle through it, and it still cannot name the binary
  (`CARGO_BIN_EXE_specforge` exists only in the CLI's tests).
- **Running the CLI in process** for its tests: ADR 0029 D9 (clap, the exit table, the streams, the environment).
- **Moving every command's tests to ops**: a command's file is its end-to-end test, flags to document to exit;
  each is a separate decision.
- **Fixed shared temp HOMEs** (`temp_dir()/specforge-parity-home`): state persists across tests and runs.
- **An isolated HOME without the shared compile cache**: each spawn recompiles the builtins (9-12 s each at
  `opt-level = 0`, 0.4 s at the test profile's `opt-level = 1`).

## What would reopen it

A surface the fixture cannot express (a project that is not a directory), or an in-process operation that must
read a HOME a test chooses (then HOME becomes a seam of that operation, not of the fixture).

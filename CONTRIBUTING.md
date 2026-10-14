# Contributing

Thanks for working on SpecForge. Read [ARCHITECTURE.md](ARCHITECTURE.md) first for how the pieces fit.

## Build from source

You need the stable Rust toolchain (pinned in `rust-toolchain.toml`).

```bash
git clone https://github.com/leaderiop/SpecForge && cd SpecForge
cargo install --path crates/specforge-cli     # installs `specforge` into ~/.cargo/bin
```

Building the CLI uses the committed builtin blobs, so it needs no wasm target. Re-vendoring the blobs after changing anything they compile from (an extension, the SDK, the protocol types) needs `rustup target add wasm32-wasip2`, then:

```bash
cargo run -p xtask --bin revendor
```

`cargo gate` fails when the vendored blobs under `extensions/<name>/wasm/` drift from their sources. See [docs/agents/revendor.md](docs/agents/revendor.md) for when a change needs this.

## Before you commit

Run `scripts/setup-hooks.sh` once per clone: the tracked pre-commit hook runs the hook's share of the check list (`cargo gate hook`) on the staged files in seconds.

Run `cargo gate` (what the working tree changed) before a PR and `cargo gate full` before a merge; CI runs `cargo gate ci`, the same list. `cargo gate list` prints every check, the run it joins and the paths that trigger it.

The prove tests ask the real SMT solver: install z3 (`brew install z3`, `apt-get install z3`); CI installs it. Every other test chooses the solver it sees.

## Tests

A test builds its project with `specforge_project::testing::TestProject` (feature `testing`) and exercises it
through one driver: `compile()`/`session()` in process, `specforge_mcp::testing::Served` over MCP, or
`crates/specforge-cli/tests/support::Cli` through the binary. Put a test in the crate that owns what it asserts;
only the binary's contract (arguments, exit codes, streams, environment) and cross-surface comparisons are CLI
tests ([ADR 0078](docs/adr/0078-a-tests-project-is-one-fixture.md)).

A test or tool that reads a file outside its own crate finds it from `specforge_repo_root::repo_root()`
(`crates/specforge-repo-root`), never by counting `..` from `CARGO_MANIFEST_DIR`: crates sit at different depths
under `crates/` ([ADR 0081](docs/adr/0081-the-repository-root-is-found-not-counted.md)).

## Where things are

The repository tracks what ships and what builds, tests or documents it, and nothing else
([ADR 0082](docs/adr/0082-the-repository-tracks-what-ships-and-what-builds-it.md)). `cargo gate`'s `root-layout` check keeps the top level to this list
(`gate/src/root_layout.rs`); planning rounds, strategy, archived plans and experiments live in the private planning
repository, `leaderiop/SpecForge-planning`.

What ships:

- [`crates/`](crates/): the Rust workspace: compiler, graph, extension runtime, CLI, LSP, MCP.
- [`extensions/`](extensions/): the builtin extensions and their vendored blobs, embedded in the binary.
- [`integrations/`](integrations/): the Rust test-tracing crates and the VS Code extension.
- [`schema/`](schema/): the JSON Schemas `specforge.json` and the test reports name.
- [`install.sh`](install.sh): the installer the README's `curl` line fetches from `main`.
- `README.md`, `LICENSE-APACHE`, `LICENSE-MIT`: packaged in every release archive; [`assets/`](assets/) holds the
  README's images.

What documents it:

- [`docs/`](docs/): guides, references, [ADRs](docs/adr/), the [vision](docs/vision/README.md),
  [agent conventions](docs/agents/) and the round reports in `docs/research/`.
- [`spec/`](spec/): SpecForge's own specification, written in SpecForge; `specforge.json` is its project file.
- [`examples/`](examples/): worked example projects, checked by the gate.
- [`ARCHITECTURE.md`](ARCHITECTURE.md), [`CONTEXT.md`](CONTEXT.md) (the domain glossary), this file and
  [`AGENTS.md`](AGENTS.md).

What builds and checks it:

- [`gate/`](gate/) (`cargo gate`, the check list), [`xtask/`](xtask/) (build-blobs, revendor, check-links),
  [`scripts/`](scripts/) (shell helpers the gate and the release run).
- [`fixtures/`](fixtures/): extension fixtures the tests load; [`evaluation/`](evaluation/): the first-attempt
  accuracy eval.
- `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `clippy.toml`, `.cargo/`, `.config/` (nextest),
  `.github/` (CI, release), `.githooks/` (the pre-commit hook).
- `.claude/`, `.codex/`, `.mcp.json`: agent configuration for working on this repository.

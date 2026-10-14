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

## Where things are

- [`crates/`](crates/) — the Rust workspace: compiler, graph, extension runtime, CLI, LSP, MCP.
- [`extensions/`](extensions/) — the builtin extensions and their vendored blobs.
- [`spec/`](spec/) — SpecForge's own specification, written in SpecForge.
- [`docs/adr/`](docs/adr/) — architecture decision records.
- [`docs/agents/`](docs/agents/) — conventions for AI agents working in this repository.

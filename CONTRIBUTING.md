# Contributing

Thanks for working on SpecForge. Read [ARCHITECTURE.md](ARCHITECTURE.md) first for how the pieces fit, and [crates/README.md](crates/README.md) for what each crate owns.

## Build from source

You need the stable Rust toolchain (pinned in `rust-toolchain.toml`).

```bash
git clone https://github.com/leaderiop/SpecForge && cd SpecForge
cargo install --path crates/surfaces/cli     # installs `specforge` into ~/.cargo/bin
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

Lints are one table in the root `Cargo.toml` (`[workspace.lints]`), inherited by every crate. No `unwrap()` outside
tests: use `expect("...")` naming the invariant, or return an error for anything a user controls. An exception is
`#[expect(lint, reason = "...")]` on the item it excuses ([ADR 0089](docs/adr/0089-one-lint-table-for-the-workspace.md)).
A new test target starts with `#![allow(clippy::unwrap_used, reason = "a test's unwrap is its assertion: a None or an Err fails the test")]`
(a test file that is a module of another target's root needs none).

No tracked `.rs` file may be over 800 lines (the `file-size` check, [ADR 0085](docs/adr/0085-no-tracked-rust-file-is-over-800-lines.md)).
The files already over it are listed in `gate/baseline.json` and may only shrink: they take no new lines, so new code
and new tests go in a new module (a test binary's `tests/<file>/<topic>.rs`) or the file is split first. After you
shrink, split or move one, run `cargo gate baseline --only file-size` and commit the file with your change.

## Tests

A test builds its project with `specforge_project::testing::TestProject` (feature `testing`) and exercises it
through one driver: `compile()`/`session()` in process, `specforge_mcp::testing::Served` over MCP, or
`crates/surfaces/cli/tests/support::Cli` through the binary. Put a test in the crate that owns what it asserts;
only the binary's contract (arguments, exit codes, streams, environment) and cross-surface comparisons are CLI
tests ([ADR 0078](docs/adr/0078-a-tests-project-is-one-fixture.md)).

A test or tool that reads a file outside its own crate finds it from `specforge_repo_root::repo_root()`
(`crates/leaf/repo-root`), never by counting `..` from `CARGO_MANIFEST_DIR`: crates sit at different depths
under `crates/` ([ADR 0081](docs/adr/0081-the-repository-root-is-found-not-counted.md)).

## Adding a crate

Follow [crates/README.md](crates/README.md#adding-or-moving-a-crate): pick its layer, give it a `description` and a
crate doc, model it in `spec/product/modules.spec`, then run `cargo gate crate-map`.

## Coverage

`rustup component add llvm-tools-preview && cargo install cargo-llvm-cov`, then `cargo llvm-cov nextest
--workspace --html` (open `target/llvm-cov/html/index.html`). CI runs the same on every push to `main` and uploads
the per-file and per-crate report (`.github/workflows/coverage.yml`, `scripts/coverage-by-crate.py`). It is a
measurement: no gate reads it
([ADR 0090](docs/adr/0090-text-a-user-types-is-tested-by-properties-over-the-corpus.md)). The parser and formatter
property tests run 256 cases per run; the deep run is `PROPTEST_CASES=20000 cargo test -p specforge-parser -p
specforge-formatter --test tests` (about 4 minutes; nextest's 60 s timeout is too short for it).

## Where things are

The repository tracks what ships and what builds, tests or documents it, and nothing else
([ADR 0082](docs/adr/0082-the-repository-tracks-what-ships-and-what-builds-it.md)). `cargo gate`'s `root-layout` check keeps the top level to this list
(`gate/src/root_layout.rs`); planning rounds, strategy, archived plans and experiments live in the private planning
repository, `leaderiop/SpecForge-planning`.

What ships:

- [`crates/`](crates/): the Rust workspace, one folder per layer ([crates/README.md](crates/README.md)).
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

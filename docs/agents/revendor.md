# Re-vendoring the builtins

The builtin extensions and the test fixtures ship as wasm blobs committed under `<crate>/wasm/`, with the hash of everything
they compile from in `inputs.json`. A change to those inputs makes the check list's `builtins` check (`build-blobs --check`) fail until
the tree is re-vendored.

## When

A change to any file an `inputs.json` records: the guests' sources, the SDK and its macros, protocol-types, coverage, cycles,
the root manifest's inherited tables. `cargo gate` runs the `builtins` check exactly then, and `build-blobs --check`
names each changed input.

## How

```text
cargo run -p xtask --bin revendor
```

Run it from the main checkout, or from a worktree outside the repository directory, with `CARGO_TARGET_DIR` unset
(it refuses otherwise; the blobs build in `target/blob-stage/`). It takes about four minutes and needs
`rustup target add wasm32-wasip2`.

It refreshes the lockfile of every committed blob's crate, rebuilds every committed blob
(`specforge_component::committed::COMMITTED`: the nine builtins and the four fixtures) into `<crate>/wasm/` with its
`inputs.json`, rewrites the declaration snapshots of every SDK blob in
`crates/runtime/component/tests/declarations/` and the builtin vocabulary
(`crates/runtime/component/data/builtin-vocabulary.json`, ADR 0122), runs `build-blobs --check` and the declarations test (with its
ignored tests), and prints the `git add` line that stages exactly those paths.

A new committed blob is listed in `crates/runtime/component/src/committed.rs` first.
`every_committed_wasm_is_listed` (xtask) fails for a `.wasm` file git tracks that the list does not name.

## Profile

The blobs are built with `--profile blob` (root `Cargo.toml`: opt-level `z`, fat LTO, one codegen unit, strip), not with the
workspace's `release`: the nine builtins go from 7.0 MB to 3.1 MB, which also shrinks the CLI that embeds them. The table is
an input of every blob (`Cargo.toml#workspace-inherited`), so editing it re-vendors them. Trap frames lose function names;
to debug a guest, rebuild its crate with a plain `cargo build --release`.

## Review

Declaration snapshots should change only when the change meant to change a declaration; a snapshot diff nobody
intended is a bug. The vocabulary changes only when a builtin's kinds or enhancement fields change. The doctor and MCP snapshots (`cargo insta review`) follow the declarations.

Run it once per round or change set, after the last input changed, then `cargo gate full`.

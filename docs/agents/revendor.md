# Re-vendoring the builtins

The builtin extensions ship as wasm blobs committed under `extensions/<name>/wasm/`, with the hash of everything
they compile from in `inputs.json`. A change to those inputs makes `build-builtins --check` (and the gate) fail until
the tree is re-vendored.

## When

A change to any of: `extensions/*/src`, `crates/specforge-extension-sdk`, its macros, `specforge-protocol-types`,
`specforge-coverage`, or anything else listed in an `inputs.json`.

## How

```text
cargo run -p xtask --bin revendor
```

Run it from the main checkout, or from a worktree outside the repository directory, with `CARGO_TARGET_DIR` unset
(it refuses otherwise; the blobs build in `target/builtin-stage/`). It takes about four minutes and needs
`rustup target add wasm32-wasip2`.

It refreshes the guest lockfiles, rebuilds and vendors the nine blobs and their `inputs.json`, rebuilds
`fixtures/greet-extension/greet.wasm`, rewrites the declaration snapshots in
`crates/specforge-component/tests/declarations/`, checks the lot, and prints the `git add` line that stages exactly
those paths.

## Review

Declaration snapshots should change only when the change meant to change a declaration; a snapshot diff nobody
intended is a bug. The doctor and MCP snapshots (`cargo insta review`) follow the declarations.

Run it once per round or change set, after the last input changed, then `scripts/gate.sh full`.

# Re-vendoring the builtins

The builtin extensions and the test fixtures ship as wasm blobs committed under `<crate>/wasm/`, with the hash of everything
they compile from in `inputs.json`. A change to those inputs makes `build-blobs --check` (and the gate) fail until
the tree is re-vendored.

## When

A change to any of: `extensions/*/src`, `fixtures/*/src`, `crates/specforge-extension-sdk`, its macros, `specforge-protocol-types`,
`specforge-coverage`, or anything else listed in an `inputs.json`.

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
`crates/specforge-component/tests/declarations/`, runs `build-blobs --check` and the declarations test (with its
ignored tests), and prints the `git add` line that stages exactly those paths.

A new committed blob is listed in `crates/specforge-component/src/committed.rs` first.
`every_committed_wasm_is_listed` (xtask) fails for a `.wasm` file git tracks that the list does not name.

## Review

Declaration snapshots should change only when the change meant to change a declaration; a snapshot diff nobody
intended is a bug. The doctor and MCP snapshots (`cargo insta review`) follow the declarations.

Run it once per round or change set, after the last input changed, then `scripts/gate.sh full`.

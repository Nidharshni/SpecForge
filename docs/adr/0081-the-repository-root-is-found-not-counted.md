# The repository root is found, not counted

**Status:** accepted (2026-10-14, structure round, plan 01).

Tests and build tools read files outside their own crate: `spec/`, `examples/`, `docs/`, `schema/`, the committed
blobs, another crate's fixtures. They found the repository root by counting directories up from
`CARGO_MANIFEST_DIR` (60 sites, 8 local helpers and `specforge_component::committed::repo_root`). Nesting `crates/` by
layer made every count wrong. Nesting one crate in a trial broke 9 of its tests, and one test passed without reading
anything.

## D1. One function

`specforge_repo_root::repo_root() -> &'static Path`, in the leaf crate `specforge-repo-root` (no dependencies, not
published), is the nearest ancestor of that crate whose `Cargo.toml` has a `[workspace]` table, canonicalised. It
walks from its own crate, so no caller depends on where it sits. Tests take the crate as a dev-dependency.
`specforge-component`'s `testing` feature and `xtask` take it as a normal one. No shipped binary links it.

## D2. A crate reaches a sibling through the root, and itself through its own manifest directory

`repo_root().join("crates/<crate>/…")`, never `"../<crate>"`; `env!("CARGO_MANIFEST_DIR")` for the crate's own files.

## D3. Compile-time paths stay relative

`#[path]` accepts only a literal, and `include_bytes!` / `include_str!` cannot call a function. A wrong relative
include is a compile error naming the file, so it cannot fail silently. A move rewrites them with the Cargo path
dependencies.

## D4. Nothing that walks the tree assumes a depth

Gate path triggers write `crates/**/…`, never `crates/*/…` (a gate test rejects it). A walk over crates finds them as
directories holding a `Cargo.toml`, and it fails when it finds none.

## Rejected

- The function in `specforge-test`: that is the published user integration, and it is dev-only, while
  `committed.rs` and `xtask` are not.
- The function in `specforge-common`: it would put source-tree plumbing into a product crate, and the leaf crates
  would need dev cycles to use it.
- `[env] SPECFORGE_REPO_ROOT` in `.cargo/config.toml`: cargo reads it from the working directory, so a build with
  `--manifest-path` from elsewhere, or `cargo install --git`, would not compile.
- A `repo_path!()` proc macro for the compile-time sites: it cannot serve `#[path]`, and it needs a second crate.

## Consequences

- Moving a crate changes no test's root, and 03 rewrites only names and relative includes.
- `docs/agents/architecture-rounds.md` move tickets check `rg 'CARGO_MANIFEST_DIR' | rg '\.\.|parent'` is empty.

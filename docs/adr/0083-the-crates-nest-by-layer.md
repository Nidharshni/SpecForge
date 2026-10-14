# The crates nest by layer

**Status:** accepted (2026-10-14, structure round, plan 03).

Thirty-one crates sat side by side under `crates/`, so nothing in the tree showed which crates are the vocabulary,
which the pipeline, and which the surfaces. The dependency graph already had that order: with eight layers, none
of its 118 normal edges points upward.

## D1. A crate's folder is `crates/<layer>/<name>`

The layers, bottom to top: `leaf kernel pipeline runtime session app adapters surfaces`. The folder name is the
package name without `specforge-`; `tree-sitter-specforge` is `crates/kernel/tree-sitter`. Package names, and so
every `use` path and every `-p` flag, are unchanged. Nothing else lives directly under `crates/` or a layer folder
except documentation.

| Package | Folder |
|---|---|
| specforge-coverage, -diagnostics, -package, -protocol-types, -repo-root | `crates/leaf/{coverage,diagnostics,package,protocol-types,repo-root}` |
| specforge-common, -parser, -shape-macros; tree-sitter-specforge | `crates/kernel/{common,parser,shape-macros,tree-sitter}` |
| specforge-emitter, -formatter, -graph, -migrate, -registry, -resolver | `crates/pipeline/{emitter,formatter,graph,migrate,registry,resolver}` |
| specforge-component, -extension-sdk, -extension-sdk-macros, -installed, -wasm | `crates/runtime/{component,extension-sdk,extension-sdk-macros,installed,wasm}` |
| specforge-project, -watch | `crates/session/{project,watch}` |
| specforge-ops | `crates/app/ops` |
| specforge-ops-registry, -registry-client, -registry-wire | `crates/adapters/{ops-registry,registry-client,registry-wire}` |
| specforge-cli, -lsp, -mcp, -mcp-macros, -registry-server | `crates/surfaces/{cli,lsp,mcp,mcp-macros,registry-server}` |

`xtask`, `gate`, `integrations/rust/specforge-test(-macros)`, the builtins under `extensions/` and the fixtures
under `fixtures/` are in no layer and did not move.

## D2. The workspace lists its members by layer

`Cargo.toml` names each member explicitly, grouped under a comment per layer. A glob (`crates/*/*`) would make any
leftover directory two levels down a member.

## D3. Paths in dated records follow the files

A citation of a crate file in an ADR or a round report was rewritten to the file's new path, because the
statement around it is unchanged. A citation of a crate that no longer exists stays as written. An ADR whose
decision changes is amended, not rewritten.

## Consequences

- Code that finds the repository root does not count directories (ADR 0081).
- The layer rule (the layer-check ADR of this round) reads a crate's layer from its folder.
- Re-vendoring followed the move: the blobs' records name the new paths.
- `cargo install --path crates/surfaces/cli`.

# One graph build for a compile and every update; the resolver returns its diagnostics

**Status:** accepted (2026-10-07). Amends [ADR 0006](0006-one-incremental-rebuild.md).

ADR 0006 put the incremental rebuild in `specforge-project`. Its graph half reached into
`specforge-graph` through five items public for that one caller (`entity_pass` with an optional sink,
`link_and_diagnose`, `node_from_entity`, `is_define_block`, `Graph::remove_entities_of_file`) and wrote
the first-writer-wins rule a second time, held in step with the cold build by a path-order comment.
The cold build was assembled three times (a compile, a session's open, an extension command's graph).
Sources were read by the resolver for a cold build and by the session for an update, which disagreed on
a file that can't be read: E025 in `check`, silently gone after any update in watch, the LSP and MCP.
`--verify-incremental` compared nodes and edges only, so it passed while the diagnostics differed, and
the LSP never ran it. The resolver returned file scopes, re-exports, import targets and a topological
order that nothing outside it read.

- **One graph build.** `specforge_graph::GraphBuild` holds a set of parsed files, their graph and the
  graph-build diagnostics, and applies whole-file changes (`FileChange::Parsed` / `Removed`). A cold
  build is every file applied at once (`GraphBuild::of`); an update strips the changed files' nodes,
  places the first declaration of each ID they touch, re-links the whole graph and recomputes the
  diagnostics. One sweep over the files in path order decides both which declaration of an ID is its
  node (the first that is not a `define` block) and E002/W060/W143. `build_graph` takes files in path
  order whatever order it is given.
- **One delta, owned by the build.** `GraphDelta` moves to `specforge-graph` (still
  `specforge_project::GraphDelta` for its readers); `apply` returns it.
- **Verification is the build's own.** With verification on, every apply is compared with a cold build
  of the same parses: nodes (kind, file, position, title, fields, methods), edges, graph-build
  diagnostics in order, the delta against a full comparison, and the delta applied to the previous
  graph. Every project session verifies in a debug build; watch's `--verify-incremental` turns it on in
  release. Watch prints a divergence; the LSP logs it; a debug LSP or MCP stops on it.
- **One read.** `specforge-project` reads every source, for a compile and for every update, through one
  function. A file that is there but can't be read is E025 naming it, left out of the graph, and kept by
  the session until it is readable or gone.
- **One cold pipeline.** `Environment::build_sources` reads what discovery found, builds the graph and
  resolves the imports; `CompiledProject::compile`, `OpeningProject::finish` and
  `Environment::build_graph` all use it. *(ADR 0047: its result is a `CompiledProject`; a session holds
  one, so `OpeningProject::finish` and `CompiledProject::compile` read through `CompiledProject::read`.)*
- **The resolver returns what is read.** `resolve_imports(spec_root, files, exists)` gives E025, I004,
  W113 and W027; `resolve_import` names one import's file for go-to-definition. File scopes, re-exports
  and the import order stay inside it, where W027 and W113 need them. Path aliases are removed:
  `specforge.json` never could set one, so `@scope/name` is always an extension import (I004).
- A session reports graph-build diagnostics in build order, the order `specforge check` reports them.
- Rename's text edits (`RenameEdit`) live with the rename operation in `specforge-ops`.

**Rejected.** Rebuilding every node from the cached parses on each update (no patch): nearly as cheap,
since every update already sweeps every file and re-links the graph, but it re-decides ADR 0006's
red-green update and makes the delta compare every node. Keeping `GraphDelta` in `specforge-project`
with the build returning what it touched: a shallow record of the diff's inputs, and the verification
split across two crates again.

**What would reopen it.** A `paths` key in `specforge.json` brings path aliases back, as configuration
`resolve_imports` reads. A profile showing the cold read should keep tree-sitter trees for the first
edit.

## The registry build derives the graph's inputs (amendment, architecture round 6, plan 08)

The graph build reads six things of the loaded extensions. Four were public fields of the registry
build, computed for one caller (`Environment::graph_config`); two (`field_coercions`,
`derived_references`) were derived in `specforge-project` from the build's field registry.

- **One value.** `specforge_graph::GraphInputs` holds the six; `GraphConfig` is
  `{ known_provider_schemes, inputs: GraphInputs }`, the schemes being the environment's.
- **One producer.** `RegistryBuild::graph_inputs()` derives all six from the build's declarations and
  field registry (`specforge-registry` now depends on `specforge-graph`; nothing below the graph
  depends on the registry). `specforge-project`'s `field_types` module is gone, and so are the four
  fields.
- **Deterministic.** Inverse pairs are written (lesser, greater) and sorted; the graph reads a pair
  symmetrically, so no graph changes. *(ADR 0060: a pair is a reference field and one of its inverse
  fields, fields of its target kind, `FieldRegistry::inverse_fields`.)*

Rejected: the registry build taking the provider schemes (`graph_config(schemes)`), which ADR 0031 D7
rejected for `build_registries`; the graph crate reading the registries, which would make every graph
test build one.
